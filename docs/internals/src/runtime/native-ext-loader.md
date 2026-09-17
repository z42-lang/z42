# Native ext loader

Infrastructure that lets a stdlib package's native code live **outside**
the `z42vm` binary, in a separate `cdylib` that z42vm `dlopen`s at
startup. First user: [`z42.compression`](../../../reference/src/stdlib/compression.md)
(shipped 2026-05-24). Designed so future heavy native stdlibs
(`z42.net`, `z42.numerics`, second-wave `z42.crypto` algorithms) follow
the same template.

## Why not just BUILTINS[]?

Existing stdlib natives (crypto / threading / fs / process / etc.)
register at compile time in `src/runtime/src/corelib/mod.rs::BUILTINS`
and are statically linked into z42vm. That's fine for small things
(SHA-256 = a few hundred lines of Rust, ~10 KB code).

It's wrong for big things:

| Pain | Concrete example |
|------|------------------|
| Binary size always paid | Apps that never compress still carry zlib-ng + libzstd (~600 KB) in z42vm |
| Rebuild ripple | Bumping flate2 minor invalidates every cached z42vm build artefact |
| Wasm bloat | wasm bundle must include compression even if the script doesn't use it |
| Cross-cutting upgrades | Switching DEFLATE backend (zlib-ng → zlib-intel) touches z42vm Cargo.toml + all cross-target presets |

The ext loader is the cleanly modular alternative. Apps that import
`Std.Compression` get the cdylib loaded; apps that don't pay nothing
extra.

## Architecture

```
z42 user code                         z42vm binary
[Native(lib="z42_compression",        ┌──────────────────────────┐
        entry="__deflate_compress")]  │ corelib::BUILTINS[]      │
       ↓                              │ (small in-VM natives)    │
compiler short-circuit                │                          │
(lib + entry without type=)           │ VmCore.ext_builtins      │
       ↓                              │   ├ by_name: HashMap     │
BuiltinInstr("__deflate_compress")    │   └ by_idx: Vec<NativeFn>│
       ↓ resolver                     │                          │
ext_builtin_id_of(ctx, name)          │ native::ext::load_all()  │
       ↓ returns id with              │   1. native_search_paths │
BUILTIN_ID_EXT_BIT (0x8000_0000) set  │      ├ Z42_NATIVE_PATH env
       ↓                              │      ├ <exe>/../native/
exec_builtin_by_id checks high bit:   │      └ <exe>/native/
  id & EXT_BIT → ext_builtins.dispatch│   2. dlopen each lib*.so/.dylib/.dll
  else         → BUILTINS[idx]        │   3. resolve known symbols
       ↓                              │   4. register VM-side wrappers
NativeFn wrapper                      │      into ext_builtins
(Vec<u8> ↔ Value::Array<I64>)         └──────────────────────────┘
       ↓                                          ↓ dlopen
extern "C" deflate_compress(           libz42_compression.{so,dylib,dll}
  *const u8, usize, ...) -> i32        ┌──────────────────────────┐
                                       │ #[no_mangle] extern "C"  │
                                       │ z42_compression_         │
                                       │   deflate_compress(...)  │
                                       │   ↓                      │
                                       │ flate2 (zlib-ng) / zstd  │
                                       └──────────────────────────┘
```

## Pieces

### Compiler short-circuit

z42 facade declares:

```z42
[Native(lib = "z42_compression", entry = "__deflate_compress")]
private static extern byte[] _CompressRaw(byte[] data, int level, int mode);
```

The `lib + entry` form (no `type=`) tells the compiler to:

1. **Skip Tier 1 type-registry / libffi codegen**. Tier 1 marshal can't
   handle `byte[]` yet (spec C5 not done), so going through it would
   block the whole package.
2. **Emit `BuiltinInstr(entry)`**. The instruction itself looks
   identical to the legacy `[Native("__name")]` short-form output; the
   `lib=` annotation is preserved as metadata for future tooling (SDK
   dependency manifests) but doesn't affect IR.

See ``src/compiler/z42.Semantics/Codegen/IrGen.Classes.cs::EmitNativeStub``（C# 编译器已移除）.

### Native search path

[`src/runtime/src/native/ext.rs::native_search_paths`](../../../../src/runtime/src/native/ext.rs)
returns a `Vec<PathBuf>` in priority order:

