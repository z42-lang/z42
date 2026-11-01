use crate::object::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

/// Extract a `Value::Array` of `Value::Str` into `Vec<String>`.
/// (Caller must bind the owning VmContext to a named local so the returned
/// array's ArrRef outlives this call — see fix-crypto-tests-ctx-lifetime.)
fn extract_strings(v: Value) -> Vec<String> {
    match v {
        Value::Array(rc) => rc
            .borrow()
            .iter_boxed()
            .map(|e| match e {
                Value::Str(s) => s.to_string(),
                other => panic!("expected Value::Str, got {other:?}"),
            })
            .collect(),
        other => panic!("expected Value::Array, got {other:?}"),
    }
}

fn builtin_env_args(c: &VmContext, args: &[Value]) -> anyhow::Result<Value> {
    crate::corelib::exec_builtin_value(c, "__env_args", args)
}

// add-z42-launcher : __env_args returns the program arguments
// passed after `--` on the z42vm command line (stored on VmCore via
// set_program_args), NOT the VM's own raw process argv.

#[test]
fn env_args_empty_by_default() {
    let c = ctx();
    let r = builtin_env_args(&c, &[]).unwrap();
    assert!(extract_strings(r).is_empty(),
        "no `--` args set → GetCommandLineArgs() must be empty");
}

#[test]
fn env_args_returns_set_program_args_in_order() {
    let c = ctx();
    c.set_program_args(vec!["alpha".into(), "beta".into(), "gamma".into()]);
    let r = builtin_env_args(&c, &[]).unwrap();
    assert_eq!(extract_strings(r), vec!["alpha", "beta", "gamma"]);
}

#[test]
fn env_args_reflects_last_set() {
    let c = ctx();
    c.set_program_args(vec!["x".into()]);
    c.set_program_args(vec!["y".into(), "z".into()]);
    let r = builtin_env_args(&c, &[]).unwrap();
    assert_eq!(extract_strings(r), vec!["y", "z"]);
}

// ── P1-8 PR 2: typed errors, parked I/O, no syscall under the handle-table lock ──

use crate::corelib::abi::BuiltinError;
use crate::corelib::exec_builtin_value;

