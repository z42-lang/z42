use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
use super::convert::arg_char;

/// True for Unicode whitespace characters (space, tab, CR, LF, NBSP, etc.).
/// args: [this: char]
/// New in simplify-string-stdlib (2026-04-24): backs script-side Trim/TrimStart/TrimEnd.
pub fn builtin_char_is_whitespace(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let c = arg_char(args, 0, "__char_is_whitespace")?;
    Ok(Value::Bool(c.is_whitespace()))
}

/// ASCII-rule lowercase conversion — non-letters return themselves unchanged.
/// args: [this: char]
/// New in simplify-string-stdlib (2026-04-24): backs script-side string.ToLower().
/// Locale-sensitive casing (Turkish I etc.) deferred to L3 CultureInfo.
pub fn builtin_char_to_lower(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let c = arg_char(args, 0, "__char_to_lower")?;
    Ok(Value::Char(c.to_ascii_lowercase()))
}

/// ASCII-rule uppercase conversion — non-letters return themselves unchanged.
/// args: [this: char]
/// New in simplify-string-stdlib (2026-04-24): backs script-side string.ToUpper().
pub fn builtin_char_to_upper(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let c = arg_char(args, 0, "__char_to_upper")?;
    Ok(Value::Char(c.to_ascii_uppercase()))
}
