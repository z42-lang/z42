//! VM-independent authoring helpers for native module registration and script calls.
use std::ffi::c_void;
pub use z42_abi::native as abi;

#[derive(Debug)]
pub enum Value {
    Void,
    Null,
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Strings(Vec<String>),
}

#[derive(Debug)]
pub enum Error {
    Exception {
        class: &'static str,
        message: String,
    },
    ScriptThrown,
    Host,
}
impl Error {
    pub fn new(class: &'static str, message: impl Into<String>) -> Self {
        Self::Exception {
            class,
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new("Std.InvalidMarshalException", message)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

/// Valid only inside dispatch. Providers must not save this object or its raw pointer.
pub struct Call<'a> {
    raw: &'a mut abi::Call,
}
impl Call<'_> {
    pub fn capabilities(&self) -> u64 {
        unsafe { (*self.raw.host).capabilities }
    }
    pub fn argument(&mut self, index: usize) -> Result<Value> {
        let mut value = abi::Value::null();
        let status = unsafe { ((*self.raw.host).argument)(self.raw, index, &mut value) };
        if status != abi::OK {
            return Err(Error::invalid(format!(
                "cannot read native argument {index}"
            )));
        }
        unsafe { decode(&value) }
    }
    pub fn integer(&mut self, index: usize) -> Result<i64> {
        match self.argument(index)? {
            Value::I64(value) => Ok(value),
            _ => Err(Error::invalid("expected an integer")),
        }
    }
    pub fn text(&mut self, index: usize) -> Result<String> {
        match self.argument(index)? {
            Value::Str(value) => Ok(value),
            _ => Err(Error::invalid("expected a string")),
        }
    }
    pub fn optional_text(&mut self, index: usize) -> Result<Option<String>> {
        match self.argument(index)? {
            Value::Null => Ok(None),
            Value::Str(value) => Ok(Some(value)),
            _ => Err(Error::invalid("expected a nullable string")),
        }
    }
    pub fn bytes(&mut self, index: usize) -> Result<Vec<u8>> {
        match self.argument(index)? {
            Value::Bytes(value) => Ok(value),
            _ => Err(Error::invalid("expected bytes")),
        }
    }
    pub fn boolean(&mut self, index: usize) -> Result<bool> {
        match self.argument(index)? {
            Value::Bool(value) => Ok(value),
            _ => Err(Error::invalid("expected a bool")),
        }
    }

    /// Borrow this call only while invoking an existing synchronous C callback API.
    pub fn raw(&mut self) -> *mut abi::Call {
        self.raw
    }

    /// # Safety
    /// T must be the state created by this module's create_state entry, and may not escape.
    pub unsafe fn state<T>(&self) -> Result<&T> {
        unsafe { self.raw.state.cast::<T>().as_ref() }
            .ok_or_else(|| Error::invalid("missing native module state"))
    }

    pub fn invoke(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        let encoded: Vec<_> = arguments.iter().map(Encoded::new).collect();
        let raw: Vec<_> = encoded.iter().map(|value| value.raw).collect();
        let mut output = abi::Value::null();
        let status = unsafe {
            ((*self.raw.host).invoke_script)(
                self.raw,
                abi::Text::new(name),
                raw.as_ptr(),
                raw.len(),
                &mut output,
            )
        };
        match status {
            abi::OK => unsafe { decode(&output) },
            abi::THROWN => Err(Error::ScriptThrown),
            _ => Err(Error::new(
                "Std.Exception",
                format!("native callback `{name}` failed"),
            )),
        }
    }

    /// # Safety
    /// T must be the symbol's actual function-pointer type. The host retains its image.
    pub unsafe fn symbol<T: Copy>(&mut self, library: &str, symbol: &str) -> Result<T> {
        if std::mem::size_of::<T>() != std::mem::size_of::<*mut c_void>() {
            return Err(Error::invalid("native symbol type is not a pointer"));
        }
        let mut pointer = std::ptr::null_mut();
        let status = unsafe {
            ((*self.raw.host).resolve_symbol)(
                self.raw,
                abi::Text::new(library),
                abi::Text::new(symbol),
                &mut pointer,
            )
        };
        if status != abi::OK || pointer.is_null() {
            return Err(Error::new(
                "Std.NotSupportedException",
                format!("native entry `{library}::{symbol}` is unavailable"),
            ));
        }
        Ok(unsafe { std::mem::transmute_copy(&pointer) })
    }
}

/// Catch Rust panics before crossing the C boundary; borrow the host only for this call.
///
/// # Safety
/// raw must refer to a live host call using the matching ABI for this synchronous span.
pub unsafe fn dispatch(
    raw: *mut abi::Call,
    body: impl FnOnce(&mut Call<'_>) -> Result<Value>,
) -> u32 {
    let Some(raw) = (unsafe { raw.as_mut() }) else {
        return abi::ERROR;
    };
    if raw.host.is_null() {
        return abi::ERROR;
    }
    let host = unsafe { &*raw.host };
    if host.version != abi::MODULE_ABI_VERSION
        || host.size < std::mem::size_of::<abi::HostApi>() as u32
    {
        return abi::ERROR;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut Call { raw })));
    match result {
        Ok(Ok(value)) => {
            let encoded = Encoded::new(&value);
            unsafe { (host.return_value)(raw, &encoded.raw) }
        }
        Ok(Err(Error::ScriptThrown)) => abi::THROWN,
        Ok(Err(Error::Host)) => abi::ERROR,
        Ok(Err(Error::Exception { class, message })) => unsafe {
            (host.raise)(raw, abi::Text::new(class), abi::Text::new(&message))
        },
        Err(_) => unsafe {
            (host.raise)(
                raw,
                abi::Text::new("Std.Exception"),
                abi::Text::new("panic in native module"),
            )
        },
    }
}

