//! Thread-local last-error slot for `z42_last_error()`.
//!
//! Each `z42_*` ABI entry point clears the slot on entry, then either
//! succeeds (slot stays cleared) or stores a `Z42Error` describing the
//! failure. The slot is per-thread so concurrent z42 hosts don't clobber
//! each other's diagnostics.

use std::cell::Cell;
use std::ffi::CString;
use std::os::raw::c_char;

use z42_abi::Z42Error;

// retire-z-codes: the named `Z####` constants formerly declared
// here (Z0905 / Z0906 / Z0910) were retired alongside the cross-language
// catalog. Callers now inline the numeric codes locally (see
// `native::exports::TYPE_REGISTRATION_FAILURE` etc.). The `Z42Error.code`
// field stays in the ABI for embedder wire-compat; values are unchanged.

thread_local! {
    /// Last-error slot. Cleared by each `z42_*` entry on call; set on
    /// failure. `Z42Error.message` points into a leaked `CString` — short
    /// error count keeps the leak insignificant and avoids ABI concerns
    /// about who owns the buffer (caller is told "do not free").
    static LAST_ERROR: Cell<Z42Error> = const { Cell::new(NO_ERROR) };
}

pub const NO_ERROR: Z42Error = Z42Error {
    code: 0,
    message: std::ptr::null(),
};

/// Reset the slot. Called by every `z42_*` entry point.
pub(crate) fn clear() {
    LAST_ERROR.with(|c| c.set(NO_ERROR));
}

/// Record an error on the calling thread.
///
/// The message is converted to a NUL-terminated C string and intentionally
/// leaked so the returned pointer stays valid until the thread exits.
/// Diagnostic paths only — never call this in a tight loop.
pub(crate) fn set(code: u32, message: impl Into<String>) {
    let s = message.into();
    let cstring = CString::new(s)
        .unwrap_or_else(|_| CString::new("(error message contained interior NUL)").unwrap());
    // `into_raw()` transfers ownership to the leaked allocation; we never
    // reclaim it. ~50 bytes per failure × low call rate is acceptable.
    let raw: *const c_char = cstring.into_raw();
    LAST_ERROR.with(|c| c.set(Z42Error { code, message: raw }));
}

/// Read the current thread's last error. Multiple reads return the same
/// value until the next `z42_*` entry point resets the slot.
pub fn last() -> Z42Error {
    LAST_ERROR.with(|c| c.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_and_set_round_trip() {
        clear();
        assert_eq!(last().code, 0);
        set(906, "ABI version mismatch: expected 1, got 2");
        let e = last();
        assert_eq!(e.code, 906);
        let msg = unsafe { std::ffi::CStr::from_ptr(e.message) }.to_string_lossy();
        assert!(msg.contains("ABI version mismatch"));
    }

    #[test]
    fn clear_resets_to_no_error() {
        set(905, "stale");
        clear();
        assert_eq!(last().code, 0);
        assert!(last().message.is_null());
    }
}
