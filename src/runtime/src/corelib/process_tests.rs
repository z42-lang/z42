//! `Std.IO.Process` Rust-side unit tests.
//!
//! Tests exercise the builtin directly with `Value`-encoded args, so they
//! verify marshalling + `std::process::Command` plumbing without going
//! through z42 facade or the IR dispatcher.
//!
//! Platform skip: tests assume POSIX coreutils (`echo` / `printf` / `cat`
//! / `pwd` / `env` / `false` / `true`). Git Bash on windows-latest CI
//! provides most of these — 489/492 pass — but a few cases fall over on
//! genuine cross-platform gaps and are gated `#[cfg(unix)]`:
//!   - `run_argv_array_passes_args_literally`  (Git Bash `printf` handles
//!     `%s\n` differently — emits no trailing newline)
//!   - `run_working_directory_takes_effect`    (hardcoded `/tmp`)
//!   - `run_timeout_fires_for_long_running_child`
//!     (Real Windows process-tree kill defect: `child.kill()` on the
//!     `sh.exe` parent does NOT propagate to the grandchild `sleep 5`;
//!     the inherited pipe handle keeps stdout-reader thread blocked.
//!     TODO: switch to `taskkill /T /F` or job-object-based kill on
//!     Windows so timeouts terminate the whole tree.)

use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

/// Serialize tests that read or mutate the process-global `PATH` env var.
/// Without this, parallel tests like `which_finds_in_custom_path` (sets
/// PATH to a temp dir) race with tests like `run_working_directory_takes_effect`
/// (spawns `pwd` — PATH lookup) and the latter intermittently fails because
/// `pwd` can't be found in the temp PATH.
static PATH_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn s(v: &str) -> Value { Value::Str(v.into()) }
fn i(n: i64) -> Value  { Value::I64(n) }
fn b(v: bool) -> Value { Value::Bool(v) }
fn nul() -> Value      { Value::Null }

fn empty_str_arr(ctx: &VmContext) -> Value { ctx.heap().alloc_array(vec![]) }
fn str_arr(ctx: &VmContext, xs: &[&str]) -> Value {
    ctx.heap().alloc_array(xs.iter().map(|x| s(x)).collect())
}

/// Build the 14-arg run tuple with sensible defaults (Run shape: stdin
/// Null, stdout/stderr Pipe). Pass `extra` closures to override.
fn run_args(ctx: &VmContext, program: &str, argv: &[&str]) -> Vec<Value> {
    vec![
        s(program),                 // 0  program
        str_arr(ctx, argv),         // 1  args
        empty_str_arr(ctx),         // 2  env_keys
        empty_str_arr(ctx),         // 3  env_vals
        empty_str_arr(ctx),         // 4  env_remove
        b(false),                   // 5  env_clear
        nul(),                      // 6  cwd
        i(STDIO_NULL),              // 7  stdin_mode
        nul(),                      // 8  stdin_bytes
        i(STDIO_PIPE),              // 9  stdout_mode
        nul(),                      // 10 stdout_path
        i(STDIO_PIPE),              // 11 stderr_mode
        nul(),                      // 12 stderr_path
        i(-1),                      // 13 timeout_ms
    ]
}

/// Helper: read the result Array discriminator + element by index.
fn result_kind(v: &Value) -> i64 {
    let Value::Array(rc) = v else { panic!("expected Array, got {v:?}") };
    let borrowed = rc.borrow();
    let Value::I64(k) = borrowed.get_boxed(0) else { panic!("kind not I64") };
    k
}

fn result_at(v: &Value, idx: usize) -> Value {
    let Value::Array(rc) = v else { panic!("expected Array") };
    rc.borrow().get_boxed(idx).clone()
}

/// Spawn args: 13-element shape (no timeout).
fn spawn_args(ctx: &VmContext, program: &str, argv: &[&str]) -> Vec<Value> {
    let mut a = run_args(ctx, program, argv);
    a.pop();  // drop timeout
    a
}

// ── ok path ──────────────────────────────────────────────────────────────