1. **`$Z42_NATIVE_PATH`** env var (colon-separated on Unix, semicolon
   on Windows). Highest priority — used by CI, dev iteration, embedders.
2. **`<exec_dir>/../native/`** — SDK package layout
   (`<sdk>/bin/z42vm` + `<sdk>/native/lib*.dylib`).
3. **`<exec_dir>/native/`** — dev / cargo-target layout
   (`artifacts/build/runtime/release/native/lib*.dylib`).

First match wins per file name — later directories don't override.

### `parse_z42_lib_name`

Filters `dlopen` candidates to `libz42_*.{so,dylib,dll}` (or
`z42_*.dll` on Windows without `lib` prefix). Files that don't match
are silently skipped — third-party libs in the same directory don't
interfere.

### dlopen + registration

For each matched file, [`load_one`](../../../../src/runtime/src/native/ext.rs)
runs:

1. `libloading::Library::new(path)` — opens the cdylib
2. Dispatch on the extracted `<basename>` (e.g. `"compression"`):
   - `"compression"`: resolve the 10 known compression symbols
     (`z42_compression_deflate_compress` etc.) and stash them in a
     process-static `LoadedCompression` struct
   - other basenames: warn + skip (future ext libs add a match arm)
3. Register `(name, wrapper_fn)` pairs into `VmCore.ext_builtins`. The
   wrappers are static Rust functions with `NativeFn` signature
   (`fn(&VmContext, &[Value]) -> Result<Value>`) that marshal Value ↔
   raw bytes and dispatch into the `LoadedCompression` fn ptrs.
4. Push the `libloading::Library` into `VmCore.native_libs` (existing
   field, was added for Tier 1) so the fn ptrs stay valid for the VM's
   lifetime.

Failures (lib not found, symbol missing, etc.) are logged via
`tracing::warn!` but **never abort VM startup** — apps that don't
import the missing ext namespace boot fine and only see the error at
the runtime call site as `unknown builtin '__deflate_compress'`.

### ext_builtins table

[`VmCore.ext_builtins: Mutex<ExtBuiltinTable>`](../../../../src/runtime/src/native/ext.rs):

```rust
pub struct ExtBuiltinTable {
    by_name: HashMap<String, u32>,
    by_idx:  Vec<NativeFn>,
}
```

Parallel to the static `BUILTINS[]` slice. Resolver checks both:

```rust
crate::corelib::builtin_id_of(name)                  // static BUILTINS[]
    .or_else(|| crate::corelib::ext_builtin_id_of(ctx, name))  // ext
    .ok_or(...)?
```

### BuiltinId high-bit dispatch

[`BUILTIN_ID_EXT_BIT = 0x8000_0000`](../../../../src/runtime/src/corelib/mod.rs).
When set on a `BuiltinId.0`, dispatch routes through
`ext_builtins.dispatch(low_31_bits)` instead of `BUILTINS[id]`. The
existing fast-path indexing for static builtins stays one array index
deep; ext dispatch is one HashMap-then-Vec lookup (cached after first
resolve via the stable `by_idx` index).

### Marshalling wrappers

[`wrap_deflate_compress`](../../../../src/runtime/src/native/ext.rs) and
its siblings handle the Value ↔ raw-bytes conversion:

```rust
fn wrap_deflate_compress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let input = require_byte_array(args, 0, NAME)?;   // Value::Array<I64> → Vec<u8>
    let level = arg_i64(args, 1, NAME)? as i32;
    let mode  = arg_i64(args, 2, NAME)? as i32;
    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize   = 0;
    let rc = unsafe { (LC.deflate_compress)(
        input.as_ptr(), input.len(), level, mode, &mut out_ptr, &mut out_len) };
    if rc != 0 { bail!("{}: {} (rc={})", NAME, last_error_string(), rc); }
    Ok(bytes_to_value(ctx, take_owned_buffer(out_ptr, out_len)))
                                  //  Vec<u8> → Value::Array<I64> on the heap
}
```

`take_owned_buffer` reconstructs a `Vec<u8>` from the (ptr, len) the
cdylib returned via `Vec::into_boxed_slice().as_mut_ptr()`. Rust's
allocator drops it when the Vec falls out of scope — no need to call
the cdylib's `z42_compression_free` entry (which exists for non-Rust
consumers).

## Platform variance

