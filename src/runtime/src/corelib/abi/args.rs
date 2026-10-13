//! Argument side of the builtin ABI: where arguments come from ([`ArgSource`]) and how
//! one becomes a typed Rust parameter ([`FromArg`]).

use super::{BuiltinError, ParamKind};
use crate::metadata::Value;
use crate::vm_context::VmContext;

/// The arguments of one builtin call.
pub trait ArgSource {
    fn argc(&self) -> usize;
    /// Argument `i` (`i < argc()`).
    fn arg(&self, i: usize) -> &Value;
}

impl ArgSource for [Value] {
    #[inline(always)]
    fn argc(&self) -> usize { self.len() }
    #[inline(always)]
    fn arg(&self, i: usize) -> &Value { &self[i] }
}

/// What a missing register reads as (malformed bytecode only; debug builds assert).
static NULL_ARG: Value = Value::Null;

/// Arguments read in place from the caller's register file: an optional receiver that is
/// not in a register (the vcall short-circuit already holds it), then `regs[idx[i]]`.
/// Nothing is copied until a parameter asks for an owned value.
pub struct RegArgs<'a> {
    this: Option<Value>,
    regs: &'a [Value],
    idx: &'a [u32],
}

impl<'a> RegArgs<'a> {
    #[inline(always)]
    pub fn new(this: Option<Value>, regs: &'a [Value], idx: &'a [u32]) -> Self {
        RegArgs { this, regs, idx }
    }
}

impl ArgSource for RegArgs<'_> {
    #[inline(always)]
    fn argc(&self) -> usize { self.idx.len() + self.this.is_some() as usize }
    #[inline(always)]
    fn arg(&self, i: usize) -> &Value {
        let r = match &self.this {
            Some(t) => {
                if i == 0 { return t; }
                self.idx[i - 1]
            }
            None => self.idx[i],
        };
        debug_assert!((r as usize) < self.regs.len(), "builtin argument register %{r} out of range");
        self.regs.get(r as usize).unwrap_or(&NULL_ARG)
    }
}

/// Where an argument sits — for the error text.
pub struct ArgAt {
    /// The builtin's name (`__int32_parse`).
    pub builtin: &'static str,
    pub index: usize,
    /// The Rust parameter name — the name .NET's `ArgumentNullException` reports.
    pub param: &'static str,
    /// `Some(member)` when this argument is the receiver of instance member `member`.
    pub receiver: Option<&'static str>,
}

impl ArgAt {
    /// Null where a value is required: a null receiver is a `NullReferenceException`
    /// (same text as a `VCall` on null), any other argument an `ArgumentNullException`.
    #[cold]
    pub fn null(&self) -> BuiltinError {
        match self.receiver {
            Some(member) => match crate::objops::OpError::null_call(member) {
                crate::objops::OpError::Throw(t) => BuiltinError::NullReference(t.msg),
                _ => BuiltinError::NullReference(format!("cannot call method `{member}` on a null reference")),
            },
            None => BuiltinError::ArgumentNull(format!("Value cannot be null. (Parameter '{}')", self.param)),
        }
    }

    /// The argument is of another kind than the parameter takes.
    #[cold]
    pub fn mismatch(&self, expected: &str, got: &Value) -> BuiltinError {
        BuiltinError::InvalidCast(format!(
            "{}: argument `{}` expects {expected}, got {}",
            self.builtin, self.param, crate::semantics::value_kind_name(got)))
    }
}

/// A Rust parameter type a builtin can take.
pub trait FromArg<'a>: Sized {
    const KIND: ParamKind;
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError>;
}

impl<'a> FromArg<'a> for &'a str {
    const KIND: ParamKind = ParamKind::Str;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::Str(s) => Ok(s.as_str()),
            Value::Null => Err(at.null()),
            other => Err(at.mismatch("a string", other)),
        }
    }
}

/// The string handle itself — for builtins that use its cached metadata (`str_meta`).
impl<'a> FromArg<'a> for &'a crate::metadata::vstr::Str {
    const KIND: ParamKind = ParamKind::Str;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::Str(s) => Ok(s),
            Value::Null => Err(at.null()),
            other => Err(at.mismatch("a string", other)),
        }
    }
}

impl<'a> FromArg<'a> for String {
    const KIND: ParamKind = ParamKind::Str;
    #[inline]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        <&str>::from_arg(v, at).map(str::to_owned)
    }
}