#[test]
fn run_echo_captures_stdout_and_exit_zero() {
    let ctx = VmContext::new();
    let args = run_args(&ctx, "echo", &["hello"]);
    let r = builtin_process_run(&ctx, &args).unwrap();

    assert_eq!(result_kind(&r), KIND_OK);
    assert_eq!(result_at(&r, 1), i(0));                       // ExitCode
    let Value::Str(out) = result_at(&r, 2) else { panic!() }; // Stdout
    assert_eq!(out, "hello\n".into());
    assert_eq!(result_at(&r, 3), s(""));                      // Stderr empty
}

#[cfg(unix)]
#[test]
fn run_argv_array_passes_args_literally() {
    // Multi-word argv element must reach the child as ONE arg, not
    // shell-split into two.
    let ctx = VmContext::new();
    let args = run_args(&ctx, "printf", &["%s\n", "a b c"]);
    let r = builtin_process_run(&ctx, &args).unwrap();
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert_eq!(out, "a b c\n".into());
}

#[test]
fn run_nonzero_exit_reaches_caller() {
    let ctx = VmContext::new();
    let args = run_args(&ctx, "false", &[]);
    let r = builtin_process_run(&ctx, &args).unwrap();
    assert_eq!(result_kind(&r), KIND_OK);
    let Value::I64(code) = result_at(&r, 1) else { panic!() };
    assert_ne!(code, 0);
}

// ── start failure path ──────────────────────────────────────────────────

#[test]
fn run_nonexistent_program_returns_start_err() {
    let ctx = VmContext::new();
    let args = run_args(&ctx, "definitely-not-a-real-binary-xyzzy-42", &[]);
    let r = builtin_process_run(&ctx, &args).unwrap();
    assert_eq!(result_kind(&r), KIND_START_ERR);
    let Value::Str(prog) = result_at(&r, 1) else { panic!() };
    assert_eq!(prog, "definitely-not-a-real-binary-xyzzy-42".into());
    let Value::Str(msg) = result_at(&r, 2) else { panic!() };
    assert!(msg.contains("NotFound") || msg.to_lowercase().contains("no such"));
}

// ── env ─────────────────────────────────────────────────────────────────

#[test]
fn run_env_override_visible_to_child() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "sh", &["-c", "echo $Z42_TEST_VAR"]);
    args[2] = str_arr(&ctx, &["Z42_TEST_VAR"]);
    args[3] = str_arr(&ctx, &["hello-from-test"]);
    let r = builtin_process_run(&ctx, &args).unwrap();
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert_eq!(out, "hello-from-test\n".into());
}

#[test]
fn run_env_clear_strips_parent_env() {
    let ctx = VmContext::new();
    // Set a known env var on the parent so we can prove it's absent.
    std::env::set_var("Z42_CLEAR_TEST", "parent-visible");
    let mut args = run_args(&ctx, "sh", &["-c", "echo ${Z42_CLEAR_TEST:-empty}"]);
    args[5] = b(true); // env_clear
    let r = builtin_process_run(&ctx, &args).unwrap();
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert_eq!(out, "empty\n".into());
}

// ── cwd ─────────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn run_working_directory_takes_effect() {
    let _path_guard = PATH_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "pwd", &[]);
    args[6] = s("/tmp");
    let r = builtin_process_run(&ctx, &args).unwrap();
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    // macOS resolves /tmp to /private/tmp via symlink; either is fine.
    let trimmed = out.trim();
    assert!(trimmed == "/tmp" || trimmed == "/private/tmp", "got {trimmed:?}");
}

// ── stdin bytes ─────────────────────────────────────────────────────────

#[test]
fn run_stdin_bytes_feeds_child() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "cat", &[]);
    args[7] = i(STDIO_PIPE);
    let bytes = ctx.heap().alloc_array(
        b"hello\n".iter().map(|x| i(*x as i64)).collect()
    );
    args[8] = bytes;
    let r = builtin_process_run(&ctx, &args).unwrap();
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert_eq!(out, "hello\n".into());
}

