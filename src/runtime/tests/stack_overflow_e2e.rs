//! Stack overflow is a fatal VM error (runtime-audit P0-4): z42vm prints a fatal
//! report with the z42 call stack and exits with `vm::exec::stack_guard::EXIT_CODE`; z42
//! `catch` / `finally` do not intercept it. Runs z42vm in a child process on the
//! `embedding_hello` fixture's `DeepMain` (unbounded recursion).

#![cfg(z42_have_embedding_hello)]

use std::path::Path;
use std::process::Command;

fn assert_fatal(mode: &str) {
    let libs = Path::new(env!("Z42_EMBEDDING_TEST_LIBS"));
    let corelib = libs.join("z42.core.zpkg");
    assert!(
        corelib.is_file(),
        "embedding fixture needs {}; run `xtask build stdlib`",
        corelib.display()
    );
    let zbc = Path::new(env!("OUT_DIR")).join("embedding_hello.zbc");
    let out = Command::new(env!("CARGO_BIN_EXE_z42vm"))
        .args(["--mode", mode])
        .arg(&zbc)
        .arg("Embedding.Hello.DeepMain")
        .env("Z42_LIBS", libs)
        .output()
        .expect("spawn z42vm");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(z42::vm::exec::stack_guard::EXIT_CODE),
        "{mode}: stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.contains("fatal error: stack overflow"), "{mode}: stderr:\n{stderr}");
    assert!(stderr.contains("Embedding.Hello.Down"), "{mode}: no z42 frames in:\n{stderr}");
    assert!(stderr.contains("Embedding.Hello.DeepMain"), "{mode}: entry frame missing in:\n{stderr}");
    assert!(!stdout.contains("caught") && !stdout.contains("finally"),
        "{mode}: a z42 handler ran during a fatal error:\n{stdout}");
}

#[test]
fn interp_stack_overflow_is_fatal() {
    assert_fatal("interp");
}

#[cfg(feature = "jit")]
#[test]
fn jit_stack_overflow_is_fatal() {
    assert_fatal("jit");
}
