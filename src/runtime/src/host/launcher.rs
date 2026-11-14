//! Dynamic process entry delegates to the same host CLI used by static embeddings.
use super::entry_api::*;
use std::ffi::{c_char, CStr, OsString};

unsafe extern "C" fn run(argc: i32, argv: *const *const c_char) -> i32 {
    let result = std::panic::catch_unwind(|| -> anyhow::Result<()> {
        anyhow::ensure!(argc > 0 && !argv.is_null(), "invalid launcher arguments");
        let mut args = Vec::with_capacity(argc as usize);
        for i in 0..argc as usize {
            let arg = unsafe { *argv.add(i) };
            anyhow::ensure!(!arg.is_null(), "null launcher argument");
            args.push(OsString::from(unsafe { CStr::from_ptr(arg) }.to_str()?));
        }
        super::cli::run_from(args)
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => { eprintln!("Error: {error:?}"); 1 }
        Err(_) => { eprintln!("z42vm: runtime entry panicked"); 1 }
    }
}
static ENTRY: RuntimeEntry = RuntimeEntry {
    header: EntryHeader { version: ENTRY_VERSION, size: std::mem::size_of::<RuntimeEntry>() as u32 },
    build_id: crate::component_abi::COMPONENT_BUILD_ID,
    run: Some(run),
};
#[no_mangle]
pub extern "C" fn z42_runtime_entry_v1() -> *const RuntimeEntry { &ENTRY }
