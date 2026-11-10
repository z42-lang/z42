//! Script exports and registration owned by the compression crate.
//! Declarations support lazy loading; implementations use the same signatures.
use z42_abi::module as abi;
#[cfg(feature = "backend")]
use z42_rs::module as sdk;
#[cfg(feature = "backend")]
use z42_rs::module::Value;

#[cfg(feature = "backend")]
fn result<T>(name: &str, value: Result<T, (i32, String)>) -> sdk::Result<T> {
    value.map_err(|(code, message)| {
        sdk::Error::new(
            "Std.IO.IOException",
            format!("{name}: {message} (rc={code})"),
        )
    })
}

macro_rules! function {
    ($entry:ident, $body:expr) => {
        #[cfg(feature = "backend")]
        unsafe extern "C" fn $entry(call: *mut abi::Call) -> u32 {
            unsafe { sdk::dispatch(call, $body) }
        }
    };
}

function!(deflate_compress, |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    let mode = call.integer(2)?;
    Ok(Value::Bytes(result(
        "__deflate_compress",
        crate::c_api::compression::deflate_compress(&input, level as i32, mode as i32),
    )?))
});
function!(deflate_decompress, |call| {
    let input = call.bytes(0)?;
    let mode = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__deflate_decompress",
        crate::c_api::compression::deflate_decompress(&input, mode as i32),
    )?))
});
function!(zstd_compress, |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__zstd_compress",
        zstd_compress_impl(&input, level as i32),
    )?))
});
function!(zstd_decompress, |call| {
    let input = call.bytes(0)?;
    Ok(Value::Bytes(result(
        "__zstd_decompress",
        zstd_decompress_impl(&input),
    )?))
});
function!(brotli_compress, |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__brotli_compress",
        crate::c_api::compression::brotli_compress(&input, level as i32),
    )?))
});
function!(brotli_decompress, |call| {
    let input = call.bytes(0)?;
    Ok(Value::Bytes(result(
        "__brotli_decompress",
        crate::c_api::compression::brotli_decompress(&input),
    )?))
});
function!(lz4_compress, |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__lz4_compress",
        crate::c_api::compression::lz4_compress(&input, level as i32),
    )?))
});
function!(lz4_decompress, |call| {
    let input = call.bytes(0)?;
    Ok(Value::Bytes(result(
        "__lz4_decompress",
        crate::c_api::compression::lz4_decompress(&input),
    )?))
});
function!(compressor_begin, |call| {
    let algo = call.integer(0)?;
    let level = call.integer(1)?;
    let decode = call.boolean(2)?;
    Ok(Value::I64(result(
        "__compressor_begin",
        crate::c_api::compression::compressor_begin(algo as i32, level as i32, decode),
    )? as i64))
});
function!(compressor_feed, |call| {
    let slot = call.integer(0)?;
    let chunk = call.bytes(1)?;
    Ok(Value::Bytes(result(
        "__compressor_feed",
        crate::c_api::compression::compressor_feed(slot as u64, &chunk),
    )?))
});
function!(compressor_finish, |call| {
    let slot = call.integer(0)?;
    Ok(Value::Bytes(result(
        "__compressor_finish",
        crate::c_api::compression::compressor_finish(slot as u64),
    )?))
});
function!(compressor_dispose, |call| {
    crate::c_api::compression::compressor_dispose(call.integer(0)? as u64);
    Ok(Value::Void)
});

