//! `__crypto_random_bytes` — OS-CSPRNG via `getrandom` crate.
//!
//! add-csprng-to-crypto . Backs `Std.Crypto.Random.GetBytes(n)`.
//! Underlying syscall by platform:
//!   - Linux / Android:  getrandom(2)
//!   - macOS / iOS:      getentropy(2) / SecRandomCopyBytes
//!   - Windows:          BCryptGenRandom (CNG)
//!   - wasm32:           NotSupportedException (`pal::random` has no source there;
//!     browser bridge / WASI integration is add-csprng-wasm-bridge follow-up).

use crate::corelib::abi::BuiltinError;
use super::convert::arg_i64;
use crate::object::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

const NAME: &str = "__crypto_random_bytes";

pub fn crypto_random_bytes(ctx: &VmContext, n: i64) -> Result<crate::object::Value, BuiltinError> {
    if n < 0 {
        return Err(BuiltinError::Argument(format!("{}: n must be non-negative, got {}", NAME, n)));
    }
    if n > i32::MAX as i64 {
        // Defensive guard: typical CSPRNG usage is < 1 KB per call (one
        // nonce / key). Larger requests are almost certainly misuse and
        // would balloon the i64-per-byte Value::Array representation
        // (8x memory inflation).
        return Err(BuiltinError::Argument(format!("{}: n exceeds i32::MAX ({}), got {}", NAME, i32::MAX, n)));
    }
    let mut buf = vec![0u8; n as usize];
    crate::pal::random::fill(&mut buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::Unsupported => BuiltinError::NotSupported(format!("{}: CSPRNG not available on this platform", NAME)),
        _ => BuiltinError::Io(format!("{}: OS CSPRNG failed: {}", NAME, e)),
    })?;
    // packed-primitive-arrays Step 3: pack CSPRNG output straight into `Bytes`.
    Ok(ctx.heap().alloc_bytes(buf))
}

/// CSPRNG 迁 builtin ABI v2(P1-8 批次 12)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__crypto_random_bytes")]
    pub fn random_bytes(ctx: &VmContext, n: i64) -> Result<crate::object::Value, crate::corelib::abi::BuiltinError> {
        crypto_random_bytes(ctx, n)
    }
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod crypto_tests;
