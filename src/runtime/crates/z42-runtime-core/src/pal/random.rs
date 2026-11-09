//! OS entropy.

/// Fill `buf` from the OS CSPRNG. `Unsupported` where the target has no entropy source
/// wired up (wasm32-unknown-unknown).
pub fn fill(buf: &mut [u8]) -> std::io::Result<()> {
    super::sys::random::fill(buf)
}

#[cfg(test)]
#[path = "random_tests.rs"]
mod tests;
