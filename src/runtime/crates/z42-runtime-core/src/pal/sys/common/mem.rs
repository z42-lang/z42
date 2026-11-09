pub const CAN_DECOMMIT: bool = false;
pub fn page_size() -> usize {
    4096
}
pub unsafe fn decommit(_: usize, _: usize) {}
pub unsafe fn recommit(_: usize, _: usize) {}
