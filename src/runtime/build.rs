use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn sources(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, files)?;
        } else if path.extension().is_some_and(|x| x == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn field(hash: &mut blake3::Hasher, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn identity() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest.as_path();
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        manifest.join("build.rs"),
    ];
    sources(&root.join("src"), &mut files)?;
    println!("cargo:rerun-if-changed={}", root.join("src").display());
    println!("cargo:rerun-if-changed={}", root.join("crates").display());
    for entry in fs::read_dir(root.join("crates"))? {
        let path = entry?.path();
        if path.join("src").is_dir() {
            sources(&path.join("src"), &mut files)?;
        }
        if path.join("Cargo.toml").is_file() {
            files.push(path.join("Cargo.toml"));
        }
    }
    files.sort();
    files.dedup();
    let mut hash = blake3::Hasher::new();
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        field(
            &mut hash,
            file.strip_prefix(root)?.to_string_lossy().as_bytes(),
        );
        field(&mut hash, &fs::read(file)?);
    }
    let rustc = Command::new(env::var("RUSTC")?).arg("-vV").output()?;
    if !rustc.status.success() {
        return Err("cannot identify rustc".into());
    }
    field(&mut hash, &rustc.stdout);
    let target = env::var("TARGET")?;
    field(&mut hash, target.as_bytes());
    field(&mut hash, env::var("CARGO_CFG_TARGET_FEATURE")?.as_bytes());
    let bytes = hash.finalize();
    let generated = format!("pub const COMPONENT_BUILD_ID: [u8; 32] = {:?};\npub const COMPONENT_TARGET: &str = {:?};\n", bytes.as_bytes(), target);
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("identity.rs"),
        generated,
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> { identity()?; host_build() }

fn host_build() -> Result<(), Box<dyn Error>> {
    // Cargo supplies an absolute manifest directory. Taking its parents avoids
    // Windows verbatim paths, which the C fixture compiler cannot consume.
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let runtime = manifest;
    println!("cargo:rustc-env=Z42_RUNTIME_DIR={}", runtime.display());
    // The facade owns the cross-layer white-box harness over shared source files.
    println!("cargo:rustc-check-cfg=cfg(z42_test_facade)");
    println!("cargo:rustc-cfg=z42_test_facade");
    println!("cargo:rustc-check-cfg=cfg(z42_jit_supported)");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(arch.as_str(), "x86_64" | "aarch64") && matches!(os.as_str(), "linux" | "macos" | "windows") {
        println!("cargo:rustc-cfg=z42_jit_supported");
    }
    if os == "linux" {
        // Shared-std TLS can retain host error destructors after dlclose.
        println!("cargo:rustc-link-arg=-Wl,-z,nodelete");
    }
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
    // add-platform-wasm Stage 0: the last two are auto-detection
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
        return Ok(());
    }

    let manifest_dir = runtime;
    let src   = manifest_dir.join("tests/data/numz42-c/numz42.c");
    let inc   = manifest_dir.join("include");

    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}/z42_abi.h", inc.display());
    println!("cargo:rerun-if-changed={}/z42_native.h", inc.display());

    // `cargo_metadata(false)`: by default cc prints `cargo:rustc-link-lib=static=numz42_c`,
    // which makes the archive a native dependency of the `z42` library itself —
    // rustc then bundled numz42.o into every staticlib built from it, i.e. the
    // released libz42.a. The test that needs it links it with `#[link]` instead.
    cc::Build::new()
        .file(&src)
        .include(&inc)
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra")
        .flag_if_supported("-Wno-unused-function")
        .cargo_metadata(false)
        .compile("numz42_c"); // emits libnumz42_c.a in OUT_DIR

    // Tests that link against this archive opt in via #[link] in source —
    // we only set the static-lib search path here.
    let out_dir = env::var("OUT_DIR")?;
    println!("cargo:rustc-link-search=native={out_dir}");

    // ── z42 test fixtures (C7 native e2e + embedding hello) ────────────
    //
    // The Rust integration tests `z42_source_calls_numz42_via_native_attr`
    // (cfg z42_have_z42c) and `host::host_tests::load_invoke_hello_world`
    // (cfg z42_have_embedding_hello) each consume a `.zbc` compiled from a fixture source.
    // C#/dotnet was removed  — these now compile
    // with z42c (z42vm + z42c.driver.zpkg), mirroring `xtask regen`'s
    // _compileCase. When no warm z42c toolchain is present (e.g. the initial
    // cold cargo build, before stdlib/z42c are built), the fixtures are
    // skipped and their tests opt out — no hard build dependency.
    //
    // ⚠️ 整块由 `z42-test-fixtures` feature 门控，默认**关闭**。
    //
    // 原因（fix-build-rs-zpkg-invalidation）：下面的
    // `rerun-if-changed=<z42c.driver.zpkg>` 在构建期形成一个环——
    //     z42vm(Rust) ──要它才能跑──> z42c ──产出──> z42c.driver.zpkg
    //          ↑                                          │
    //          └────── build.rs rerun-if-changed ─────────┘
    // 而 `xtask build compiler` 每轮都重写 driver.zpkg，于是**下一次** cargo 判定
    // build script 陈旧 → 整个 crate graph（lib z42 → z42vm → 各 cdylib）全量重编。
    // 实测：warm 树上仅 `touch` 一下 driver.zpkg 就会让 release 全量重编（debug 只需秒级，故只在
    // release 上致命）。CI 的 ci-bootstrap 每个 job 都踩一次，一次 run 踩 7 次。
    //
    // 这是**纯测试便利设施**（编两个 fixture）被挂进了所有人的构建关键路径。
    // 门控后：普通 release 构建根本不声明这条依赖 → 环在非测试路径上断开；
    // 只有 `xtask test runtime` / CI 的 Rust 单测步骤显式带 feature 时才接上，
    // 那些路径本来就要跑 z42c，覆盖面不变。
    let want_fixtures = env::var_os("CARGO_FEATURE_Z42_TEST_FIXTURES").is_some();
    let project_root = manifest_dir.parent().and_then(Path::parent);
    if let (true, Some(root)) = (want_fixtures, project_root) {
        // z42c.driver is self-contained: its 6 z42c.* siblings are bundled into its
        // own dist by `z42c build` (simplify-compiler-build). So Z42_LIBS supplies
        // only the stdlib — the driver's siblings resolve from its own dir (z42vm's
        // entry-zpkg dir + libs search). Both produced by `xtask build stdlib`.
        let driver = root
            .join("artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg");
        let home = root.join("artifacts/intermediate/libraries/flat/release");
        let vm = find_z42vm(root);
        let ready = driver.is_file() && vm.as_ref().is_some_and(|v| v.is_file());
        println!("cargo:rerun-if-changed={}", driver.display());

        let c7_src = manifest_dir.join("tests/data/z42_native_e2e/source.z42");
        if c7_src.is_file() {
            println!("cargo:rerun-if-changed={}", c7_src.display());
            if ready {
                let out = PathBuf::from(&out_dir).join("z42_native_e2e_source.zbc");
                if z42c_emit_zbc(vm.as_ref().ok_or("fixture compiler unavailable")?, &driver, &home, &c7_src, &out) {
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
            if z42c_emit_zbc(vm.as_ref().ok_or("fixture compiler unavailable")?, &driver, &home, &emb_src, &out) {
                println!("cargo:rustc-cfg=z42_have_embedding_hello");
                println!("cargo:rustc-env=Z42_EMBEDDING_TEST_LIBS={}", home.display());
            }
        }
    }
    Ok(())
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
