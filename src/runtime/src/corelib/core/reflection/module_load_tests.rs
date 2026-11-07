use super::*;
use crate::corelib::application::Services;
use crate::pal::{fs::Vfs, vfs::MemoryFs};
use std::path::PathBuf;

#[test]
fn application_service_preserves_capture_arguments_error_and_outer_sink() {
    fn run(path: &str, entry: Option<&str>, libs: Option<PathBuf>, vfs: &Arc<dyn Vfs>) -> anyhow::Result<()> {
        assert_eq!(path, "/case.zbc");
        assert_eq!(entry, Some("Case.Main"));
        assert_eq!(libs, Some(PathBuf::from("/libs")));
        let ctx = VmContext::with_vfs(vfs.clone());
        assert_eq!(ctx.vfs().read("/marker").unwrap(), b"caller-vfs");
        crate::vm::exec::call::builtins::exec_builtin(&ctx, "__print", &[Value::Str("nested output".into())]).unwrap();
        anyhow::bail!("deliberate nested error")
    }
    let fs = Arc::new(MemoryFs::new());
    fs.mount("/marker", b"caller-vfs".to_vec());
    let vfs: Arc<dyn Vfs> = fs;
    let services = Services { run_isolated: run, report_on_exit: |_| {} };
    crate::corelib::io::push_stdout_sink();
    let captured = run_one_golden_isolated(services, "/case.zbc", Some("Case.Main"), Some("/libs".into()), &vfs);
    let outer = crate::corelib::io::take_stdout_sink();
    assert_eq!(captured, "nested output\n<z42-run-error: deliberate nested error>");
    assert!(outer.is_empty(), "nested output must not escape to its outer sink");
}

#[test]
fn application_service_default_vfs_failures_match_in_sequential_and_parallel_runs() {
    let fs = Arc::new(MemoryFs::new());
    fs.mount("/first.zbc", b"invalid-first".to_vec());
    fs.mount("/second.zbc", b"invalid-second".to_vec());
    let ctx = VmContext::with_vfs(fs);
    let paths = ctx.heap().alloc_array(vec![Value::Str("/first.zbc".into()), Value::Str("/second.zbc".into())]);
    let entries = ctx.heap().alloc_array(vec![Value::Str("".into()), Value::Str("Other.Main".into())]);
    let mut results = Vec::new();
    for jobs in [1, 2] {
        crate::corelib::io::push_stdout_sink();
        let out = run_goldens_isolated(&ctx, &paths, &entries, &"".into(), jobs).unwrap();
        assert!(crate::corelib::io::take_stdout_sink().is_empty());
        let Value::Array(array) = out else { panic!("expected string[]") };
        let rows: Vec<String> = array.borrow().iter_boxed().map(|v| {
            let Value::Str(s) = v else { panic!("expected string") };
            s.to_string()
        }).collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("<z42-run-error:") && rows[0].contains("/first.zbc"), "{:?}", rows);
        assert!(rows[1].contains("<z42-run-error:") && rows[1].contains("/second.zbc"), "{:?}", rows);
        results.push(rows);
    }
    assert_eq!(results[0], results[1], "parallel workers retain input order and error formatting");
}
