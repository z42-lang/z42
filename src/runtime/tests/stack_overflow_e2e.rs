//! Stack overflow is a fatal VM error (runtime-audit P0-4): z42vm prints a fatal
//! report with the z42 call stack and exits with `stack_guard::EXIT_CODE`; z42
//! `catch` / `finally` do not intercept it. Runs z42vm in a child process on the
//! `embedding_hello` fixture's `DeepMain` (unbounded recursion).

#![cfg(z42_have_embedding_hello)]

use std::path::PathBuf;
use std::process::Command;

fn libs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/build/libraries/dist/release")
}

fn run_deep(mode: &str) -> (Option<i32>, String, String) {
    let zbc = PathBuf::from(env!("OUT_DIR")).join("embedding_hello.zbc");
    let out = Command::new(env!("CARGO_BIN_EXE_z42vm"))
        .args(["--mode", mode])
        .arg(&zbc)
        .arg("Embedding.Hello.DeepMain")
        .env("Z42_LIBS", libs_dir())
        .output()
        .expect("spawn z42vm");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_fatal(mode: &str) {
    if !libs_dir().join("z42.core.zpkg").is_file() {
        eprintln!("skipping: stdlib not built");
        return;
    }
    let (code, stdout, stderr) = run_deep(mode);
    assert_eq!(code, Some(z42::stack_guard::EXIT_CODE), "{mode}: stdout:\n{stdout}\nstderr:\n{stderr}");
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