fn str_v(s: &str) -> Value { Value::Str(s.into()) }

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("z42-fs-tests-{tag}-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The typed error a failed builtin call carries (`exec_builtin*` keep it downcastable).
fn fail(c: &VmContext, name: &str, args: &[Value]) -> BuiltinError {
    let e = exec_builtin_value(c, name, args).expect_err(name);
    match e.downcast::<BuiltinError>() {
        Ok(b) => b,
        Err(e) => panic!("{name}: not a typed error: {e}"),
    }
}

#[test]
fn missing_file_and_missing_directory_map_like_dotnet() {
    let c = ctx();
    let d = temp_dir("map");
    let missing = d.join("nope.txt");
    let p = missing.to_str().unwrap();
    for name in ["__file_read_text", "__file_read_bytes", "__file_get_size", "__file_delete", "__file_last_write_time_ms"] {
        let e = fail(&c, name, &[str_v(p)]);
        assert!(matches!(&e, BuiltinError::FileNotFound(m) if m == &format!("Could not find file '{p}'.")), "{name}: {e:?}");
        assert_eq!(e.classes(), ["Std.IO.FileNotFoundException", "Std.IO.IOException"]);
    }
    let in_missing = d.join("no-dir").join("f.txt");
    let q = in_missing.to_str().unwrap();
    assert!(matches!(fail(&c, "__file_read_text", &[str_v(q)]), BuiltinError::DirectoryNotFound(_)));
    assert!(matches!(fail(&c, "__file_write_text", &[str_v(q), str_v("x")]), BuiltinError::DirectoryNotFound(_)));
    assert!(matches!(fail(&c, "__file_write_bytes_atomic", &[str_v(q), c.heap().alloc_bytes(vec![1])]), BuiltinError::DirectoryNotFound(_)));
    assert!(matches!(fail(&c, "__dir_enumerate", &[str_v(d.join("gone").to_str().unwrap())]), BuiltinError::DirectoryNotFound(_)));
    assert!(matches!(fail(&c, "__file_open", &[str_v(p), Value::I64(0)]), BuiltinError::FileNotFound(_)));
    assert!(matches!(fail(&c, "__file_open", &[str_v(q), Value::I64(1)]), BuiltinError::DirectoryNotFound(_)));
    // copy: a missing source blames the source, a missing destination directory the destination
    let src = d.join("src.txt");
    std::fs::write(&src, b"hi").unwrap();
    assert!(matches!(fail(&c, "__file_copy", &[str_v(p), str_v(src.to_str().unwrap())]), BuiltinError::FileNotFound(_)));
    assert!(matches!(fail(&c, "__file_copy", &[str_v(src.to_str().unwrap()), str_v(q)]), BuiltinError::DirectoryNotFound(_)));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn directory_as_file_empty_path_and_null_map_like_dotnet() {
    let c = ctx();
    let d = temp_dir("dir");
    let dp = d.to_str().unwrap();
    assert!(matches!(fail(&c, "__file_read_text", &[str_v(dp)]), BuiltinError::UnauthorizedAccess(_)));
    assert!(matches!(fail(&c, "__file_open", &[str_v(dp), Value::I64(0)]), BuiltinError::UnauthorizedAccess(_)));
    assert!(matches!(fail(&c, "__file_get_size", &[str_v(dp)]), BuiltinError::FileNotFound(_)));
    std::fs::write(d.join("f"), b"x").unwrap();
    assert!(matches!(fail(&c, "__dir_delete", &[str_v(dp), Value::Bool(false)]), BuiltinError::Io(_)));
    assert!(matches!(fail(&c, "__dir_create", &[str_v(d.join("f").to_str().unwrap())]), BuiltinError::Io(_)));
    assert!(matches!(fail(&c, "__file_read_text", &[str_v("")]), BuiltinError::Argument(_)));
    assert!(matches!(fail(&c, "__file_read_text", &[Value::Null]), BuiltinError::ArgumentNull(_)));
    assert!(matches!(fail(&c, "__file_open", &[str_v(dp), Value::I64(9)]), BuiltinError::ArgumentOutOfRange(_)));
    assert_eq!(exec_builtin_value(&c, "__file_exists", &[str_v("")]).unwrap(), Value::Bool(false));
    assert_eq!(exec_builtin_value(&c, "__file_exists", &[Value::Null]).unwrap(), Value::Bool(false));
    assert_eq!(exec_builtin_value(&c, "__dir_exists", &[Value::Null]).unwrap(), Value::Bool(false));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn file_stream_round_trip_and_closed_handle() {
    let c = ctx();
    let d = temp_dir("stream");
    let p = d.join("s.bin");
    let ps = str_v(p.to_str().unwrap());
    let Value::I64(w) = exec_builtin_value(&c, "__file_open", &[ps, Value::I64(1)]).unwrap() else { panic!() };
    let data = c.heap().alloc_bytes((0..=255u8).collect());
    crate::corelib::exec_builtin(&c, "__file_write", &[Value::I64(w), data, Value::I64(10), Value::I64(100)]).unwrap();
    crate::corelib::exec_builtin(&c, "__file_close", &[Value::I64(w)]).unwrap();
    let closed = fail(&c, "__file_length", &[Value::I64(w)]);
    assert_eq!(closed.classes(), ["Std.ObjectDisposedException", "Std.InvalidOperationException"], "closed handle");
    let Value::I64(r) = exec_builtin_value(&c, "__file_open", &[ps, Value::I64(0)]).unwrap() else { panic!() };
    assert_eq!(exec_builtin_value(&c, "__file_length", &[Value::I64(r)]).unwrap(), Value::I64(100));
    let buf = c.heap().alloc_bytes(vec![0; 128]);
    let n = exec_builtin_value(&c, "__file_read", &[Value::I64(r), buf, Value::I64(28), Value::I64(100)]).unwrap();
    assert_eq!(n, Value::I64(100));
    let Value::Array(a) = buf else { panic!() };
    assert_eq!(a.borrow().byte_range(28, 3).unwrap(), vec![10, 11, 12]);
    assert_eq!(exec_builtin_value(&c, "__file_position", &[Value::I64(r)]).unwrap(), Value::I64(100));
    assert!(matches!(fail(&c, "__file_read", &[Value::I64(r), buf, Value::I64(100), Value::I64(100)]), BuiltinError::Argument(_)));
    assert!(matches!(fail(&c, "__file_read", &[Value::I64(r), buf, Value::I64(-1), Value::I64(1)]), BuiltinError::ArgumentOutOfRange(_)));
    crate::corelib::exec_builtin(&c, "__file_close", &[Value::I64(r)]).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

fn parked(c: &VmContext) -> usize {
    c.core.parked_count.load(std::sync::atomic::Ordering::Acquire) as usize
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < std::time::Duration::from_secs(5) {
        if cond() { return true; }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cond()
}

#[cfg(unix)]
fn mkfifo(path: &std::path::Path) {
    let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: plain libc call on a NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0, "mkfifo");
}

/// A thread blocked in a file read is parked: another thread can run a full collection
/// meanwhile, and the FileStream table is not held (another open / close goes through).
#[cfg(unix)]
#[test]
fn a_collection_completes_while_a_thread_is_blocked_in_a_file_read() {
    let c = ctx();
    let d = temp_dir("fifo");
    let fifo = d.join("p");
    mkfifo(&fifo);
    let fifo_s = fifo.to_str().unwrap().to_string();

    // Path read: `open` of a FIFO blocks until a writer appears.
    let core = std::sync::Arc::clone(&c.core);
    let path = fifo_s.clone();
    let reader = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        match exec_builtin_value(&w, "__file_read_text", &[Value::Str(path.as_str().into())]).unwrap() {
            Value::Str(s) => s.to_string(),
            other => panic!("{other:?}"),
        }
    });
    assert!(wait_until(|| parked(&c) == 1), "a thread blocked in File.ReadAllText must be parked");
    {
        let pause = crate::gc::safepoint::request_gc_pause(&c);
        assert!(pause.is_some(), "this thread becomes the collector");
        c.heap().collect_full();
    }
    std::fs::write(&fifo, b"through the fifo").unwrap();
    assert_eq!(reader.join().unwrap(), "through the fifo");
    assert_eq!(parked(&c), 0, "the park is released");

    // Handle read: blocked in `read` on an open FIFO, outside the handle-table lock.
    let mut writer_end = None;
    let core = std::sync::Arc::clone(&c.core);
    let path = fifo_s.clone();
    let opener = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        let Value::I64(slot) = exec_builtin_value(&w, "__file_open", &[Value::Str(path.as_str().into()), Value::I64(0)]).unwrap()
            else { panic!() };
        // A Rust local is no GC root (a z42 caller's register is): pin the buffer for the
        // collection the main thread runs while this read is blocked.
        let buf = w.heap().alloc_bytes(vec![0; 16]);
        let root = w.heap().pin_root(buf);
        let n = exec_builtin_value(&w, "__file_read", &[Value::I64(slot), buf, Value::I64(0), Value::I64(16)]).unwrap();
        w.heap().unpin_root(root);
        crate::corelib::exec_builtin(&w, "__file_close", &[Value::I64(slot)]).unwrap();
        n
    });
    assert!(wait_until(|| parked(&c) == 1), "the open is parked");
    writer_end.get_or_insert_with(|| std::fs::OpenOptions::new().write(true).open(&fifo).unwrap());
    // The reader now blocks in `read` (no data yet) — still parked.
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(wait_until(|| parked(&c) == 1), "the read is parked");
    {
        let pause = crate::gc::safepoint::request_gc_pause(&c);
        assert!(pause.is_some());
        c.heap().collect_full();
    }
    // The table lock is free: another file op does not queue behind the blocked read.
    let other = d.join("other.txt");
    std::fs::write(&other, b"x").unwrap();
    let Value::I64(s2) = exec_builtin_value(&c, "__file_open", &[str_v(other.to_str().unwrap()), Value::I64(0)]).unwrap()
        else { panic!() };
    assert_eq!(exec_builtin_value(&c, "__file_length", &[Value::I64(s2)]).unwrap(), Value::I64(1));
    crate::corelib::exec_builtin(&c, "__file_close", &[Value::I64(s2)]).unwrap();
    use std::io::Write;
    writer_end.as_mut().unwrap().write_all(b"abc").unwrap();
    assert_eq!(opener.join().unwrap(), Value::I64(3));
    assert_eq!(parked(&c), 0);
    let _ = std::fs::remove_dir_all(&d);
}