macro_rules! descriptors {
    ($($entry:ident => ($name:literal, [$($parameter:expr),*], $result:expr)),* $(,)?) => {
        #[cfg(feature = "backend")]
        static FUNCTIONS: &[abi::Function] = &[$(abi::Function {
            name: abi::Text::new($name), parameters: abi::Parameters::new(&[$($parameter),*]),
            result: $result, flags: 0, invoke: Some($entry),
        }),*];
        static DECLARATIONS: &[abi::Function] = &[$(abi::Function {
            name: abi::Text::new($name), parameters: abi::Parameters::new(&[$($parameter),*]),
            result: $result, flags: 0, invoke: None,
        }),*];
    };
}
descriptors! {
deflate_compress => ("__deflate_compress",
        [abi::BYTES, abi::I64, abi::I64],
        abi::BYTES),
deflate_decompress => ("__deflate_decompress",
        [abi::BYTES, abi::I64],
        abi::BYTES),
zstd_compress => ("__zstd_compress",
        [abi::BYTES, abi::I64],
        abi::BYTES),
zstd_decompress => ("__zstd_decompress",
        [abi::BYTES],
        abi::BYTES),
brotli_compress => ("__brotli_compress",
        [abi::BYTES, abi::I64],
        abi::BYTES),
brotli_decompress => ("__brotli_decompress",
        [abi::BYTES],
        abi::BYTES),
lz4_compress => ("__lz4_compress",
        [abi::BYTES, abi::I64],
        abi::BYTES),
lz4_decompress => ("__lz4_decompress", [abi::BYTES], abi::BYTES),
compressor_begin => ("__compressor_begin",
        [abi::I64, abi::I64, abi::BOOL],
        abi::I64),
compressor_feed => ("__compressor_feed",
        [abi::I64, abi::BYTES],
        abi::BYTES),
compressor_finish => ("__compressor_finish",
        [abi::I64],
        abi::BYTES),
compressor_dispose => ("__compressor_dispose",
        [abi::I64],
        abi::VOID)
}
#[cfg(feature = "backend")]
static MODULE: abi::Module = abi::Module {
    version: abi::MODULE_ABI_VERSION,
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("z42_compression"),
    functions: FUNCTIONS.as_ptr(),
    function_count: FUNCTIONS.len(),
    create_state: None,
    destroy_state: None,
};

static DECLARATION: abi::Module = abi::Module {
    version: abi::MODULE_ABI_VERSION,
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("z42_compression"),
    functions: DECLARATIONS.as_ptr(),
    function_count: DECLARATIONS.len(),
    create_state: None,
    destroy_state: None,
};

/// Immutable signatures for lazy loading. No function wrappers or algorithm state.
pub fn declaration() -> *const abi::Module {
    &DECLARATION
}

/// Identical registration entry for cdylib discovery and static linkage.
#[cfg(feature = "backend")]
#[unsafe(no_mangle)]
pub extern "C" fn z42_compression_module_v1() -> *const abi::Module {
    &MODULE
}

/// Resolver for statically linked hosts. It uses the same registration entry as a native image.
///
/// # Safety
/// name must be NULL or point to a readable NUL-terminated string.
#[cfg(feature = "backend")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_get_symbol(
    name: *const std::ffi::c_char,
) -> *const std::ffi::c_void {
    if name.is_null() {
        return std::ptr::null();
    }
    if unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes() == b"z42_compression_module_v1" {
        z42_compression_module_v1 as *const () as *const std::ffi::c_void
    } else {
        unsafe { crate::c_api::resolve_symbol(name) }
    }
}

#[cfg(all(feature = "backend", not(target_arch = "wasm32")))]
fn zstd_compress_impl(input: &[u8], level: i32) -> Result<Vec<u8>, (i32, String)> {
    crate::c_api::compression::zstd_compress(input, level)
}
#[cfg(all(feature = "backend", not(target_arch = "wasm32")))]
fn zstd_decompress_impl(input: &[u8]) -> Result<Vec<u8>, (i32, String)> {
    crate::c_api::compression::zstd_decompress(input)
}
#[cfg(all(feature = "backend", target_arch = "wasm32"))]
fn zstd_compress_impl(_: &[u8], _: i32) -> Result<Vec<u8>, (i32, String)> {
    Err((
        crate::c_api::Z42_COMPRESSION_ERR_INVALID_MODE,
        "zstd not supported on wasm32 — see compression.md Deferred: compression-future-wasm-zstd"
            .into(),
    ))
}
#[cfg(all(feature = "backend", target_arch = "wasm32"))]
fn zstd_decompress_impl(input: &[u8]) -> Result<Vec<u8>, (i32, String)> {
    zstd_compress_impl(input, 0)
}

#[cfg(test)]
#[path = "registration_tests.rs"]
mod tests;
