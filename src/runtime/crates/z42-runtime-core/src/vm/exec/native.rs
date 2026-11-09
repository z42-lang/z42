//! VM-owned native call contract. The facade registers its provider before execution.
use crate::object::Value;
use crate::vm::context::VmContext;
use anyhow::Result;

/// Marshal failure surfaced to the caller (`ffi_call`).
///
/// `InvalidMarshal` is **user-catchable** — the caller constructs a
/// `Std.InvalidMarshalException` instance carrying the embedded message and
/// throws it through the interp's value-based propagation channel.
/// `Internal` covers VM-side programmer mismatches (IR / signature drift)
/// that cannot happen after a successful type check.
#[derive(Debug)]
pub enum NativeCallError {
    InvalidMarshal(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for NativeCallError {
    fn from(e: anyhow::Error) -> Self { Self::Internal(e) }
}

impl std::fmt::Display for NativeCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NativeCallError::InvalidMarshal(m) => write!(f, "{m}"),
            NativeCallError::Internal(e)       => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for NativeCallError {}

/// Immutable entry points; none retains a VM or managed argument.
pub struct NativeServices {
    pub call: fn(&VmContext, &str, &str, &str, &[Value], &[u32])
        -> std::result::Result<Value, NativeCallError>,
    pub pin_owned_buffer: fn(&VmContext, Box<[u8]>) -> Result<u64>,
    pub release_owned_buffer: fn(&VmContext, u64) -> Result<()>,
}

pub fn services(ctx: &VmContext) -> Result<&NativeServices> {
    ctx.ext::<NativeServices>()
        .ok_or_else(|| anyhow::anyhow!("native services were not registered before execution"))
}

#[cfg(all(test, z42_test_facade))]
#[path = "native_tests.rs"]
mod native_tests;
