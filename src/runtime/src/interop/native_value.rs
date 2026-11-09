//! Copies at the native boundary keep all provider buffers independent of GC movement.
use crate::exec::call::builtin_abi::{ArgAt, BuiltinError, CallResult, FromArg, IntoRet};
use crate::{context::VmContext, object::Value};
use z42_abi::native as abi;
pub(crate) use z42_rs::native::Value as Owned;

pub(crate) fn from_vm(
    value: &Value,
    kind: u32,
    function: &'static str,
    index: usize,
) -> Result<Owned, BuiltinError> {
    let at = ArgAt {
        builtin: function,
        index,
        param: "argument",
        receiver: None,
    };
    Ok(match kind {
        abi::I64 => Owned::I64(i64::from_arg(value, &at)?),
        abi::F64 => Owned::F64(f64::from_arg(value, &at)?),
        abi::BOOL => Owned::Bool(bool::from_arg(value, &at)?),
        abi::STR => Owned::Str(String::from_arg(value, &at)?),
        abi::OPT_STR => match Option::<String>::from_arg(value, &at)? {
            Some(text) => Owned::Str(text),
            None => Owned::Null,
        },
        abi::BYTES => Owned::Bytes(Vec::<u8>::from_arg(value, &at)?),
        abi::STR_ARRAY => Owned::Strings(Vec::<String>::from_arg(value, &at)?),
        _ => {
            return Err(BuiltinError::Internal(
                "invalid registered native parameter kind".into(),
            ));
        }
    })
}

pub(crate) fn to_vm(ctx: &VmContext, value: Owned) -> CallResult {
    match value {
        Owned::Void => Ok(None),
        Owned::Null => Ok(Some(Value::Null)),
        Owned::I64(value) => value.into_ret(ctx),
        Owned::F64(value) => value.into_ret(ctx),
        Owned::Bool(value) => value.into_ret(ctx),
        Owned::Str(value) => value.into_ret(ctx),
        Owned::Bytes(value) => value.into_ret(ctx),
        Owned::Strings(value) => value.into_ret(ctx),
    }
}

pub(crate) fn from_callback(value: &Value) -> Result<Owned, BuiltinError> {
    let kind = match value {
        Value::Null => return Ok(Owned::Null),
        Value::I64(_) => abi::I64,
        Value::F64(_) => abi::F64,
        Value::Bool(_) => abi::BOOL,
        Value::Str(_) => abi::STR,
        Value::Array(_) => abi::STR_ARRAY,
        _ => {
            return Err(BuiltinError::InvalidCast(
                "native callback returned an unsupported value".into(),
            ));
        }
    };
    from_vm(value, kind, "native callback", 0)
}

pub(crate) fn error(error: z42_rs::native::Error) -> BuiltinError {
    match error {
        z42_rs::native::Error::Exception { class, message } => {
            BuiltinError::Exception { class, message }
        }
        other => BuiltinError::Internal(format!("unexpected native codec error: {other:?}")),
    }
}

#[derive(Default)]
pub(crate) struct Arena {
    owned: Vec<Box<Owned>>,
    arrays: Vec<Box<[abi::Value]>>,
}
impl Arena {
    pub fn insert(&mut self, value: Owned) -> abi::Value {
        let value = Box::new(value);
        let raw = encode(&value, &mut self.arrays);
        self.owned.push(value);
        raw
    }
    pub fn view(&mut self, value: &Owned) -> abi::Value {
        encode(value, &mut self.arrays)
    }
}
fn encode(value: &Owned, arrays: &mut Vec<Box<[abi::Value]>>) -> abi::Value {
    let mut raw = abi::Value::null();
    match value {
        Owned::Void => raw.kind = abi::VOID,
        Owned::Null => {}
        Owned::I64(value) => raw = abi::Value::scalar(abi::I64, *value as u64),
        Owned::F64(value) => raw = abi::Value::scalar(abi::F64, value.to_bits()),
        Owned::Bool(value) => raw = abi::Value::scalar(abi::BOOL, u64::from(*value)),
        Owned::Str(value) => {
            raw.kind = abi::STR;
            raw.data = value.as_ptr();
            raw.len = value.len();
        }
        Owned::Bytes(value) => {
            raw.kind = abi::BYTES;
            raw.data = value.as_ptr();
            raw.len = value.len();
        }
        Owned::Strings(values) => {
            let span: Box<[_]> = values
                .iter()
                .map(|text| abi::Value {
                    kind: abi::STR,
                    reserved: 0,
                    bits: 0,
                    data: text.as_ptr(),
                    len: text.len(),
                })
                .collect();
            raw.kind = abi::STR_ARRAY;
            raw.data = span.as_ptr().cast();
            raw.len = span.len();
            arrays.push(span);
        }
    }
    raw
}
