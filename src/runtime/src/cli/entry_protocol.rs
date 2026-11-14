//! C-only process entry contract, also compiled by the standalone launcher.
use std::ffi::c_char;

pub const ENTRY_VERSION: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EntryHeader {
    pub version: u32,
    pub size: u32,
}
#[repr(C)]
pub struct RuntimeEntry {
    pub header: EntryHeader,
    pub build_id: [u8; 32],
    pub run: Option<unsafe extern "C" fn(i32, *const *const c_char) -> i32>,
}
pub type GetRuntimeEntry = unsafe extern "C" fn() -> *const RuntimeEntry;