| Platform | dlopen available? | Compression delivery |
|----------|-------------------|----------------------|
| linux-x64 / linux-arm64 / macos-arm64 / windows-x64 | yes | `libz42_compression.{so,dylib,dll}` dlopened from `<sdk>/native/` |
| ios-arm64 / iossim-arm64 | **no** (App Store bans dlopen of arbitrary dylibs) | `bundled-compression` Cargo feature: z42 main crate links the `z42-compression` rlib at compile time; `ext::load_all` calls `register_bundled_compression` instead of dlopen. Side product: `libz42_compression.a` shipped in the SDK package for integrators who want to manually link. |
| android-arm64 / android-x64 | yes (JNI side-load convention) | Same as iOS — `bundled-compression` feature on. JNI integrators can additionally `System.loadLibrary("z42_compression")` to use the .so. |
| browser-wasm | **no** | `bundled-compression` static link; `flate2` uses `miniz_oxide` (pure Rust) instead of zlib-ng (which has no SIMD on wasm and is bigger). |

The `bundled-compression` Cargo feature in `src/runtime/Cargo.toml`
adds `z42-compression` as an optional rlib dep and changes
`ext::load_all` to call `register_bundled_compression` (statically
linked entry) rather than dlopen-scan paths. The bundled path
populates the same `ext_builtins` table with the same wrapper
functions — consumers can't tell the difference.

## Adding a new stdlib ext lib

Suppose `z42.net` wants to follow this pattern. Steps:

1. Create `src/runtime/crates/z42-net/` (cdylib + staticlib + rlib),
   pure C ABI symbols
2. Add a match arm to `ext::load_one`:
   ```rust
   "net" => {
       let symbols = unsafe { net_symbols_via_dlopen(&lib)? };
       /* register into ext_builtins */
   }
   ```
3. Add wrapper functions per symbol (parallel to the compression set)
4. Add `bundled-net` feature in z42 main crate (mirroring
   `bundled-compression`) for wasm / mobile static link
5. Update the `./xtask package` desktop / iOS / Android
   paths to build + ship `libz42_net.*`
6. CI `Verify package manifest` step asserts the new lib is present
7. z42 facade uses `[Native(lib="z42_net", entry="__socket_connect")]`
   (etc.)

There's currently no plugin-style "drop a cdylib in `<sdk>/native/`
and z42vm auto-discovers it" path — the symbol resolution + wrapper
functions are hardcoded per lib. This is intentional for stdlib's
curated finite set; for user-extensible plugins, the existing Tier 1
type-registry path
([`src/runtime/src/native/loader.rs`](../../../../src/runtime/src/native/loader.rs))
is the right tool once spec C5 (byte[] / String marshal) lands.

## Relationship to Tier 1 native interop

| Aspect | Tier 1 (`native::loader`) | Ext loader (`native::ext`) |
|--------|---------------------------|----------------------------|
| Target | User-supplied native libs (e.g. `numz42`) | Curated stdlib backings (compression, future net / numerics) |
| z42 syntax | `[Native(lib=, type=, entry=)]` (full form) | `[Native(lib=, entry=)]` (short form, no `type=`) |
| Codegen | `CallNativeInstr` (libffi dispatch) | `BuiltinInstr` (resolved name lookup) |
| Marshal | `Z42Value` tagged union via libffi (currently i64/f64/bool/ptr only; byte[] blocked on spec C5) | Direct Rust types (`Vec<u8>`, `i32`, etc.) via per-symbol wrapper functions |
| Generality | Any registered type / method | Hardcoded match arms per known lib |
| Performance | One libffi `cif` call (~30 ns overhead) | Direct fn ptr call (~5 ns) + Value↔bytes marshal |
| Lifecycle | `<libname>_register` entry calls `z42_register_type` | `register_<basename>_builtins`-style dispatch (no z42 API surface for cdylib to call) |

The two paths share `VmCore.native_libs` for library handle lifetime
management but otherwise don't overlap.

## Migration of existing stdlib natives

Out of scope for the compression spec. Once we have ≥ 2 ext libs and
the pattern is comfortable, an `add-migrate-stdlib-natives-to-ext`
spec can selectively move existing in-VM natives where the
modularity benefit justifies the friction (crypto's SHA-256 is small
enough to stay in-VM; future crypto additions like RSA / EC might be
worth extracting).