impl<'a> FromArg<'a> for Option<&'a str> {
    const KIND: ParamKind = ParamKind::OptStr;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::Null => Ok(None),
            other => <&str>::from_arg(other, at).map(Some),
        }
    }
}

impl<'a> FromArg<'a> for Option<String> {
    const KIND: ParamKind = ParamKind::OptStr;
    #[inline]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        <Option<&str>>::from_arg(v, at).map(|o| o.map(str::to_owned))
    }
}

impl<'a> FromArg<'a> for i64 {
    const KIND: ParamKind = ParamKind::I64;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::I64(n) => Ok(*n),
            // A boxed integer (`object`-typed call sites box their int arguments).
            Value::BoxedStruct(gc) => gc.borrow().boxed_prim_i64().ok_or_else(|| at.mismatch("an integer", v)),
            Value::Null => Err(at.null()),
            other => Err(at.mismatch("an integer", other)),
        }
    }
}

impl<'a> FromArg<'a> for f64 {
    const KIND: ParamKind = ParamKind::F64;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::F64(x) => Ok(*x),
            Value::I64(n) => Ok(*n as f64),
            Value::Null => Err(at.null()),
            other => Err(at.mismatch("a number", other)),
        }
    }
}

impl<'a> FromArg<'a> for bool {
    const KIND: ParamKind = ParamKind::Bool;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::Bool(b) => Ok(*b),
            Value::Null => Err(at.null()),
            other => Err(at.mismatch("a bool", other)),
        }
    }
}

impl<'a> FromArg<'a> for char {
    const KIND: ParamKind = ParamKind::Char;
    #[inline(always)]
    fn from_arg(v: &'a Value, at: &ArgAt) -> Result<Self, BuiltinError> {
        match v {
            Value::Char(c) => Ok(*c),
            Value::Null => Err(at.null()),
            other => Err(at.mismatch("a char", other)),
        }
    }
}

impl<'a> FromArg<'a> for &'a Value {
    const KIND: ParamKind = ParamKind::Any;
    #[inline(always)]
    fn from_arg(v: &'a Value, _at: &ArgAt) -> Result<Self, BuiltinError> { Ok(v) }
}

impl<'a> FromArg<'a> for Value {
    const KIND: ParamKind = ParamKind::Any;
    #[inline(always)]
    fn from_arg(v: &'a Value, _at: &ArgAt) -> Result<Self, BuiltinError> { Ok(*v) }
}

/// A `ref` / `out` argument (the caller passed the address of a local, an array element
/// or a field). [`RefArg::store`] writes through it, as an assignment to the parameter
/// inside a z42 method would.
///
/// The builtin sees the caller's reference only when its `[Native]` wrapper is folded
/// (`interp::forward` — every direct and virtual call is). A wrapper that does run (a
/// reflective `Invoke`, an embedder calling it) receives plain values — reflection has
/// no `ref` locations — and a store then goes nowhere, exactly as an unfolded wrapper
/// could not have written it back either.
#[derive(Clone, Copy)]
pub struct RefArg<'a> {
    handle: Option<&'a Value>,
}

impl RefArg<'_> {
    /// Store `val` into the location the caller passed (nothing, when it passed none).
    pub fn store(&self, ctx: &VmContext, val: Value) -> Result<(), BuiltinError> {
        let Some(&Value::Ref { idx, frame_id }) = self.handle else { return Ok(()) };
        let kind = ctx.transient_arena.lock().ref_kind(idx, frame_id)
            .map_err(|e| BuiltinError::Internal(e.to_string()))?;
        crate::interp::store_thru_ref(&kind, val, ctx).map_err(|e| BuiltinError::Internal(e.to_string()))
    }
}

impl<'a> FromArg<'a> for RefArg<'a> {
    const KIND: ParamKind = ParamKind::Ref;
    #[inline]
    fn from_arg(v: &'a Value, _at: &ArgAt) -> Result<Self, BuiltinError> {
        Ok(RefArg { handle: matches!(v, Value::Ref { .. }).then_some(v) })
    }
}

/// Parameter types a `blocking` builtin may take: owned plain Rust data, nothing that
/// points into the GC heap (its body runs parked, while a collection may run).
pub trait PlainArg {}
impl PlainArg for i64 {}
impl PlainArg for f64 {}
impl PlainArg for bool {}
impl PlainArg for char {}
impl PlainArg for String {}
impl PlainArg for Option<String> {}
