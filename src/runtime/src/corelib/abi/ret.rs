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

/// A fresh `byte[]` (packed).
impl IntoRet for Vec<u8> {
    const KIND: RetKind = RetKind::Bytes;
    #[inline]
    fn into_ret(self, ctx: &VmContext) -> CallResult { Ok(Some(ctx.heap().alloc_bytes(self))) }
}

/// A fresh `string[]`.
impl IntoRet for Vec<String> {
    const KIND: RetKind = RetKind::StrArray;
    fn into_ret(self, ctx: &VmContext) -> CallResult {
        let elems: Vec<Value> = self.into_iter().map(|s| Value::Str(s.into())).collect();
        Ok(Some(ctx.heap().alloc_array(elems)))
    }
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
impl PlainRet for Vec<u8> {}
impl PlainRet for Vec<String> {}

type ParkedBody<R> = Box<dyn FnOnce() -> Result<R, BuiltinError> + Send + 'static>;

/// The blocking part of a builtin that needs the context first (to take a handle out of
/// a registry, say): the builtin does that unparked and returns `Blocking::new(move ||
/// …)`. The closure then runs with the context **parked** — a collection may run
/// meanwhile — and its plain result is converted to a `Value` after unparking.
///
/// The closure is `Send + 'static`: it cannot borrow the context (nor anything else on
/// the caller's stack), so it has no way to reach the GC heap, and the result type is
/// [`PlainRet`]. A builtin whose whole body may run parked uses `#[builtin(blocking)]`
/// instead (no closure, no allocation).
///
/// - [`Blocking::ready`] — the answer is known without blocking (a zero-length read):
///   no park at all;
/// - [`Blocking::then`] — the parked part produces Rust data that is not a `PlainRet`
///   (an opened `File`, a spawned `Child`, bytes to copy into the caller's buffer); the
///   `then` step runs **unparked**, with the context, and builds the result (inserts the
///   handle into its registry, writes the GC array, allocates).
pub struct Blocking<R> {
    body: BlockingBody<R>,
}

enum BlockingBody<R> {
    Run(ParkedBody<R>),
    Ready(R),
}

impl<R: Send + 'static> Blocking<R> {
    pub fn new(body: impl FnOnce() -> Result<R, BuiltinError> + Send + 'static) -> Self {
        Blocking { body: BlockingBody::Run(Box::new(body)) }
    }

    /// Already done: the call does not park.
    pub fn ready(r: R) -> Self {
        Blocking { body: BlockingBody::Ready(r) }
    }

    /// Run the parked part, then `then` unparked with the context (see the type docs).
    pub fn then<V: IntoRet>(
        self, then: impl FnOnce(&VmContext, R) -> Result<V, BuiltinError> + 'static,
    ) -> Then<V> {
        Then { run: Box::new(move |ctx| then(ctx, self.run(ctx)?)) }
    }

    /// The parked part (or the ready value).
    fn run(self, ctx: &VmContext) -> Result<R, BuiltinError> {
        match self.body {
            BlockingBody::Ready(r) => Ok(r),
            BlockingBody::Run(body) => {
                let _park = crate::gc::NativeParkGuard::enter(ctx);
                body()
            }
        }
    }
}

impl<R: PlainRet + Send + 'static> IntoRet for Blocking<R> {
    const KIND: RetKind = R::KIND;
    fn into_ret(self, ctx: &VmContext) -> CallResult {
        self.run(ctx)?.into_ret(ctx)
    }
}

/// A [`Blocking`] part followed by an unparked step ([`Blocking::then`]).
pub struct Then<V> {
    run: Box<dyn FnOnce(&VmContext) -> Result<V, BuiltinError>>,
}

impl<V: IntoRet> IntoRet for Then<V> {
    const KIND: RetKind = V::KIND;
    fn into_ret(self, ctx: &VmContext) -> CallResult {
        (self.run)(ctx)?.into_ret(ctx)
    }
}
