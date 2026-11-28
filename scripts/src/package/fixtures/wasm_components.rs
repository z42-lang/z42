//! C-ABI consumer linked from the published .a files, without Cargo dependencies.
#![no_std]
use core::{ffi::c_void, ptr};
#[repr(C)]
struct ComponentId([u8; 16]);
#[repr(C)]
struct Config {
    version: u32, size: u32, mode: i32,
    stdout: usize, stderr: usize, sink: usize, paths: usize,
    resolver: unsafe extern "C" fn(*const FileRequest, *mut c_void, *mut c_void) -> i32,
    resolver_data: usize,
}
#[repr(C)]
struct FileRequest { size: u32, kind: u32, name: *const u8 }
#[repr(C)]
struct Value { tag: u32, reserved: u32, payload: u64 }
#[repr(C)]
struct Error { code: u32, message: *const u8 }
extern "C" {
    fn resolve(name: *const u8, kind: u32, bytes: *mut *const u8, length: *mut usize) -> i32;
    fn report(message: *const u8);
    #[cfg(profiler)]
    fn z42_profiler_init() -> *const c_void;
    fn z42_interp_init() -> *const c_void;
    fn z42_component_register_static(id: ComponentId, init: unsafe extern "C" fn() -> *const c_void) -> u32;
    fn z42_host_initialize(config: *const Config, host: *mut *mut c_void) -> i32;
    fn z42_file_write(writer: *mut c_void, bytes: *const u8, length: usize) -> i32;
    fn z42_host_load_bytes(host: *mut c_void, name: *const u8, bytes: *const u8, length: usize, module: *mut *mut c_void) -> i32;
    fn z42_host_resolve_entry(host: *mut c_void, module: *mut c_void, name: *const u8, entry: *mut *mut c_void) -> i32;
    fn z42_host_invoke(entry: *mut c_void, args: *const Value, length: usize, result: *mut Value) -> i32;
    fn z42_host_last_error(host: *mut c_void) -> Error;
    fn z42_host_shutdown(host: *mut c_void) -> i32;
    #[cfg(compression)]
    fn z42_compression_get_symbol(name: *const u8) -> *const c_void;
    #[cfg(compression)]
    fn z42_native_register_static(name: *const u8, resolver: unsafe extern "C" fn(*const u8) -> *const c_void) -> u32;
}
// JS stores immutable zpkg/zbc data here; runtime allocations use a separate heap.
static mut INPUT: [u8; 64 * 1024 * 1024] = [0; 64 * 1024 * 1024];
#[no_mangle]
pub extern "C" fn input() -> *mut u8 { ptr::addr_of_mut!(INPUT).cast() }
unsafe extern "C" fn resolve_file(request: *const FileRequest, writer: *mut c_void, _: *mut c_void) -> i32 {
    let request = unsafe { &*request };
    let mut bytes = ptr::null();
    let mut length = 0;
    let status = unsafe { resolve(request.name, request.kind, &mut bytes, &mut length) };
    if status == 1 && unsafe { z42_file_write(writer, bytes, length) } != 0 {
        return -1;
    }
    status
}
unsafe fn failed(host: *mut c_void, code: i32) -> i32 {
    unsafe { report(z42_host_last_error(host).message); }
    code
}
#[no_mangle]
pub unsafe extern "C" fn run(name: *const u8, bytes: *const u8, length: usize) -> i32 {
    let id = ComponentId(*b"z42.interp.v1\0\0\0");
    if unsafe { z42_component_register_static(id, z42_interp_init) } != 0 { return 1; }
    #[cfg(compression)]
    if unsafe { z42_native_register_static(b"z42_compression\0".as_ptr(), z42_compression_get_symbol) } != 0 { return 2; }
    #[cfg(profiler)]
    if unsafe { z42_component_register_static(ComponentId(*b"z42.profiler.v1\0"), z42_profiler_init) } != 0 { return 11; }
    let config = Config { version: 3, size: core::mem::size_of::<Config>() as u32, mode: 1,
        stdout: 0, stderr: 0, sink: 0, paths: 0, resolver: resolve_file, resolver_data: 0 };
    let mut host = ptr::null_mut();
    if unsafe { z42_host_initialize(&config, &mut host) } != 0 { return unsafe { failed(host, 3) }; }
    let mut module = ptr::null_mut();
    if unsafe { z42_host_load_bytes(host, name, bytes, length, &mut module) } != 0 { return unsafe { failed(host, 4) }; }
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
    if unsafe { z42_host_resolve_entry(host, module, b"EngineComponentProbe.HeapInspection\0".as_ptr(), &mut entry) } != 0 { return unsafe { failed(host, 12) }; }
    let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
    #[cfg(profiler)]
    if status != 0 || result.tag != 1 || result.payload != 42 { return unsafe { failed(host, 13) }; }
    #[cfg(not(profiler))]
    if status == 0 { return 14; } else { unsafe { report(z42_host_last_error(host).message); } }
    unsafe { z42_host_shutdown(host) }
}
