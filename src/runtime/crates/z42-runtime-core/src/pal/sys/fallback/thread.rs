pub const CAN_SPAWN: bool = true;
pub use super::super::common::thread::current_token;
pub(crate) fn current_thread_stack() -> Option<(usize, usize)> {
    None
}
