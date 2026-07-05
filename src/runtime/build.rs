//! Build script: compiles the `numz42-c` PoC native library used by spec
//! C2's end-to-end integration test.
//!
//! The library is built as a static archive and linked into any cargo test
//! binary that consumes z42_vm. Tests call `numz42_register_static()`
//! directly, so the dlopen path is exercised separately when present.
//!
//! Set `Z42_SKIP_NATIVE_POC=1` to opt out (e.g. on CI agents without a C
//! toolchain); `tests/native_interop_e2e.rs` will then skip its scenarios.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Allow rustc to recognise our custom cfg without warning.
    println!("cargo:rustc-check-cfg=cfg(z42_skip_native_poc)");
    println!("cargo:rustc-check-cfg=cfg(z42_have_z42c)");
    println!("cargo:rustc-check-cfg=cfg(z42_have_embedding_hello)");
    println!("cargo:rerun-if-env-changed=Z42_SKIP_NATIVE_POC");
    // Auto-skip the C PoC when the build environment can't compile it:
    //   - explicit opt-out (Z42_SKIP_NATIVE_POC=1)
    //   - native-interop feature disabled (numz42-c is only ever consumed
    //     by tests/native_interop_e2e.rs, which is itself gated)
    //   - wasm32 target (no libc / stdlib.h in the wasm sandbox)
    //
    // 2026-05-12 add-platform-wasm Stage 0: the last two are auto-detection
    // so contributors building for wasm don't have to remember the env var.
    let native_interop_off = env::var("CARGO_FEATURE_NATIVE_INTEROP").is_err();
    let wasm_target = env::var("CARGO_CFG_TARGET_ARCH")
        .map(|a| a == "wasm32")
        .unwrap_or(false);
    if env::var_os("Z42_SKIP_NATIVE_POC").is_some() || native_interop_off || wasm_target {
        let reason = if env::var_os("Z42_SKIP_NATIVE_POC").is_some() {
            "Z42_SKIP_NATIVE_POC set"
        } else if wasm_target {
            "target is wasm32"
        } else {
            "native-interop feature is disabled"
        };
        println!("cargo:warning=skipping numz42-c build ({reason})");
        println!("cargo:rustc-cfg=z42_skip_native_poc");
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src   = manifest_dir.join("tests/data/numz42-c/numz42.c");
    let inc   = manifest_dir.join("include");

    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}/z42_abi.h", inc.display());

    cc::Build::new()
        .file(&src)
        .include(&inc)
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra")
        .flag_if_supported("-Wno-unused-function")
        .compile("numz42_c"); // emits libnumz42_c.a in OUT_DIR

    // Tests that link against this archive opt-in via #[link] in source —
    // we only set the static-lib search path here.
    let out_dir = env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={out_dir}");

    // ── z42 test fixtures (C7 native e2e + embedding hello) ────────────
    //
    // The Rust integration tests `z42_source_calls_numz42_via_native_attr`
    // (cfg z42_have_z42c) and `host::host_tests::load_invoke_hello_world`
    // (cfg z42_have_embedding_hello) each consume a `.zbc` compiled from a
    // fixture source. C#/dotnet was removed (2026-06-26) — these now compile
    // with z42c (z42vm + z42c.driver.zpkg), mirroring `xtask regen`'s
    // _compileCase. When no warm z42c toolchain is present (e.g. the initial
    // cold cargo build, before stdlib/z42c are built), the fixtures are
    // skipped and their tests opt out — no hard build dependency.
    let project_root = manifest_dir.parent().and_then(Path::parent);
    if let Some(root) = project_root {
        // z42c.driver is self-contained: its 6 z42c.* siblings are bundled into its
        // own dist by `z42c build` (simplify-compiler-build). So Z42_LIBS supplies
        // only the stdlib — the driver's siblings resolve from its own dir (z42vm's
        // entry-zpkg dir + libs search). Both produced by `xtask build stdlib`.
        let driver = root
            .join("artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg");
        let home = root.join("artifacts/build/libraries/dist/release");
        let vm = find_z42vm(root);
        let ready = driver.is_file() && vm.as_ref().is_some_and(|v| v.is_file());
        println!("cargo:rerun-if-changed={}", driver.display());

        let c7_src = manifest_dir.join("tests/data/z42_native_e2e/source.z42");
        if c7_src.is_file() {
            println!("cargo:rerun-if-changed={}", c7_src.display());
            if ready {
                let out = PathBuf::from(&out_dir).join("z42_native_e2e_source.zbc");
                if z42c_emit_zbc(vm.as_ref().unwrap(), &driver, &home, &c7_src, &out) {
                    println!("cargo:rustc-cfg=z42_have_z42c");
                } else {
                    println!("cargo:warning=z42c failed compiling C7 fixture; e2e test will be skipped");
                }
            } else {
                println!("cargo:warning=no warm z42c toolchain (run `xtask build stdlib`); C7 e2e test will be skipped");
            }
        }

        let emb_src = manifest_dir.join("tests/data/embedding_hello/source.z42");
        if emb_src.is_file() && ready {
            println!("cargo:rerun-if-changed={}", emb_src.display());
            let out = PathBuf::from(&out_dir).join("embedding_hello.zbc");
            if z42c_emit_zbc(vm.as_ref().unwrap(), &driver, &home, &emb_src, &out) {
                println!("cargo:rustc-cfg=z42_have_embedding_hello");
            }
        }
    }
}

/// Locate a built z42vm (release preferred, then debug) under artifacts/build/runtime.
fn find_z42vm(root: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "z42vm.exe" } else { "z42vm" };
    for profile in ["release", "debug"] {
        let p = root.join("artifacts/build/runtime").join(profile).join(exe);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Compile a single `.z42` → `.zbc` via z42c (z42vm running z42c.driver.zpkg),
/// mirroring `xtask regen`'s _compileCase. `driver_home` is Z42_LIBS (stdlib); the
/// driver's z42c.* siblings resolve from the driver's own dir (bundled, self-
/// contained exe). Returns true on success.
fn z42c_emit_zbc(vm: &Path, driver: &Path, driver_home: &Path, src: &Path, out: &Path) -> bool {
    Command::new(vm)
        .arg(driver)
        .arg("--mode")
        .arg("interp")
        .arg("--")
        .arg("--emit-zbc")
        .arg(src)
        .arg(out)
        .env("Z42_LIBS", driver_home)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