// ── stdio modes ─────────────────────────────────────────────────────────

#[test]
fn run_stdout_null_drops_output() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "echo", &["should-be-discarded"]);
    args[9] = i(STDIO_NULL); // stdout_mode
    let r = builtin_process_run(&ctx, &args).unwrap();
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert_eq!(out, "".into());
}

#[test]
fn run_stdout_to_file() {
    let ctx = VmContext::new();
    let tmp_dir = std::env::temp_dir();
    let path = tmp_dir.join("z42-process-stdout-test.log");
    let _ = std::fs::remove_file(&path);

    let mut args = run_args(&ctx, "echo", &["redirected"]);
    args[9]  = i(STDIO_FILE);
    args[10] = s(path.to_str().unwrap());

    let r = builtin_process_run(&ctx, &args).unwrap();
    assert_eq!(result_kind(&r), KIND_OK);
    let Value::Str(captured) = result_at(&r, 2) else { panic!() };
    assert_eq!(captured, "".into()); // not captured — redirected

    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, "redirected\n");
    let _ = std::fs::remove_file(&path);
}

// ── bytes / lossy decode ────────────────────────────────────────────────

#[test]
fn run_invalid_utf8_in_stdout_becomes_replacement_char() {
    // `printf '\xff'` is non-portable (BSD vs GNU printf differ on
    // hex escapes), so pre-stage the invalid bytes in a temp file and
    // have `cat` emit them — every POSIX `cat` is byte-transparent.
    let ctx = VmContext::new();
    let tmp = std::env::temp_dir().join("z42-process-bad-utf8.bin");
    std::fs::write(&tmp, [0xff_u8, 0xfe_u8]).unwrap();

    let args = run_args(&ctx, "cat", &[tmp.to_str().unwrap()]);
    let r = builtin_process_run(&ctx, &args).unwrap();

    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert!(out.contains('\u{FFFD}'),
        "expected replacement char, got bytes {:?}", out.as_bytes());

    // Raw bytes round-trip unchanged via StdoutBytes path.
    let Value::Array(rc) = result_at(&r, 4) else { panic!() };
    let bytes = rc.borrow();
    assert_eq!(bytes.len(), 2);
    assert_eq!(bytes.get_boxed(0), i(0xff));
    assert_eq!(bytes.get_boxed(1), i(0xfe));

    let _ = std::fs::remove_file(&tmp);
}

// ── Phase 3: spawn + handle ops ─────────────────────────────────────────

fn slot_id_from(v: &Value) -> u64 {
    assert_eq!(result_kind(v), KIND_OK);
    let Value::I64(id) = result_at(v, 1) else { panic!("slot id not I64") };
    id as u64
}

#[test]
fn spawn_then_wait_returns_ok_result() {
    let ctx = VmContext::new();
    let args = spawn_args(&ctx, "echo", &["spawned"]);
    let spawn_r = builtin_process_spawn(&ctx, &args).unwrap();
    let slot = slot_id_from(&spawn_r);

    assert_eq!(ctx.process_slot_count(), 1);

    let wait_r = builtin_process_handle_wait(&ctx, &[i(slot as i64)]).unwrap();
    assert_eq!(result_kind(&wait_r), KIND_OK);
    assert_eq!(result_at(&wait_r, 1), i(0));
    let Value::Str(out) = result_at(&wait_r, 2) else { panic!() };
    assert_eq!(out, "spawned\n".into());

    // Slot consumed after wait.
    assert_eq!(ctx.process_slot_count(), 0);
}

#[test]
fn spawn_nonexistent_program_returns_start_err() {
    let ctx = VmContext::new();
    let args = spawn_args(&ctx, "definitely-not-a-real-binary-xyzzy-99", &[]);
    let r = builtin_process_spawn(&ctx, &args).unwrap();
    assert_eq!(result_kind(&r), KIND_START_ERR);
    assert_eq!(ctx.process_slot_count(), 0);
}

