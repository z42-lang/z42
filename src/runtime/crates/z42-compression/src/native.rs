//! Script exports and registration owned by the compression crate.
//! Adapter-only linkage forwards stateless calls to this module's full native image;
//! bundled linkage invokes the same implementations directly.
#[cfg(feature = "backend")]
use z42_rs::native::Value;
use z42_rs::native::{self, abi};

#[cfg(feature = "backend")]
fn result<T>(name: &str, value: Result<T, (i32, String)>) -> native::Result<T> {
    value.map_err(|(code, message)| {
        native::Error::new(
            "Std.IO.IOException",
            format!("{name}: {message} (rc={code})"),
        )
    })
}

macro_rules! function {
    ($entry:ident, $name:literal, $body:expr) => {
        unsafe extern "C" fn $entry(call: *mut abi::Call) -> u32 {
            #[cfg(feature = "backend")]
            {
                unsafe { native::dispatch(call, $body) }
            }
            #[cfg(not(feature = "backend"))]
            {
                unsafe {
                    native::forward(call, "z42_compression", "z42_compression_module_v1", $name)
                }
            }
        }
    };
}

function!(deflate_compress, "__deflate_compress", |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    let mode = call.integer(2)?;
    Ok(Value::Bytes(result(
        "__deflate_compress",
        crate::backend::compression::deflate_compress(&input, level as i32, mode as i32),
    )?))
});
function!(deflate_decompress, "__deflate_decompress", |call| {
    let input = call.bytes(0)?;
    let mode = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__deflate_decompress",
        crate::backend::compression::deflate_decompress(&input, mode as i32),
    )?))
});
function!(zstd_compress, "__zstd_compress", |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__zstd_compress",
        zstd_compress_impl(&input, level as i32),
    )?))
});
function!(zstd_decompress, "__zstd_decompress", |call| {
    let input = call.bytes(0)?;
    Ok(Value::Bytes(result(
        "__zstd_decompress",
        zstd_decompress_impl(&input),
    )?))
});
function!(brotli_compress, "__brotli_compress", |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__brotli_compress",
        crate::backend::compression::brotli_compress(&input, level as i32),
    )?))
});
function!(brotli_decompress, "__brotli_decompress", |call| {
    let input = call.bytes(0)?;
    Ok(Value::Bytes(result(
        "__brotli_decompress",
        crate::backend::compression::brotli_decompress(&input),
    )?))
});
function!(lz4_compress, "__lz4_compress", |call| {
    let input = call.bytes(0)?;
    let level = call.integer(1)?;
    Ok(Value::Bytes(result(
        "__lz4_compress",
        crate::backend::compression::lz4_compress(&input, level as i32),
    )?))
});
function!(lz4_decompress, "__lz4_decompress", |call| {
    let input = call.bytes(0)?;
    Ok(Value::Bytes(result(
        "__lz4_decompress",
        crate::backend::compression::lz4_decompress(&input),
    )?))
});
function!(compressor_begin, "__compressor_begin", |call| {
    let algo = call.integer(0)?;
    let level = call.integer(1)?;
    let decode = call.boolean(2)?;
    Ok(Value::I64(result(
        "__compressor_begin",
        crate::backend::compression::compressor_begin(algo as i32, level as i32, decode),
    )? as i64))
});
function!(compressor_feed, "__compressor_feed", |call| {
    let slot = call.integer(0)?;
    let chunk = call.bytes(1)?;
    Ok(Value::Bytes(result(
        "__compressor_feed",
        crate::backend::compression::compressor_feed(slot as u64, &chunk),
    )?))
});
function!(compressor_finish, "__compressor_finish", |call| {
    let slot = call.integer(0)?;
    Ok(Value::Bytes(result(
        "__compressor_finish",
        crate::backend::compression::compressor_finish(slot as u64),
    )?))
});
function!(compressor_dispose, "__compressor_dispose", |call| {
    crate::backend::compression::compressor_dispose(call.integer(0)? as u64);
    Ok(Value::Void)
});

