pub(crate) fn register_fatal_handlers(_: extern "C" fn(i32)) -> bool {
    false
}
pub(crate) fn leak_fd(file: std::fs::File) -> Option<i32> {
    drop(file);
    None
}
pub(crate) fn reset_default_and_reraise(_: i32) {}
pub(crate) fn signal_name(_: i32) -> &'static [u8] {
    b"UNKNOWN"
}
pub(crate) fn write_str(_: i32, _: &[u8]) {}