struct Encoded {
    raw: abi::Value,
    // Keep the C array backing alive while the host copies it.
    _strings: Vec<abi::Value>,
}
impl Encoded {
    fn new(value: &Value) -> Self {
        let mut strings = Vec::new();
        let mut raw = abi::Value::null();
        match value {
            Value::Void => raw.kind = abi::VOID,
            Value::Null => {}
            Value::I64(value) => raw = abi::Value::scalar(abi::I64, *value as u64),
            Value::F64(value) => raw = abi::Value::scalar(abi::F64, value.to_bits()),
            Value::Bool(value) => raw = abi::Value::scalar(abi::BOOL, u64::from(*value)),
            Value::Str(value) => {
                raw.kind = abi::STR;
                raw.data = value.as_ptr();
                raw.len = value.len();
            }
            Value::Bytes(value) => {
                raw.kind = abi::BYTES;
                raw.data = value.as_ptr();
                raw.len = value.len();
            }
            Value::Strings(values) => {
                strings = values
                    .iter()
                    .map(|value| abi::Value {
                        kind: abi::STR,
                        reserved: 0,
                        bits: 0,
                        data: value.as_ptr(),
                        len: value.len(),
                    })
                    .collect();
                raw.kind = abi::STR_ARRAY;
                raw.data = strings.as_ptr().cast();
                raw.len = strings.len();
            }
        }
        Self {
            raw,
            _strings: strings,
        }
    }
}

unsafe fn span<'a, T>(data: *const T, len: usize) -> Result<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null()
        || (data as usize) % std::mem::align_of::<T>() != 0
        || len > isize::MAX as usize / std::mem::size_of::<T>()
    {
        return Err(Error::invalid("invalid native value span"));
    }
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}

/// Copy a host value during the synchronous call that owns its spans.
///
/// # Safety
/// Every nonempty span must point to live readable data for its declared kind.
pub unsafe fn decode(value: &abi::Value) -> Result<Value> {
    match value.kind {
        abi::VOID => Ok(Value::Void),
        abi::NULL => Ok(Value::Null),
        abi::I64 => Ok(Value::I64(value.bits as i64)),
        abi::F64 => Ok(Value::F64(f64::from_bits(value.bits))),
        abi::BOOL => Ok(Value::Bool(value.bits != 0)),
        abi::STR => Ok(Value::Str(
            std::str::from_utf8(unsafe { span(value.data, value.len)? })
                .map_err(|_| Error::invalid("native string is not UTF-8"))?
                .to_owned(),
        )),
        abi::BYTES => Ok(Value::Bytes(
            unsafe { span(value.data, value.len)? }.to_vec(),
        )),
        abi::STR_ARRAY => {
            let mut output = Vec::new();
            for item in unsafe { span(value.data.cast::<abi::Value>(), value.len)? } {
                if item.kind != abi::STR {
                    return Err(Error::invalid("expected string array element"));
                }
                match unsafe { decode(item)? } {
                    Value::Str(text) => output.push(text),
                    _ => return Err(Error::invalid("expected string array element")),
                }
            }
            Ok(Value::Strings(output))
        }
        _ => Err(Error::invalid(format!(
            "unknown native value kind {}",
            value.kind
        ))),
    }
}

/// Forward a stateless lightweight registration to the same module's full native image.
///
/// # Safety
/// raw is a live host call. The loaded module must be stateless; this helper rejects
/// descriptors with state hooks so no Rust state can cross between independently built images.
pub unsafe fn forward(raw: *mut abi::Call, library: &str, entry: &str, function: &str) -> u32 {
    if raw.is_null() || unsafe { (*raw).host.is_null() } {
        return abi::ERROR;
    }
    let host = unsafe { &*(*raw).host };
    if host.version != abi::MODULE_ABI_VERSION
        || host.size < std::mem::size_of::<abi::HostApi>() as u32
    {
        return abi::ERROR;
    }
    let target = (|| -> Result<abi::Invoke> {
        let call = unsafe { raw.as_mut() }.ok_or_else(|| Error::invalid("null native call"))?;
        let mut call = Call { raw: call };
        let entry: abi::ModuleEntry = unsafe { call.symbol(library, entry)? };
        let module = unsafe { entry().as_ref() }
            .ok_or_else(|| Error::invalid("null native module descriptor"))?;
        if module.version != abi::MODULE_ABI_VERSION
            || module.size < std::mem::size_of::<abi::Module>() as u32
        {
            return Err(Error::invalid("native module ABI mismatch"));
        }
        if module.create_state.is_some() || module.destroy_state.is_some() {
            return Err(Error::invalid(
                "stateless forwarding cannot share module state",
            ));
        }
        for candidate in unsafe { span(module.functions, module.function_count)? } {
            let name = unsafe { span(candidate.name.data, candidate.name.len)? };
            if name == function.as_bytes() {
                return candidate
                    .invoke
                    .ok_or_else(|| Error::invalid("missing native invoke entry"));
            }
        }
        Err(Error::invalid(format!(
            "native module does not export `{function}`"
        )))
    })();
    match target {
        Ok(invoke) => unsafe { invoke(raw) },
        Err(error) => unsafe { dispatch(raw, |_| Err(error)) },
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