macro_rules! descriptor {
    ($entry:ident, $name:literal, [$($parameter:expr),*], $result:expr) => {
        abi::Function { name: abi::Text::new($name), parameters: abi::Parameters::new(&[$($parameter),*]),
            result: $result, flags: 0, invoke: Some($entry) }
    };
}
static FUNCTIONS: &[abi::Function] = &[
    descriptor!(
        deflate_compress,
        "__deflate_compress",
        [abi::BYTES, abi::I64, abi::I64],
        abi::BYTES
    ),
    descriptor!(
        deflate_decompress,
        "__deflate_decompress",
        [abi::BYTES, abi::I64],
        abi::BYTES
    ),
    descriptor!(
        zstd_compress,
        "__zstd_compress",
        [abi::BYTES, abi::I64],
        abi::BYTES
    ),
    descriptor!(
        zstd_decompress,
        "__zstd_decompress",
        [abi::BYTES],
        abi::BYTES
    ),
    descriptor!(
        brotli_compress,
        "__brotli_compress",
        [abi::BYTES, abi::I64],
        abi::BYTES
    ),
    descriptor!(
        brotli_decompress,
        "__brotli_decompress",
        [abi::BYTES],
        abi::BYTES
    ),
    descriptor!(
        lz4_compress,
        "__lz4_compress",
        [abi::BYTES, abi::I64],
        abi::BYTES
    ),
    descriptor!(lz4_decompress, "__lz4_decompress", [abi::BYTES], abi::BYTES),
    descriptor!(
        compressor_begin,
        "__compressor_begin",
        [abi::I64, abi::I64, abi::BOOL],
        abi::I64
    ),
    descriptor!(
        compressor_feed,
        "__compressor_feed",
        [abi::I64, abi::BYTES],
        abi::BYTES
    ),
    descriptor!(
        compressor_finish,
        "__compressor_finish",
        [abi::I64],
        abi::BYTES
    ),
    descriptor!(
        compressor_dispose,
        "__compressor_dispose",
        [abi::I64],
        abi::VOID
    ),
];
static MODULE: abi::Module = abi::Module {
    version: abi::MODULE_ABI_VERSION,
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("z42_compression"),
    functions: FUNCTIONS.as_ptr(),
    function_count: FUNCTIONS.len(),
    create_state: None,
    destroy_state: None,
};

/// Identical registration entry for cdylib discovery and static linkage.
#[unsafe(no_mangle)]
pub extern "C" fn z42_compression_module_v1() -> *const abi::Module {
    &MODULE
}

/// Resolver for statically linked hosts. It uses the same registration entry as a native image.
///
/// # Safety
/// name must be NULL or point to a readable NUL-terminated string.
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
        std::ptr::null()
    }
}

#[cfg(all(feature = "backend", not(target_arch = "wasm32")))]
fn zstd_compress_impl(input: &[u8], level: i32) -> Result<Vec<u8>, (i32, String)> {
    crate::backend::compression::zstd_compress(input, level)
}
#[cfg(all(feature = "backend", not(target_arch = "wasm32")))]
fn zstd_decompress_impl(input: &[u8]) -> Result<Vec<u8>, (i32, String)> {
    crate::backend::compression::zstd_decompress(input)
}
#[cfg(all(feature = "backend", target_arch = "wasm32"))]
fn zstd_compress_impl(_: &[u8], _: i32) -> Result<Vec<u8>, (i32, String)> {
    Err((
        crate::backend::Z42_COMPRESSION_ERR_INVALID_MODE,
        "zstd not supported on wasm32 — see compression.md Deferred: compression-future-wasm-zstd"
            .into(),
    ))
}
#[cfg(all(feature = "backend", target_arch = "wasm32"))]
fn zstd_decompress_impl(input: &[u8]) -> Result<Vec<u8>, (i32, String)> {
    zstd_compress_impl(input, 0)
}
