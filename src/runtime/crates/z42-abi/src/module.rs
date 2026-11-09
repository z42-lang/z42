//! Module registration and script-call ABI shared by statically linked and loaded providers.
//! All spans are borrowed. Module descriptors remain immutable while their code is retained;
//! call values are valid only for the synchronous, same-thread call that supplied them.
use core::ffi::c_void;

pub const MODULE_ABI_VERSION: u32 = 1;
pub const OK: u32 = 0;
pub const ERROR: u32 = 1;
pub const THROWN: u32 = 2;

pub const NULL: u32 = 0;
pub const I64: u32 = 1;
pub const F64: u32 = 2;
pub const BOOL: u32 = 3;
pub const STR: u32 = 4;
pub const OPT_STR: u32 = 5;
pub const BYTES: u32 = 6;
pub const STR_ARRAY: u32 = 7;
pub const VOID: u32 = 8;

pub const PURE: u32 = 1;
pub const BLOCKING: u32 = 2;
pub const REENTERS: u32 = 4;
pub const CAP_NATIVE_LOADING: u64 = 1;
pub const CAP_TERMINAL: u64 = 2;
pub const CAP_FILESYSTEM: u64 = 4;
pub const CAP_CONSOLE_INPUT: u64 = 8;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Text {
    pub data: *const u8,
    pub len: usize,
}
impl Text {
    pub const fn new(text: &str) -> Self {
        Self {
            data: text.as_ptr(),
            len: text.len(),
        }
    }
}

/// STR/BYTES use data/len; STR_ARRAY uses a data pointer to len Values.
/// Scalars use bits. NULL and VOID are distinct and carry no payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Value {
    pub kind: u32,
    pub reserved: u32,
    pub bits: u64,
    pub data: *const u8,
    pub len: usize,
}
impl Value {
    pub const fn scalar(kind: u32, bits: u64) -> Self {
        Self {
            kind,
            reserved: 0,
            bits,
            data: core::ptr::null(),
            len: 0,
        }
    }
    pub const fn null() -> Self {
        Self::scalar(NULL, 0)
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Parameters {
    pub data: *const u32,
    pub len: usize,
}
impl Parameters {
    pub const fn new(kinds: &[u32]) -> Self {
        Self {
            data: kinds.as_ptr(),
            len: kinds.len(),
        }
    }
}

/// The host owns context; the module owns state. Neither pointer may escape this call.
#[repr(C)]
pub struct Call {
    pub host: *const HostApi,
    pub context: *mut c_void,
    pub state: *mut c_void,
}

pub type Invoke = unsafe extern "C" fn(*mut Call) -> u32;

#[repr(C)]
pub struct Function {
    pub name: Text,
    pub parameters: Parameters,
    pub result: u32,
    pub flags: u32,
    pub invoke: Option<Invoke>,
}

/// Both the native image entry and static linkage return this same descriptor.
#[repr(C)]
pub struct Module {
    pub version: u32,
    pub size: u32,
    pub name: Text,
    pub functions: *const Function,
    pub function_count: usize,
    pub create_state: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy_state: Option<unsafe extern "C" fn(*mut c_void)>,
}
pub type ModuleEntry = unsafe extern "C" fn() -> *const Module;

/// Host services never retain borrowed provider buffers. return_value copies data before
/// returning; managed allocation happens after the native blocking span ends. invoke_script
/// temporarily resumes the mutator and returns copied C-layout values in a call-owned arena.
#[repr(C)]
pub struct HostApi {
    pub version: u32,
    pub size: u32,
    pub capabilities: u64,
    pub argument: unsafe extern "C" fn(*mut Call, usize, *mut Value) -> u32,
    pub return_value: unsafe extern "C" fn(*mut Call, *const Value) -> u32,
    pub raise: unsafe extern "C" fn(*mut Call, Text, Text) -> u32,
    pub invoke_script:
        unsafe extern "C" fn(*mut Call, Text, *const Value, usize, *mut Value) -> u32,
    pub resolve_symbol: unsafe extern "C" fn(*mut Call, Text, Text, *mut *mut c_void) -> u32,
}

// Descriptor spans point to immutable data retained with their module image.
unsafe impl Sync for Text {}
unsafe impl Sync for Parameters {}
unsafe impl Sync for Function {}
unsafe impl Sync for Module {}
unsafe impl Sync for HostApi {}

#[cfg(test)]
#[path = "module_tests.rs"]
mod tests;
