pub(crate) fn fill(buf: &mut [u8]) -> std::io::Result<()> {
    getrandom::fill(buf).map_err(|e| std::io::Error::other(e.to_string()))
}
