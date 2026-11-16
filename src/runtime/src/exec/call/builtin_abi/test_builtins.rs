//! Test-only typed builtins (`cfg(test)`), registered after the real ones: they let the
//! ABI tests drive every argument / result / error shape through both engines.

#[z42_macros::builtins]
pub mod typed {
    use crate::exec::call::builtin_abi::{Blocking, BuiltinError, RefArg};
    use crate::object::Value;
    use crate::vm::VmContext;

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
        Err(super::kind(k))
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
        let depth = ctx.exec().frame_stack_depth();
        Blocking::new(move || {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            Ok(format!("depth {depth}"))
        })
    }
}

pub(crate) use typed::DESCS;

use super::BuiltinError;

/// Error kind `k` for `__test_abi_fail`.
pub(crate) fn kind(k: i64) -> BuiltinError {
    let m = || format!("kind {k}");
    match k {
        0 => BuiltinError::Format(m()),
        1 => BuiltinError::Argument(m()),
        2 => BuiltinError::ArgumentNull(m()),
        3 => BuiltinError::ArgumentOutOfRange(m()),
        4 => BuiltinError::IndexOutOfRange(m()),
        5 => BuiltinError::Overflow(m()),
        6 => BuiltinError::InvalidOperation(m()),
        7 => BuiltinError::NotSupported(m()),
        8 => BuiltinError::InvalidCast(m()),
        9 => BuiltinError::NullReference(m()),
        10 => BuiltinError::KeyNotFound(m()),
        11 => BuiltinError::Io(m()),
        12 => BuiltinError::FileNotFound(m()),
        13 => BuiltinError::DirectoryNotFound(m()),
        14 => BuiltinError::PathTooLong(m()),
        15 => BuiltinError::EndOfStream(m()),
        16 => BuiltinError::UnauthorizedAccess(m()),
        17 => BuiltinError::Timeout(m()),
        _ => BuiltinError::Exception { class: "Std.Threading.ThreadException", message: m() },
    }
}
