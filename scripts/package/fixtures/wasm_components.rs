//! C-ABI consumer linked from the published .a files, without Cargo dependencies.
#![no_std]
use core::{ffi::c_void, ptr};
#[repr(C)]
struct ComponentId([u8; 16]);
#[repr(C)]
struct Config {
    version: u32, size: u32, mode: i32, initial: usize, maximum: usize,
    stdout: usize, stderr: usize, sink: usize, paths: usize,
    resolver: unsafe extern "C" fn(*const u8, *mut *const u8, *mut usize, *mut c_void) -> i32,
    resolver_data: usize,
}
#[repr(C)]
struct Value { tag: u32, reserved: u32, payload: u64 }
#[repr(C)]
struct Error { code: u32, message: *const u8 }
extern "C" {
    fn resolve(name: *const u8, bytes: *mut *const u8, length: *mut usize, data: *mut c_void) -> i32;
    fn report(message: *const u8);
    fn namespace(name: *const u8, length: usize, data: *mut c_void);
    fn z42_interp_init() -> *const c_void;
    fn z42_host_register_static_component(id: ComponentId, init: unsafe extern "C" fn() -> *const c_void) -> i32;
    fn z42_host_initialize(config: *const Config, host: *mut *mut c_void) -> i32;
    fn z42_host_load_zbc(host: *mut c_void, bytes: *const u8, length: usize, module: *mut *mut c_void) -> i32;
    fn z42_host_resolve_entry(host: *mut c_void, module: *mut c_void, name: *const u8, entry: *mut *mut c_void) -> i32;
    fn z42_host_invoke(entry: *mut c_void, args: *const Value, length: usize, result: *mut Value) -> i32;
    fn z42_host_last_error(host: *mut c_void) -> Error;
    fn z42_host_shutdown(host: *mut c_void) -> i32;
    fn z42_zpkg_read_namespaces(bytes: *const u8, length: usize,
        visitor: unsafe extern "C" fn(*const u8, usize, *mut c_void), data: *mut c_void) -> i32;
    #[cfg(compression)]
    fn z42_compression_get_symbol(name: *const u8) -> *const c_void;
    #[cfg(compression)]
    fn z42_host_register_static_native_library(name: *const u8, resolver: unsafe extern "C" fn(*const u8) -> *const c_void) -> i32;
}
// JS stores immutable zpkg/zbc data here; runtime allocations use a separate heap.
static mut INPUT: [u8; 64 * 1024 * 1024] = [0; 64 * 1024 * 1024];
#[no_mangle]
pub extern "C" fn input() -> *mut u8 { ptr::addr_of_mut!(INPUT).cast() }
#[no_mangle]
pub unsafe extern "C" fn index_package(bytes: *const u8, length: usize) -> i32 {
    unsafe { z42_zpkg_read_namespaces(bytes, length, namespace, ptr::null_mut()) }
}
unsafe fn failed(host: *mut c_void, code: i32) -> i32 {
    unsafe { report(z42_host_last_error(host).message); }
    code
}
#[no_mangle]
pub unsafe extern "C" fn run(bytes: *const u8, length: usize) -> i32 {
    let id = ComponentId(*b"z42.interp.v1\0\0\0");
    if unsafe { z42_host_register_static_component(id, z42_interp_init) } != 0 { return 1; }
    #[cfg(compression)]
    if unsafe { z42_host_register_static_native_library(b"z42_compression\0".as_ptr(), z42_compression_get_symbol) } != 0 { return 2; }
    let config = Config { version: 2, size: core::mem::size_of::<Config>() as u32, mode: 1,
        initial: 0, maximum: 0, stdout: 0, stderr: 0, sink: 0, paths: 0, resolver: resolve, resolver_data: 0 };
    let mut host = ptr::null_mut();
    if unsafe { z42_host_initialize(&config, &mut host) } != 0 { return unsafe { failed(host, 3) }; }
    let mut module = ptr::null_mut();
    if unsafe { z42_host_load_zbc(host, bytes, length, &mut module) } != 0 { return unsafe { failed(host, 4) }; }
    let mut entry = ptr::null_mut();
    if unsafe { z42_host_resolve_entry(host, module, b"EngineComponentProbe.Echo\0".as_ptr(), &mut entry) } != 0 { return unsafe { failed(host, 5) }; }
    let arg = Value { tag: 1, reserved: 0, payload: 40 };
    let mut result = Value { tag: 0, reserved: 0, payload: 0 };
    if unsafe { z42_host_invoke(entry, &arg, 1, &mut result) } != 0 { return unsafe { failed(host, 6) }; }
    if result.tag != 1 || result.payload != 41 { return 7; }
    if unsafe { z42_host_resolve_entry(host, module, b"EngineComponentProbe.CompressionRoundtrip\0".as_ptr(), &mut entry) } != 0 { return unsafe { failed(host, 8) }; }
    let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
    #[cfg(compression)]
    if status != 0 || result.tag != 1 || result.payload != 36 { return unsafe { failed(host, 9) }; }
    #[cfg(not(compression))]
    if status == 0 { return 10; } else { unsafe { report(z42_host_last_error(host).message); } }
    unsafe { z42_host_shutdown(host) }
}
