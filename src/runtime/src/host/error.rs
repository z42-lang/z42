//! Host ABI status codes and thread-local last-error storage.
//!
//! Mirrors `Z42HostStatus` and `z42_host_last_error` from
//! `src/runtime/include/z42_host.h`. Spec: docs/internals/src/runtime/embedding.md §10.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

use z42_abi::Z42Error;

/// Status code returned from every `z42_host_*` entry point.
///
/// Layout: `i32` to match a plain C enum.
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Z42HostStatus {
    Ok = 0,

    AlreadyInit = 1,
    NotInit = 2,
    BadConfig = 3,
    FeatureOff = 4,

    BadZbc = 10,
    Verification = 11,

    EntryNotFound = 20,
    ArgMismatch = 21,

    VmException = 30,

    Internal = 99,
}

impl Z42HostStatus {
    /// Numeric code surfaced through `Z42Error.code`.
    pub fn as_code(self) -> u32 {
        self as i32 as u32
    }
}

thread_local! {
    /// Holds the last error set by any host API call on this thread.
    /// The CString backs the C-side `Z42Error.message` pointer; its
    /// lifetime extends until the next `set_error` / `clear_error` call.
    static LAST_ERROR: RefCell<LastError> = RefCell::new(LastError::default());
}

#[derive(Default)]
struct LastError {
    code: u32,
    message: Option<CString>,
}

/// Empty C string used when no error is currently recorded.
/// Static so the returned pointer is stable across calls.
static EMPTY_MESSAGE: &[u8] = b"\0";

/// Record an error on the current thread and return the status code so
/// callers can `return set_error(...)` in one line.
pub(crate) fn set_error(status: Z42HostStatus, message: impl Into<String>) -> Z42HostStatus {
    let msg = message.into();
    let cstring = CString::new(msg).unwrap_or_else(|_| {
        // Should be unreachable: messages are constructed from static strings
        // or `format!` output (no interior NULs). Fall back to a generic
        // marker so we never panic in this path.
        CString::new("z42_host: error message contained NUL").unwrap_or_default()
    });
    LAST_ERROR.with(|cell| {
        let mut cell = cell.borrow_mut();
        cell.code = status.as_code();
        cell.message = Some(cstring);
    });
    status
}

/// Clear any pending thread-local error. Called on the success path of
/// every host API entry point.
pub(crate) fn clear_error() {
    LAST_ERROR.with(|cell| {
        let mut cell = cell.borrow_mut();
        cell.code = 0;
        cell.message = None;
    });
}

/// Snapshot of the thread-local last error in C ABI form.
///
/// The returned `Z42Error.message` pointer is valid until the next
/// `set_error` / `clear_error` call on the same thread.
pub(crate) fn snapshot_last_error() -> Z42Error {
    LAST_ERROR.with(|cell| {
        let cell = cell.borrow();
        let message: *const c_char = match cell.message.as_ref() {
            Some(cs) => cs.as_ptr(),
            None => EMPTY_MESSAGE.as_ptr() as *const c_char,
        };
        Z42Error {
            code: cell.code,
            message,
        }
    })
}

