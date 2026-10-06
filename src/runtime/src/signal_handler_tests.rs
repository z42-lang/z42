//! Unit test for `signal_handler` — install idempotency. The `sigsafe` write
//! primitives + signal-name table moved to `pal::signal` (Phase 3) and are
//! tested in `pal/signal_tests.rs`; the actual signal-firing path is covered by
//! the `tests/signal_handler_e2e.rs` integration tests.

#[test]
fn install_is_idempotent() {
    // Call install() twice — second call must not panic, must not double-
    // register handlers. signal-hook-registry queues all handlers and runs
    // them in order — duplicate registration just means our handler runs
    // twice, harmless but wasteful. Reaching the end = no panic = pass.
    super::install();
    super::install();
}

/// Capture what `f` writes to an fd (a temp file: a walk over every live
/// VmCore of the test process can exceed a pipe buffer).
fn capture(f: impl FnOnce(i32)) -> String {
    use std::os::fd::AsRawFd;
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "z42-sigwalk-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let file = std::fs::File::create(&path).unwrap();
    f(file.as_raw_fd());
    drop(file);
    let out = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    out
}

/// `write_call_stacks` with a retry: other tests create VmCores concurrently,
/// so the handler's `try_lock` on the registry can lose.
fn walk() -> String {
    for _ in 0..200 {
        let out = capture(super::write_call_stacks);
        if !out.contains("contended") {
            return out;
        }
        std::thread::yield_now();
    }
    panic!("VM registry stayed contended");
}

#[test]
fn crashing_thread_header_precedes_its_frames() {
    let out = capture(|fd| super::write_thread_header(fd, 0, 3, true));
    assert_eq!(out, "  thread #0 (3 frame(s))\n");
}

#[test]
fn other_threads_print_only_their_frame_count() {
    let out = capture(|fd| super::write_thread_header(fd, 2, 5, false));
    assert_eq!(out, "  thread #2 (5 frame(s), other thread: frames not read)\n");
    let empty = capture(|fd| super::write_thread_header(fd, 1, 0, false));
    assert_eq!(empty, "  thread #1 (0 frame(s))\n");
}

#[test]
fn walk_prints_frames_only_for_the_calling_thread() {
    use crate::exception::VmFrame;
    let mut main = crate::exception::tests::test_function("Main", &[], Vec::new());
    main.frame_meta = Some(("Main".into(), "t.z42".into()));
    let ctx = crate::vm_context::VmContext::new();
    ctx.push_frame(VmFrame::new(&main, std::ptr::null(), std::ptr::null()));
    let mine = walk();
    assert!(mine.contains("#0  Main at t.z42:0:0"), "own frames printed; got:\n{mine}");
    let ctx_ref: &crate::vm_context::VmContext = &ctx;
    let theirs = std::thread::scope(|sc| {
        sc.spawn(|| {
            let _ = ctx_ref; // the context stays registered while this thread walks
            walk()
        })
        .join()
        .unwrap()
    });
    assert!(theirs.contains("(1 frame(s), other thread: frames not read)"), "got:\n{theirs}");
    assert!(!theirs.contains("Main at"), "another thread's frames are not read; got:\n{theirs}");
    ctx.pop_frame();
}
