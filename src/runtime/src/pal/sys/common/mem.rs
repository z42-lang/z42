pub(crate) const CAN_DECOMMIT: bool = false;
pub(crate) fn page_size() -> usize {
    4096
}
pub(crate) unsafe fn decommit(_: usize, _: usize) {}
pub(crate) unsafe fn recommit(_: usize, _: usize) {}
