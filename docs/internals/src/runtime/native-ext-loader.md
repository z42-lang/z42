# Native ext loader

Infrastructure that lets a stdlib package's native code live **outside**
the `z42vm` binary, in a separate `cdylib` that z42vm `dlopen`s the
first time one of its builtins is needed. First user: [`z42.compression`](https://z42-lang.github.io/z42/reference/stdlib/compression.html).
Designed so future heavy native stdlibs
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
[Native(lib="z42_compression",        ┌──────────────────────────────┐
        entry="__deflate_compress")]  │ corelib::BUILTINS[]          │
       ↓                              │   … small in-VM natives      │
compiler short-circuit                │   … interop::ext::typed      │
(lib + entry without type=)           │       (__deflate_compress …) │
       ↓                              │                              │
BuiltinInstr("__deflate_compress")    │ typed body, first call:      │
       ↓ resolver: builtin_id_of      │   loaded(ctx, name)          │
BUILTINS[idx] (one table, one id      │   miss → ensure_lib_for()    │
space for every builtin)              │   1. builtin name → lib      │
       ↓                              │      (KNOWN_EXT_LIBS)        │
typed body                            │   2. probe each search path  │
(Vec<u8> in / Vec<u8> out)            │   3. dlopen, ABI handshake,  │
       ↓                              │      resolve known symbols   │
LoadedCompression fn ptr              │      into LoadedCompression  │
                                      └──────────────────────────────┘
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

See `src/compiler/z42c.emission/src/Emission/StubEmitter.z42` (`_emitNativeStub`).

### Native search path

[`src/runtime/src/interop/ext.rs::native_search_paths`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interop/ext.rs)
returns a `Vec<PathBuf>` in priority order:

1. **`$Z42_NATIVE_PATH`** env var (colon-separated on Unix, semicolon
   on Windows). Highest priority — used by CI, dev iteration, embedders.
2. **`<exec_dir>/../native/`** — SDK package layout
   (`<sdk>/bin/z42vm` + `<sdk>/native/lib*.dylib`).
3. **`<exec_dir>/native/`** — dev / cargo-target layout
   (`artifacts/build/runtime/release/native/lib*.dylib`).

First match wins per file name — later directories don't override.

### Lazy loading

Nothing is loaded at VM startup (the bundled wasm build fills
`LoadedCompression` from its statically linked entries in `load_all`, which
involves no I/O). The builtins themselves are ordinary typed builtins in
`BUILTINS` (`interop::ext::typed`), so they always resolve; the first call
whose library is not loaded yet goes to
[`ensure_lib_for`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interop/ext.rs)
maps the builtin name to its library through `KNOWN_EXT_LIBS`
(`[("compression", COMPRESSION_BUILTINS)]`), probes each search path for
exactly `{DLL_PREFIX}z42_<lib>{DLL_SUFFIX}` — no directory listing — and
loads the first one found. Each library is looked for once per VM
(`VmCore.ext_libs_tried`), so a missing library costs one probe, not one per
call. Names not in the map never trigger a probe; a `libz42_*` file in a
search directory is only ever opened when one of its builtins is asked for
(the cargo-target directory, one of the search paths, holds every cdylib of
the workspace).

Before this, every VM listed every search directory and dlopened
`libz42_compression` at startup, whether or not the program used it.

### dlopen + registration

For the library found, [`load_one`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interop/ext.rs)
runs:

1. `libloading::Library::new(path)` — opens the cdylib
2. Dispatch on the extracted `<basename>` (e.g. `"compression"`):
   - `"compression"`: **ABI handshake first** — resolve
     `z42_compression_abi_version` and refuse to bind anything unless it
     equals the loader's `EXPECTED_COMPRESSION_ABI`
     (== `z42_compression::Z42_COMPRESSION_ABI_VERSION`); on mismatch **or**
     a missing version symbol (pre-versioning build) the whole library is
     skipped. Only then resolve the known compression symbols
     (`z42_compression_deflate_compress` etc.) and stash them in a
     process-static `LoadedCompression` struct. **Why:** the search path
     includes `<exec_dir>` itself, so a stale `libz42_compression` beside
     z42vm (dev incremental builds, partial SDK upgrades) has the same
     symbol names but a possibly-changed layout — calling it through the
     current `C*Fn` signatures would be UB. Link-time version-locking only
     covers the bundled / packaged case, not this dlopen. Bump both ABI
     constants together on any `z42_compression_*` signature change; the
     `bundled-compression` unit test `abi_handshake_tests` gates that they
     agree.
   - other basenames: warn + skip (future ext libs add a match arm)
3. The `libloading::Library` moves into the process-static
   `LoadedCompression` table (a `OnceLock`, set by the first VM that loads
   it) next to the fn ptrs, so they stay valid for the whole process. The
   wrappers read the table without a lock; a later VM reuses it.

Failures (lib not found, symbol missing, etc.) are logged, and the call
fails with `__deflate_compress: z42-compression not loaded`.

### Builtin bodies

The typed builtins in `interop::ext::typed` (`#[builtin(name = "__deflate_compress")]`
etc.) take and return `Vec<u8>` — the builtin ABI does the `byte[]` ↔ `Vec<u8>`
conversion — and call a body that dispatches into the `LoadedCompression` fn ptrs:

```rust
fn deflate_compress(ctx: &VmContext, input: Vec<u8>, level: i64, mode: i64) -> Result<Vec<u8>, BuiltinError> {
    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;   // loads the lib on first use
    let (mut out_ptr, mut out_len) = (std::ptr::null_mut(), 0usize);
    let rc = unsafe { (lc.deflate_compress)(
        input.as_ptr(), input.len(), level as i32, mode as i32, &mut out_ptr, &mut out_len) };
    if rc != 0 { return Err(BuiltinError::Io(/* last_error_string() */)); }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}
```

`take_owned_buffer` copies the cdylib's output into a Rust-owned `Vec<u8>` and
frees the original through the library's own `z42_compression_free`, so the two
sides never mix allocators.

## Platform variance

| Platform | dlopen available? | Compression delivery |
|----------|-------------------|----------------------|
| linux-x64 / linux-arm64 / macos-arm64 / windows-x64 | yes | `libz42_compression.{so,dylib,dll}` dlopened from `<sdk>/native/` |
| ios-arm64 / iossim-arm64 | **no** (App Store bans dlopen of arbitrary dylibs) | `bundled-compression` Cargo feature: z42 main crate links the `z42-compression` rlib at compile time; `ext::load_all` fills `LoadedCompression` from it instead of dlopen. No separate libz42_compression.a is built or shipped (it would duplicate what libz42.a already contains). |
| android-arm64 / android-x64 | yes (JNI side-load convention) | Same as iOS — `bundled-compression` feature on. JNI integrators can additionally Java 的 System.loadLibrary("z42_compression") 调用 to use the .so. |
| browser-wasm | **no** | `bundled-compression` static link; `flate2` uses `miniz_oxide` (pure Rust) instead of zlib-ng (which has no SIMD on wasm and is bigger). |

The `bundled-compression` Cargo feature in `src/runtime/Cargo.toml`
adds `z42-compression` as an optional rlib dep and changes
`ext::load_all` to fill `LoadedCompression` from the statically linked
entries rather than dlopen one. The builtin bodies read the same table
either way — they can't tell the difference.

## Adding a new stdlib ext lib

Suppose `z42.net` wants to follow this pattern. Steps:

1. Create `src/runtime/crates/z42-net/` (cdylib + staticlib + rlib),
   pure C ABI symbols
2. Add a match arm to `ext::load_one` that resolves the symbols into a
   process-static table (parallel to `LoadedCompression`), and add the lib
   with its builtin names to `KNOWN_EXT_LIBS`
3. Add typed `#[builtin]` functions per symbol (parallel to
   `interop::ext::typed`) — they join `BUILTINS` like any other builtin
4. Add `bundled-net` feature in z42 main crate (mirroring
   `bundled-compression`) for wasm / mobile static link
5. Update the `./xtask package` desktop / iOS / Android
   paths to build + ship `libz42_net.*`
6. Extend the package layout gate (`scripts/package/xtask_package_layout.z42`) to assert the new lib ships
7. z42 facade uses `[Native(lib="z42_net", entry="__socket_connect")]`
   (etc.)

There's currently no plugin-style "drop a cdylib in `<sdk>/native/`
and z42vm auto-discovers it" path — the symbol resolution + wrapper
functions are hardcoded per lib. This is intentional for stdlib's
curated finite set; for user-extensible plugins, the existing Tier 1
type-registry path
([`src/runtime/src/interop/loader.rs`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/interop/loader.rs))
is the right tool once spec C5 (byte[] / String marshal) lands.

## Relationship to Tier 1 native interop

| Aspect | Tier 1 (`interop::loader`) | Ext loader (`interop::ext`) |
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

Not done: existing in-VM stdlib natives stay in `BUILTINS[]`. Once there are ≥ 2 ext libs and
the pattern is comfortable, existing in-VM natives can selectively move where the
modularity benefit justifies the friction (crypto's SHA-256 is small
enough to stay in-VM; future crypto additions like RSA / EC might be
worth extracting).
