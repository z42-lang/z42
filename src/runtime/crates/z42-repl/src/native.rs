//! Script-facing REPL registration and callbacks. This adapter has no VM dependency;
//! terminal editing/history stays in the optional backend, with plain input available alone.
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Mutex, OnceLock};
use z42_rs::native::{self, Error, Value, abi};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EditorCallbacks {
    pub ctx: *mut c_void,
    pub complete: extern "C" fn(*mut c_void, *const c_char, usize) -> *mut c_char,
    pub key_edit: extern "C" fn(*mut c_void, *const c_char, *const c_char, usize) -> *mut c_char,
    pub free_str: extern "C" fn(*mut c_char),
}

#[derive(Default)]
struct Session {
    completer: Mutex<Option<String>>,
    key_editor: Mutex<Option<String>>,
    keywords: Mutex<String>,
    editor: OnceLock<Editor>,
}
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Session::default())).cast()
}
unsafe extern "C" fn destroy(state: *mut c_void) {
    if !state.is_null() {
        drop(unsafe { Box::from_raw(state.cast::<Session>()) });
    }
}

fn lock<T>(cell: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(|poison| poison.into_inner())
}
fn configured(name: String) -> Option<String> {
    (!name.is_empty()).then_some(name)
}

fn terminal_supported(call: &native::Call<'_>) -> bool {
    call.capabilities() & abi::CAP_TERMINAL != 0
        && (cfg!(feature = "backend") || call.capabilities() & abi::CAP_NATIVE_LOADING != 0)
}
unsafe extern "C" fn supports_terminal(raw: *mut abi::Call) -> u32 {
    unsafe { native::dispatch(raw, |call| Ok(Value::Bool(terminal_supported(call)))) }
}
unsafe extern "C" fn supports_history(raw: *mut abi::Call) -> u32 {
    unsafe {
        native::dispatch(raw, |call| {
            Ok(Value::Bool(
                terminal_supported(call) && call.capabilities() & abi::CAP_FILESYSTEM != 0,
            ))
        })
    }
}

unsafe extern "C" fn set_completer(raw: *mut abi::Call) -> u32 {
    unsafe {
        native::dispatch(raw, |call| {
            let name = call.text(0)?;
            *lock(&call.state::<Session>()?.completer) = configured(name);
            Ok(Value::Void)
        })
    }
}
unsafe extern "C" fn set_key_editor(raw: *mut abi::Call) -> u32 {
    unsafe {
        native::dispatch(raw, |call| {
            let name = call.text(0)?;
            *lock(&call.state::<Session>()?.key_editor) = configured(name);
            Ok(Value::Void)
        })
    }
}
unsafe extern "C" fn set_keywords(raw: *mut abi::Call) -> u32 {
    unsafe {
        native::dispatch(raw, |call| {
            let words = call.text(0)?;
            *lock(&call.state::<Session>()?.keywords) = words.clone();
            if let Ok(editor) = editor(call) {
                if let Ok(words) = CString::new(words) {
                    (editor.set_keywords)(words.as_ptr());
                }
            }
            Ok(Value::Void)
        })
    }
}
unsafe extern "C" fn complete_probe(raw: *mut abi::Call) -> u32 {
    unsafe {
        native::dispatch(raw, |call| {
            let name = call.text(0)?;
            let line = call.text(1)?;
            let position = call.integer(2)?;
            call.invoke(&name, &[Value::Str(line), Value::I64(position)])
        })
    }
}

type Readline =
    unsafe extern "C" fn(*const c_char, *const EditorCallbacks, *mut i32) -> *mut c_char;
type Free = unsafe extern "C" fn(*mut c_char);
type LastError = extern "C" fn() -> *const c_char;
type SetKeywords = unsafe extern "C" fn(*const c_char);
#[derive(Clone, Copy)]
struct Editor {
    readline: Readline,
    free: Free,
    last_error: LastError,
    set_keywords: SetKeywords,
}

