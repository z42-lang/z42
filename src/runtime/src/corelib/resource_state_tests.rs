use crate::{pal::fs::default_vfs, vm::ext::VmExtensions, vm::context::VmContext};

fn assert_installed(ctx: &VmContext) {
    assert!(ctx.ext::<super::os::process::State>().is_some());
    assert!(ctx.ext::<super::threading::State>().is_some());
    assert!(ctx.ext::<super::io::fs::State>().is_some());
    #[cfg(feature = "net")]
    {
        assert!(ctx.ext::<super::network::tcp::State>().is_some());
        assert!(ctx.ext::<super::network::udp::State>().is_some());
    }
    #[cfg(feature = "tls")]
    assert!(ctx.ext::<super::tls::State>().is_some());
}

fn module() -> crate::vm::runtime::Module {
    crate::vm::runtime::Module::new(crate::zpkg::image::ModuleData {
        name: "resource-state".into(), string_pool: Vec::new(), classes: Vec::new(),
        func_index: Default::default(),
    }, Vec::new(), Default::default())
}

#[test]
fn existing_constructors_and_custom_extensions_install_resources() {
    assert_installed(&VmContext::new());
    assert_installed(&VmContext::with_vfs(default_vfs()));
    assert_installed(&VmContext::with_module(module()));
    assert_installed(&VmContext::with_module_and_vfs(module(), default_vfs()));
    let mut extensions = VmExtensions::new();
    extensions.register(73_u64).unwrap();
    let ctx = VmContext::with_extensions(Some(module()), default_vfs(), extensions);
    assert_installed(&ctx);
    assert_eq!(ctx.ext::<u64>(), Some(&73));
}

#[test]
fn resource_state_is_shared_by_workers_and_isolated_between_vms() {
    let first = VmContext::new();
    let second = VmContext::new();
    let worker = VmContext::new_with_core(first.core_arc());
    macro_rules! check {
        ($state:ty) => {{
            assert!(std::ptr::eq(first.ext::<$state>().unwrap(), worker.ext::<$state>().unwrap()));
            assert!(!std::ptr::eq(first.ext::<$state>().unwrap(), second.ext::<$state>().unwrap()));
        }};
    }
    check!(super::os::process::State);
    check!(super::threading::State);
    check!(super::io::fs::State);
    #[cfg(feature = "net")]
    { check!(super::network::tcp::State); check!(super::network::udp::State); }
    #[cfg(feature = "tls")]
    check!(super::tls::State);
    assert_installed(&worker);
}

#[test]
fn file_slots_keep_one_sequence_across_threads_and_close_from_a_worker() {
    use crate::object::Value;
    let path = std::env::temp_dir().join(format!("z42-resource-state-{}.bin", std::process::id()));
    std::fs::write(&path, b"abc").unwrap();
    let open = |ctx: &VmContext| {
        let value = crate::vm::exec::call::builtins::exec_builtin_value(ctx, "__file_open", &[
            Value::Str(path.to_str().unwrap().into()), Value::I64(0),
        ]).unwrap();
        let Value::I64(id) = value else { panic!("file open did not return an id") };
        id
    };
    let parent = VmContext::new();
    let other = VmContext::new();
    let first = open(&parent);
    let independent = open(&other);
    assert_eq!((first, independent), (1, 1));
    let core = parent.core_arc();
    let worker_path = path.clone();
    let next = std::thread::spawn(move || {
        let worker = VmContext::new_with_core(core);
        crate::vm::exec::call::builtins::exec_builtin(&worker, "__file_close", &[Value::I64(first)]).unwrap();
        let value = crate::vm::exec::call::builtins::exec_builtin_value(&worker, "__file_open", &[
            Value::Str(worker_path.to_str().unwrap().into()), Value::I64(0),
        ]).unwrap();
        let Value::I64(id) = value else { panic!("file open did not return an id") };
        id
    }).join().unwrap();
    assert_eq!(next, 2);
    assert!(crate::vm::exec::call::builtins::exec_builtin_value(&parent, "__file_length", &[Value::I64(first)]).is_err());
    assert_eq!(crate::vm::exec::call::builtins::exec_builtin_value(&parent, "__file_length", &[Value::I64(next)]).unwrap(), Value::I64(3));
    assert_eq!(crate::vm::exec::call::builtins::exec_builtin_value(&other, "__file_length", &[Value::I64(independent)]).unwrap(), Value::I64(3));
    for (ctx, slot) in [(&parent, next), (&other, independent)] {
        crate::vm::exec::call::builtins::exec_builtin(ctx, "__file_close", &[Value::I64(slot)]).unwrap();
        crate::vm::exec::call::builtins::exec_builtin(ctx, "__file_close", &[Value::I64(slot)]).unwrap();
    }
    std::fs::remove_file(path).unwrap();
}
