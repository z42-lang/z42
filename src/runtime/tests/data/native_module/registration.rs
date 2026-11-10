#[path = "../../../crates/z42-abi/src/module.rs"]
mod abi;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

static DROPS: AtomicU64 = AtomicU64::new(0);
unsafe extern "C" fn create() -> *mut c_void { Box::into_raw(Box::new(AtomicI64::new(0))).cast() }
unsafe extern "C" fn destroy(state: *mut c_void) {
    drop(unsafe { Box::from_raw(state.cast::<AtomicI64>()) });
    DROPS.fetch_add(1, Ordering::SeqCst);
}
unsafe extern "C" fn count(call: *mut abi::Call) -> u32 {
    let call_ref = unsafe { &*call };
    let count = unsafe { &*call_ref.state.cast::<AtomicI64>() }.fetch_add(1, Ordering::SeqCst) + 1;
    let result = abi::Value::scalar(abi::I64, count as u64);
    unsafe { ((*call_ref.host).return_value)(call, &result) }
}
static FUNCTIONS: &[abi::Function] = &[abi::Function {
    name: abi::Text::new("__fixture_native_count"), parameters: abi::Parameters::new(&[]),
    result: abi::I64, flags: 0, invoke: Some(count),
}];
static MODULE: abi::Module = abi::Module {
    version: if cfg!(broken) { 999 } else { abi::MODULE_ABI_VERSION },
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("z42_fixture"), functions: FUNCTIONS.as_ptr(), function_count: FUNCTIONS.len(),
    create_state: Some(create), destroy_state: Some(destroy),
};
#[no_mangle]
pub extern "C" fn z42_fixture_module_v1() -> *const abi::Module { &MODULE }
#[no_mangle]
pub extern "C" fn z42_fixture_drop_count() -> u64 { DROPS.load(Ordering::SeqCst) }