fn editor(call: &mut native::Call<'_>) -> native::Result<Editor> {
    if !terminal_supported(call) {
        return Err(Error::new(
            "Std.NotSupportedException",
            "terminal editor is unavailable",
        ));
    }
    if let Some(editor) = unsafe { call.state::<Session>()? }.editor.get() {
        return Ok(*editor);
    }
    #[cfg(feature = "backend")]
    let loaded = Editor {
        readline: crate::backend::z42_repl_readline,
        free: crate::backend::z42_repl_free,
        last_error: crate::backend::z42_repl_last_error,
        set_keywords: crate::backend::z42_repl_set_keywords,
    };
    #[cfg(not(feature = "backend"))]
    let loaded = unsafe {
        let version: extern "C" fn() -> u32 = call.symbol("z42_repl", "z42_repl_abi_version")?;
        if version() != 1 {
            return Err(Error::new(
                "Std.NotSupportedException",
                "REPL editor ABI mismatch",
            ));
        }
        Editor {
            readline: call.symbol("z42_repl", "z42_repl_readline")?,
            free: call.symbol("z42_repl", "z42_repl_free")?,
            last_error: call.symbol("z42_repl", "z42_repl_last_error")?,
            set_keywords: call.symbol("z42_repl", "z42_repl_set_keywords")?,
        }
    };
    let state = unsafe { call.state::<Session>()? };
    let _ = state.editor.set(loaded);
    state
        .editor
        .get()
        .copied()
        .ok_or_else(|| Error::new("Std.Exception", "REPL editor publication failed"))
}

unsafe extern "C" fn readline(raw: *mut abi::Call) -> u32 {
    unsafe {
        native::dispatch(raw, |call| {
            let prompt = call.optional_text(0)?.unwrap_or_default();
            let editor = match editor(call) {
                Ok(editor) => editor,
                Err(_) => return plain_readline(&prompt),
            };
            if let Ok(words) = CString::new(lock(&call.state::<Session>()?.keywords).as_str()) {
                (editor.set_keywords)(words.as_ptr());
            }
            let prompt = CString::new(prompt)
                .map_err(|_| Error::new("Std.ArgumentException", "prompt contains NUL"))?;
            let callbacks = EditorCallbacks {
                ctx: call.raw().cast(),
                complete,
                key_edit,
                free_str,
            };
            let mut kind = 3;
            let output = (editor.readline)(prompt.as_ptr(), &callbacks, &mut kind);
            match kind {
                0 => {
                    let line = if output.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(output).to_string_lossy().into_owned()
                    };
                    (editor.free)(output);
                    Ok(Value::Str(line))
                }
                1 => Ok(Value::Null),
                2 => Ok(Value::Str(String::new())),
                4 => plain_readline(prompt.to_str().unwrap_or_default()),
                _ => {
                    let message = (editor.last_error)();
                    let message = if message.is_null() {
                        "editor failed".into()
                    } else {
                        CStr::from_ptr(message).to_string_lossy().into_owned()
                    };
                    Err(Error::new("Std.IO.IOException", message))
                }
            }
        })
    }
}

fn plain_readline(prompt: &str) -> native::Result<Value> {
    use std::io::Write;
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(prompt.as_bytes());
    let _ = stderr.flush();
    let mut line = String::new();
    let size = std::io::stdin()
        .read_line(&mut line)
        .map_err(|error| Error::new("Std.IO.IOException", error.to_string()))?;
    Ok(if size == 0 {
        Value::Null
    } else {
        Value::Str(line.trim_end_matches(['\n', '\r']).to_owned())
    })
}

