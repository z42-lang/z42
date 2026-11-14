#[cfg_attr(not(missing_entry), no_mangle)]
#[cfg_attr(missing_entry, export_name = "other_entry")]
pub extern "C" fn z42_main_v1() -> i32 {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == ["space value", "中文", "--value=quoted\"value"].map(std::ffi::OsString::from) { 37 } else { 19 }
}