#[test]
fn try_wait_returns_null_while_running_then_result_after() {
    let ctx = VmContext::new();
    // Long enough that even a slow CI's `sh` startup + first try_wait still
    // observes "running" before the child's `sleep` returns. Was 0.15s →
    // intermittent on Windows/macOS CI where sh + spawn overhead could
    // push first try_wait past the child's exit window.
    let args = spawn_args(&ctx, "sh", &["-c", "sleep 1; echo done"]);
    let spawn_r = builtin_process_spawn(&ctx, &args).unwrap();
    let slot = slot_id_from(&spawn_r);

    // First poll: still running.
    let first = builtin_process_handle_try_wait(&ctx, &[i(slot as i64)]).unwrap();
    assert!(matches!(first, Value::Null), "expected Null, got {first:?}");
    assert_eq!(ctx.process_slot_count(), 1);

    // Poll-loop until child reaps (avoid a fixed sleep that could time out
    // on heavily-loaded CI). Capped at 5 s.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let second = loop {
        let r = builtin_process_handle_try_wait(&ctx, &[i(slot as i64)]).unwrap();
        if !matches!(r, Value::Null) { break r; }
        if std::time::Instant::now() >= deadline {
            panic!("try_wait did not observe child exit within 5 s");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(result_kind(&second), KIND_OK);
    let Value::Str(out) = result_at(&second, 2) else { panic!() };
    assert_eq!(out, "done\n".into());
    assert_eq!(ctx.process_slot_count(), 0);
}

#[test]
fn kill_terminates_long_running_child() {
    let ctx = VmContext::new();
    let args = spawn_args(&ctx, "sh", &["-c", "sleep 30"]);
    let slot = slot_id_from(&builtin_process_spawn(&ctx, &args).unwrap());

    let r = builtin_process_handle_kill(&ctx, &[i(slot as i64), b(false)]).unwrap();
    assert!(matches!(r, Value::Null));

    // After kill we can still wait — child should reap quickly.
    let wait_r = builtin_process_handle_wait(&ctx, &[i(slot as i64)]).unwrap();
    assert_eq!(result_kind(&wait_r), KIND_OK);
    let Value::I64(code) = result_at(&wait_r, 1) else { panic!() };
    assert_ne!(code, 0); // killed → non-zero (128+SIGKILL=137 on unix)
}

#[test]
fn write_stdin_then_close_then_wait() {
    let ctx = VmContext::new();
    let mut args = spawn_args(&ctx, "cat", &[]);
    args[7] = i(STDIO_PIPE);  // stdin = Pipe
    let slot = slot_id_from(&builtin_process_spawn(&ctx, &args).unwrap());

    let payload = ctx.heap().alloc_array(b"hi\n".iter().map(|x| i(*x as i64)).collect());
    let r = builtin_process_handle_write_stdin(&ctx, &[i(slot as i64), payload]).unwrap();
    assert!(matches!(r, Value::Null));

    let r = builtin_process_handle_close_stdin(&ctx, &[i(slot as i64)]).unwrap();
    assert!(matches!(r, Value::Null));

    let wait_r = builtin_process_handle_wait(&ctx, &[i(slot as i64)]).unwrap();
    let Value::Str(out) = result_at(&wait_r, 2) else { panic!() };
    assert_eq!(out, "hi\n".into());
}

#[test]
fn pid_returns_positive_int() {
    let ctx = VmContext::new();
    let args = spawn_args(&ctx, "sh", &["-c", "sleep 0.1"]);
    let slot = slot_id_from(&builtin_process_spawn(&ctx, &args).unwrap());

    let pid_v = builtin_process_handle_pid(&ctx, &[i(slot as i64)]).unwrap();
    let Value::I64(pid) = pid_v else { panic!("pid not I64") };
    assert!(pid > 0);

    let _ = builtin_process_handle_wait(&ctx, &[i(slot as i64)]).unwrap();
}

#[test]
fn handle_invalid_after_drop() {
    let ctx = VmContext::new();
    let args = spawn_args(&ctx, "sh", &["-c", "sleep 30"]);
    let slot = slot_id_from(&builtin_process_spawn(&ctx, &args).unwrap());

    // Drop reaps the child and frees the slot.
    let _ = builtin_process_handle_drop(&ctx, &[i(slot as i64)]).unwrap();
    assert_eq!(ctx.process_slot_count(), 0);

    // Subsequent ops on the freed slot id are KIND_HANDLE_INVALID.
    let r = builtin_process_handle_wait(&ctx, &[i(slot as i64)]).unwrap();
    let Value::Array(rc) = r else { panic!() };
    let Value::I64(kind) = rc.borrow().get_boxed(0) else { panic!() };
    assert_eq!(kind, 3 /* KIND_HANDLE_INVALID */);
}

// ── Phase 4: timeout ────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn run_timeout_fires_for_long_running_child() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "sh", &["-c", "sleep 5"]);
    args[13] = i(150); // 150ms timeout
    let start = std::time::Instant::now();
    let r = builtin_process_run(&ctx, &args).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(result_kind(&r), KIND_TIMEOUT);
    let Value::Str(prog) = result_at(&r, 1) else { panic!() };
    assert_eq!(prog, "sh".into());
    let Value::I64(ms) = result_at(&r, 2) else { panic!() };
    assert_eq!(ms, 150);
    // Should return well before the 5-second sleep would have finished.
    assert!(elapsed.as_secs() < 2, "elapsed {:?} should be << 5s", elapsed);
}