/// On a virtual file system a missing file is `FileNotFoundException` whatever its
/// "directory" (no real directories there — `Vfs::local_path` is `None`); on disk a
/// missing directory is `DirectoryNotFoundException` (the test above).
#[test]
fn memory_fs_missing_file_is_file_not_found() {
    let c = VmContext::with_vfs(std::sync::Arc::new(crate::pal::vfs::MemoryFs::new()));
    let e = fail(&c, "__file_read_text", &[str_v("/no-dir/f.txt")]);
    assert!(matches!(&e, BuiltinError::FileNotFound(m) if m == "Could not find file '/no-dir/f.txt'."), "{e:?}");
    crate::corelib::exec_builtin(&c, "__file_write_bytes", &[str_v("/d/a.bin"), c.heap().alloc_bytes(vec![1, 2])]).unwrap();
    crate::corelib::exec_builtin(&c, "__file_copy", &[str_v("/d/a.bin"), str_v("/d/b.bin")]).unwrap();
    let names = exec_builtin_value(&c, "__dir_enumerate_recursive", &[str_v("/d")]).unwrap();
    assert_eq!(extract_strings(names), vec!["a.bin", "b.bin"]);
    assert_eq!(exec_builtin_value(&c, "__file_get_size", &[str_v("/d/b.bin")]).unwrap(), Value::I64(2));
}
