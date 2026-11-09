use super::*;
unsafe extern "C" fn run(_: i32, _: *const *const std::ffi::c_char) -> i32 { 42 }
fn entry() -> RuntimeEntry {
    RuntimeEntry { header: EntryHeader { version: ENTRY_VERSION, size: std::mem::size_of::<RuntimeEntry>() as u32 },
        build_id: identity::COMPONENT_BUILD_ID, run: Some(run) }
}
#[test]
fn same_pack_entry_is_accepted() {
    let callback = unsafe { callback(&entry()) }.unwrap();
    assert_eq!(unsafe { callback(0, std::ptr::null()) }, 42);
}
#[test]
fn null_or_misaligned_entry_is_rejected() {
    for ptr in [std::ptr::null(), 1usize as *const RuntimeEntry] {
        assert!(unsafe { callback(ptr) }.unwrap_err().to_string().contains("null or misaligned"));
    }
}
#[test]
fn version_and_size_are_checked_before_reading_the_table() {
    #[repr(C, align(8))]
    struct Aligned(EntryHeader);
    for header in [EntryHeader { version: 2, size: 0 }, EntryHeader { version: ENTRY_VERSION, size: 8 }] {
        let header = Aligned(header);
        assert!(unsafe { callback((&header as *const Aligned).cast()) }.unwrap_err().to_string().contains("ABI"));
    }
}
#[test]
fn different_build_and_absent_callback_are_rejected() {
    let mut table = entry(); table.build_id[0] ^= 1;
    assert!(unsafe { callback(&table) }.unwrap_err().to_string().contains("build identity"));
    table = entry(); table.run = None;
    assert!(unsafe { callback(&table) }.unwrap_err().to_string().contains("no run callback"));
}
