pub fn register_fatal_handlers(_: extern "C" fn(i32)) -> bool {
    false
}
pub fn leak_fd(file: std::fs::File) -> Option<i32> {
    drop(file);
    None
}
pub fn reset_default_and_reraise(_: i32) {}
pub fn signal_name(_: i32) -> &'static [u8] {
    b"UNKNOWN"
}
pub fn write_str(_: i32, _: &[u8]) {}
