pub(crate) fn fill(_buf: &mut [u8]) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no OS CSPRNG on wasm32",
    ))
}
