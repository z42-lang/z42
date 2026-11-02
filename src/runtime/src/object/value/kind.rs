use super::Value;

/// 值的用户可读种类名（诊断消息用；**不是** Rust Debug 格式）。
pub fn value_kind_name(v: &Value) -> &'static str {
    match v {
        Value::I64(_) => "整数",
        Value::F64(_) => "浮点数",
        Value::Bool(_) => "bool",
        Value::Char(_) => "char",
        Value::Str(_) => "string",
        Value::Null => "null",
        Value::Array(_) => "数组",
        Value::Object(_) => "对象",
        Value::BoxedStruct(_) => "装箱值类型",
        _ => "值",
    }
}