// The editor calls synchronously on the same thread. Only generic host APIs re-enter the VM;
// this crate never dereferences a VmContext or transfers Rust state through a native image.
extern "C" fn complete(context: *mut c_void, line: *const c_char, position: usize) -> *mut c_char {
    callback(context, |call| {
        if line.is_null() {
            return Ok(None);
        }
        let name = unsafe { call.state::<Session>()? }
            .completer
            .lock()
            .ok()
            .and_then(|name| name.clone());
        let Some(name) = name else {
            return Ok(None);
        };
        let line = unsafe { CStr::from_ptr(line) }
            .to_string_lossy()
            .into_owned();
        match call.invoke(&name, &[Value::Str(line), Value::I64(position as i64)])? {
            Value::Strings(values) => Ok(Some(values.join("\n"))),
            _ => Ok(None),
        }
    })
}
extern "C" fn key_edit(
    context: *mut c_void,
    key: *const c_char,
    line: *const c_char,
    position: usize,
) -> *mut c_char {
    callback(context, |call| {
        if key.is_null() || line.is_null() {
            return Ok(None);
        }
        let name = unsafe { call.state::<Session>()? }
            .key_editor
            .lock()
            .ok()
            .and_then(|name| name.clone());
        let Some(name) = name else {
            return Ok(None);
        };
        let key = unsafe { CStr::from_ptr(key) }
            .to_string_lossy()
            .into_owned();
        let line = unsafe { CStr::from_ptr(line) }
            .to_string_lossy()
            .into_owned();
        match call.invoke(
            &name,
            &[
                Value::Str(key),
                Value::Str(line),
                Value::I64(position as i64),
            ],
        )? {
            Value::Str(value) => Ok(Some(value)),
            _ => Ok(None),
        }
    })
}

fn callback(
    context: *mut c_void,
    body: impl FnOnce(&mut native::Call<'_>) -> native::Result<Option<String>>,
) -> *mut c_char {
    if context.is_null() {
        return std::ptr::null_mut();
    }
    let mut output = None;
    // dispatch catches panics and uses the call's host services. It may stage an error;
    // readline's eventual successful return replaces swallowed editing errors.
    let _ = unsafe {
        native::dispatch(context.cast(), |call| {
            output = body(call)?;
            Ok(Value::Void)
        })
    };
    output
        .filter(|text| !text.is_empty())
        .and_then(|text| CString::new(text).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}
extern "C" fn free_str(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

macro_rules! descriptor {
    ($name:literal, [$($parameter:expr),*], $result:expr, $flags:expr, $invoke:ident) => {
        abi::Function { name: abi::Text::new($name), parameters: abi::Parameters::new(&[$($parameter),*]),
            result: $result, flags: $flags, invoke: Some($invoke) }
    };
}
static FUNCTIONS: &[abi::Function] = &[
    descriptor!(
        "__repl_terminal_supported",
        [],
        abi::BOOL,
        0,
        supports_terminal
    ),
    descriptor!(
        "__repl_history_supported",
        [],
        abi::BOOL,
        0,
        supports_history
    ),
    descriptor!(
        "__repl_set_completer",
        [abi::STR],
        abi::VOID,
        0,
        set_completer
    ),
    descriptor!(
        "__repl_set_key_editor",
        [abi::STR],
        abi::VOID,
        0,
        set_key_editor
    ),
    descriptor!(
        "__repl_set_keywords",
        [abi::STR],
        abi::VOID,
        0,
        set_keywords
    ),
    descriptor!(
        "__repl_complete_probe",
        [abi::STR, abi::STR, abi::I64],
        abi::STR_ARRAY,
        abi::REENTERS,
        complete_probe
    ),
    descriptor!(
        "__repl_readline",
        [abi::OPT_STR],
        abi::OPT_STR,
        abi::BLOCKING | abi::REENTERS,
        readline
    ),
];
static MODULE: abi::Module = abi::Module {
    version: abi::MODULE_ABI_VERSION,
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("z42_repl"),
    functions: FUNCTIONS.as_ptr(),
    function_count: FUNCTIONS.len(),
    create_state: Some(create),
    destroy_state: Some(destroy),
};

#[unsafe(no_mangle)]
pub extern "C" fn z42_repl_module_v1() -> *const abi::Module {
    &MODULE
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