#[test]
fn run_timeout_does_not_fire_if_child_exits_quickly() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "echo", &["fast"]);
    args[13] = i(5000); // 5s timeout
    let r = builtin_process_run(&ctx, &args).unwrap();
    assert_eq!(result_kind(&r), KIND_OK);
    let Value::Str(out) = result_at(&r, 2) else { panic!() };
    assert_eq!(out, "fast\n".into());
}

#[test]
fn slot_ids_are_monotonic_unique() {
    let ctx = VmContext::new();
    let a1 = spawn_args(&ctx, "true", &[]);
    let a2 = spawn_args(&ctx, "true", &[]);
    let s1 = slot_id_from(&builtin_process_spawn(&ctx, &a1).unwrap());
    let s2 = slot_id_from(&builtin_process_spawn(&ctx, &a2).unwrap());
    assert_ne!(s1, s2);
    assert!(s2 > s1, "{s2} should be > {s1}");

    let _ = builtin_process_handle_wait(&ctx, &[i(s1 as i64)]);
    let _ = builtin_process_handle_wait(&ctx, &[i(s2 as i64)]);
}

// ── __process_which (add-process-which 2026-05-26) ───────────────────────

#[test]
fn which_returns_null_for_empty_name() {
    let ctx = VmContext::new();
    let r = builtin_process_which(&ctx, &[s("")]).unwrap();
    assert!(matches!(r, Value::Null), "empty name → null, got {r:?}");
}

#[test]
fn which_returns_null_for_nonexistent_command() {
    let ctx = VmContext::new();
    let r = builtin_process_which(&ctx, &[s("__z42_definitely_no_such_cmd__")]).unwrap();
    assert!(matches!(r, Value::Null), "missing command → null, got {r:?}");
}

#[cfg(unix)]
#[test]
fn which_finds_in_custom_path() {
    use std::os::unix::fs::PermissionsExt;
    let _path_guard = PATH_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = VmContext::new();
    let tmp = tempdir_unique("z42-which-test");
    std::fs::create_dir_all(&tmp).unwrap();
    let stub = tmp.join("zwhich_stub");
    std::fs::write(&stub, "#!/bin/sh\necho hi\n").unwrap();
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let prev = std::env::var("PATH").ok();
    std::env::set_var("PATH", &tmp);
    let r = builtin_process_which(&ctx, &[s("zwhich_stub")]).unwrap();
    if let Some(p) = prev { std::env::set_var("PATH", p); } else { std::env::remove_var("PATH"); }

    let Value::Str(found) = r else { panic!("expected Str, got {r:?}") };
    assert_eq!(found, stub.to_string_lossy().into());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(unix)]
