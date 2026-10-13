//! Result side of the builtin ABI: how a Rust result becomes the call's value
//! ([`IntoRet`]), and [`Blocking`] — the parked part of a builtin.

use super::{BuiltinError, CallResult, RetKind};
use crate::metadata::Value;
use crate::vm_context::VmContext;

/// A Rust result type a builtin can return. `None` = void (the destination register is
/// left alone).
pub trait IntoRet {
    const KIND: RetKind;
    fn into_ret(self, ctx: &VmContext) -> CallResult;
}

impl IntoRet for () {
    const KIND: RetKind = RetKind::Void;
    #[inline(always)]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(None) }
}

impl IntoRet for i64 {
    const KIND: RetKind = RetKind::I64;
    #[inline(always)]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(Value::I64(self))) }
}

impl IntoRet for f64 {
    const KIND: RetKind = RetKind::F64;
    #[inline(always)]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(Value::F64(self))) }
}

impl IntoRet for bool {
    const KIND: RetKind = RetKind::Bool;
    #[inline(always)]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(Value::Bool(self))) }
}

impl IntoRet for char {
    const KIND: RetKind = RetKind::Char;
    #[inline(always)]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(Value::Char(self))) }
}

impl IntoRet for String {
    const KIND: RetKind = RetKind::Str;
    #[inline]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(Value::Str(self.into()))) }
}

impl IntoRet for &str {
    const KIND: RetKind = RetKind::Str;
    #[inline]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(Value::Str(self.into()))) }
}

impl IntoRet for Option<String> {
    const KIND: RetKind = RetKind::OptStr;
    #[inline]
    fn into_ret(self, _: &VmContext) -> CallResult {
        Ok(Some(self.map_or(Value::Null, |s| Value::Str(s.into()))))
    }
}

impl IntoRet for Value {
    const KIND: RetKind = RetKind::Any;
    #[inline(always)]
    fn into_ret(self, _: &VmContext) -> CallResult { Ok(Some(self)) }
}

/// Result types a parked body may produce: plain Rust data, turned into a `Value` only
/// after the context is unparked (a `String` result is a GC allocation).
pub trait PlainRet: IntoRet {}
impl PlainRet for () {}
impl PlainRet for i64 {}
impl PlainRet for f64 {}
impl PlainRet for bool {}
impl PlainRet for char {}
impl PlainRet for String {}
impl PlainRet for Option<String> {}

/// The blocking part of a builtin that needs the context first (to take a handle out of
/// a registry, say): the builtin does that unparked and returns `Blocking::new(move ||
/// …)`. The closure then runs with the context **parked** — a collection may run
/// meanwhile — and its plain result is converted to a `Value` after unparking.
///
/// The closure is `Send + 'static`: it cannot borrow the context (nor anything else on
/// the caller's stack), so it has no way to reach the GC heap, and the result type is
/// [`PlainRet`]. A builtin whose whole body may run parked uses `#[builtin(blocking)]`
/// instead (no closure, no allocation).
pub struct Blocking<R> {
    body: Box<dyn FnOnce() -> Result<R, BuiltinError> + Send + 'static>,
}

impl<R: PlainRet> Blocking<R> {
    pub fn new(body: impl FnOnce() -> Result<R, BuiltinError> + Send + 'static) -> Self {
        Blocking { body: Box::new(body) }
    }
}

impl<R: PlainRet> IntoRet for Blocking<R> {
    const KIND: RetKind = R::KIND;
    fn into_ret(self, ctx: &VmContext) -> CallResult {
        let r = {
            let _park = crate::gc::NativeParkGuard::enter(ctx);
            (self.body)()
        };
        r?.into_ret(ctx)
    }
}
