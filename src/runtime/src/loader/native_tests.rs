use super::*;
use std::path::PathBuf;

#[test]
fn guess_register_symbol_strips_prefix_and_ext() {
    for (input, expected) in [
        ("libnumz42_c.dylib", "numz42_c_register"),
        ("libnumz42_c.so", "numz42_c_register"),
        ("numz42_c.dll", "numz42_c_register"),
        ("/abs/path/libfoo.dylib", "foo_register"),
    ] {
        assert_eq!(
            guess_register_symbol(&PathBuf::from(input)).as_deref(),
            Some(expected),
            "input = {input}"
        );
    }
}

#[test]
fn nonexistent_path_sets_lib_load_failure() {
    // retire-z-codes: no Z#### prefix in message, just the
    // diagnostic. The numeric code stays in Z42Error for embedder
    // wire-compat (LIB_LOAD_FAILURE = 910).
    let ctx = VmContext::new();
    let err = load_library(
        &ctx,
        Path::new("/nonexistent/lib_definitely_not_here.dylib"),
    )
    .expect_err("must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("dlopen"), "msg = {msg}");
    assert!(
        !msg.contains("Z0910"),
        "Z#### prefix must be retired; msg = {msg}"
    );
    assert_eq!(error::last().code, LIB_LOAD_FAILURE);
}

#[test]
fn host_registers_named_paths_and_workers_share_the_service() {
    let ctx = VmContext::new();
    assert!(path_source(&ctx).is_ok());
    let worker = VmContext::new_with_core(ctx.core_arc());
    assert!(std::ptr::eq(
        ctx.ext::<SearchPaths>().unwrap(),
        worker.ext::<SearchPaths>().unwrap()
    ));
}

fn fixture(broken: bool) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "z42-native-module-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let library = directory.join(crate::pal::dynlib::file_name("z42_fixture"));
    let source =
        Path::new(env!("Z42_RUNTIME_DIR")).join("tests/data/native_module/registration.rs");
    let mut command = std::process::Command::new("rustc");
    command
        .args(["--edition=2021", "--crate-type=cdylib", "-Awarnings"])
        .arg(source)
        .arg("-o")
        .arg(&library);
    if broken {
        command.args(["--cfg", "broken"]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    library
}

#[test]
fn actual_dynamic_module_registers_calls_isolates_vms_and_retains_code() {
    let library = fixture(false);
    let first = VmContext::new();
    let second = VmContext::new();
    load_native_module(&first, &library).unwrap();
    load_native_module(&first, &library).unwrap();
    load_native_module(&second, &library).unwrap();
    let call = |ctx: &VmContext| {
        crate::interop::invoke_export(ctx, Some("z42_fixture"), "__fixture_native_count", &[])
            .unwrap()
            .unwrap()
    };
    assert_eq!(call(&first), Some(crate::object::Value::I64(1)));
    let worker = VmContext::new_with_core(first.core_arc());
    assert_eq!(call(&worker), Some(crate::object::Value::I64(2)));
    assert_eq!(call(&second), Some(crate::object::Value::I64(1)));
    let pointer = resolve_symbol(&first, "z42_fixture", "z42_fixture_drop_count").unwrap();
    let drops: extern "C" fn() -> u64 = unsafe { std::mem::transmute(pointer) };
    assert_eq!(drops(), 0);
    drop(first);
    assert_eq!(drops(), 0);
    drop(worker);
    drop(second);
    assert_eq!(
        drops(),
        2,
        "module states drop once, code remains callable after VM teardown"
    );
    let _ = std::fs::remove_dir_all(library.parent().unwrap());
}

#[test]
fn dynamic_abi_failure_does_not_publish_and_good_retry_can_register() {
    let bad = fixture(true);
    let good = fixture(false);
    let ctx = VmContext::new();
    let error = load_native_module(&ctx, &bad).unwrap_err();
    assert!(error.to_string().contains("ABI mismatch"), "{error}");
    assert!(
        crate::interop::resolve_native_module(&ctx, "z42_fixture")
            .unwrap()
            .is_none()
    );
    load_native_module(&ctx, &good).unwrap();
    assert!(
        crate::interop::resolve_native_module(&ctx, "z42_fixture")
            .unwrap()
            .is_some()
    );
    drop(ctx);
    let _ = std::fs::remove_dir_all(bad.parent().unwrap());
    let _ = std::fs::remove_dir_all(good.parent().unwrap());
}
