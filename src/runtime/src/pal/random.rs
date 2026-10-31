//! OS entropy.

/// Fill `buf` from the OS CSPRNG. `Unsupported` where the target has no entropy source
/// wired up (wasm32-unknown-unknown).
pub fn fill(buf: &mut [u8]) -> std::io::Result<()> {
    imp(buf)
}

#[cfg(not(target_arch = "wasm32"))]
fn imp(buf: &mut [u8]) -> std::io::Result<()> {
    getrandom::fill(buf).map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(target_arch = "wasm32")]
fn imp(_buf: &mut [u8]) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no OS CSPRNG on wasm32"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn fill_produces_bytes() {
        let mut a = [0u8; 32];
        super::fill(&mut a).unwrap();
        let mut b = [0u8; 32];
        super::fill(&mut b).unwrap();
        assert_ne!(a, b);
    }
}
