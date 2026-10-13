//! Test-only typed builtins (`cfg(test)`), registered after the real ones: they let the
//! ABI tests drive every argument / result / error shape through both engines.

#[z42_macros::builtins]
pub mod typed {
    use crate::corelib::abi::{Blocking, BuiltinError, RefArg};
    use crate::metadata::Value;
    use crate::vm_context::VmContext;

    /// Echo every scalar kind back as text.
    #[builtin(name = "__test_abi_args")]
    pub fn args(s: &str, n: i64, x: f64, b: bool, c: char, o: Option<&str>) -> String {
        format!("{s}|{n}|{x}|{b}|{c}|{}", o.unwrap_or("<null>"))
    }

    #[builtin(name = "__test_abi_add", pure)]
    pub fn add(a: i64, b: i64) -> i64 { a + b }

    /// A receiver argument: null → `NullReferenceException`.
    #[builtin(name = "__test_abi_recv", this = "Recv")]
    pub fn recv(this: &str) -> i64 { this.len() as i64 }

    /// Fail with error kind `k` (see `abi_tests::KINDS`).
    #[builtin(name = "__test_abi_fail")]
    pub fn fail(k: i64) -> Result<(), BuiltinError> {
        Err(super::super::abi_tests::kind(k))
    }

    /// Always an internal (fatal) error.
    #[builtin(name = "__test_abi_internal")]
    pub fn internal() -> Result<i64, BuiltinError> {
        Err(BuiltinError::Internal("broken invariant (test)".into()))
    }

    /// Store `n` through a `ref` argument.
    #[builtin(name = "__test_abi_store")]
    pub fn store(ctx: &VmContext, v: RefArg, n: i64) -> Result<(), BuiltinError> {
        v.store(ctx, Value::I64(n))
    }

    /// Parked for `ms` milliseconds, then returns a fresh string (allocated after unparking).
    #[builtin(name = "__test_abi_sleep", blocking)]
    pub fn sleep(ms: i64) -> String {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        format!("slept {ms}")
    }

    /// Prepare unparked, block parked, convert after.
    #[builtin(name = "__test_abi_prepared")]
    pub fn prepared(ctx: &VmContext, ms: i64) -> Blocking<String> {
        let depth = ctx.frame_stack_depth();
        Blocking::new(move || {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            Ok(format!("depth {depth}"))
        })
    }
}

pub(crate) use typed::DESCS;