#[test]
fn which_skips_non_executable_files() {
    let _path_guard = PATH_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = VmContext::new();
    let tmp = tempdir_unique("z42-which-noexec");
    std::fs::create_dir_all(&tmp).unwrap();
    let plain = tmp.join("notexec");
    std::fs::write(&plain, "data").unwrap();
    // No chmod +x.

    let prev = std::env::var("PATH").ok();
    std::env::set_var("PATH", &tmp);
    let r = builtin_process_which(&ctx, &[s("notexec")]).unwrap();
    if let Some(p) = prev { std::env::set_var("PATH", p); } else { std::env::remove_var("PATH"); }

    assert!(matches!(r, Value::Null), "non-executable should not resolve, got {r:?}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(unix)]
#[test]
fn which_passthrough_for_path_with_separator() {
    let ctx = VmContext::new();
    // /bin/sh is POSIX-mandated and executable.
    let r = builtin_process_which(&ctx, &[s("/bin/sh")]).unwrap();
    let Value::Str(found) = r else { panic!("expected Str, got {r:?}") };
    assert_eq!(found, "/bin/sh".into());
}

#[cfg(unix)]
#[test]
fn which_passthrough_returns_null_when_path_missing() {
    let ctx = VmContext::new();
    let r = builtin_process_which(&ctx, &[s("/nonexistent/dir/no_such_bin")]).unwrap();
    assert!(matches!(r, Value::Null), "missing absolute path → null, got {r:?}");
}

#[cfg(unix)]
fn tempdir_unique(prefix: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("{}.{}.{}", prefix, std::process::id(), nanos))
}

// ── process group (arg 14: own_process_group) — fix-repl-launcher-process-group ──

/// Run a child that prints its own process-group id, returning that pgid. `$$` is
/// the child sh's pid; `ps -o pgid=` prints its pgid header-less.
#[cfg(unix)]
fn child_pgid(ctx: &VmContext, args: &[Value]) -> i32 {
    let r = builtin_process_run(ctx, args).unwrap();
    assert_eq!(result_kind(&r), KIND_OK);
    let Value::Str(out) = result_at(&r, 2) else { panic!("stdout not Str") };
    out.trim().parse().expect("child pgid")
}

#[cfg(unix)]
#[test]
fn run_own_process_group_puts_child_in_fresh_group() {
    // own_process_group=true (arg 14) → child leads its OWN group; its pgid differs
    // from ours (so a run-timeout can tree-kill it).
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "sh", &["-c", "ps -o pgid= -p $$"]);
    args.push(b(true));   // 14 own_process_group
    let our_pgid = unsafe { libc::getpgrp() };
    assert_ne!(child_pgid(&ctx, &args), our_pgid, "own-group child must not share our pgid");
}

#[cfg(unix)]
#[test]
fn run_share_process_group_keeps_child_in_caller_group() {
    // own_process_group=false (Process.ShareProcessGroup) → child STAYS in our group,
    // so it can join the caller's terminal job-control (interactive `z42 repl`). Its
    // pgid equals ours.
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "sh", &["-c", "ps -o pgid= -p $$"]);
    args.push(b(false));  // 14 own_process_group
    let our_pgid = unsafe { libc::getpgrp() };
    assert_eq!(child_pgid(&ctx, &args), our_pgid, "shared-group child must share our pgid");
}

#[cfg(unix)]
#[test]
fn run_absent_process_group_arg_defaults_to_own_group() {
    // A 14-arg call (no arg 14) behaves as own_process_group=true — the defensive read
    // keeps old zpkgs working on a new VM and vice-versa (no arity / two-nightly coupling).
    let ctx = VmContext::new();
    let args = run_args(&ctx, "sh", &["-c", "ps -o pgid= -p $$"]);  // 14 args, no flag
    let our_pgid = unsafe { libc::getpgrp() };
    assert_ne!(child_pgid(&ctx, &args), our_pgid);
}
