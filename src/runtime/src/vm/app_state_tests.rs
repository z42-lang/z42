use super::{program_args, set_program_args};
use crate::vm::VmContext;

#[test]
fn context_constructors_install_application_services_and_arguments() {
    fn module() -> crate::metadata::Module {
        crate::metadata::Module::new(crate::zpkg::image::ModuleData {
            name: "HostDefaults".into(), string_pool: vec![], classes: vec![],
            func_index: Default::default(),
        }, vec![], Default::default())
    }
    let constructors: [fn() -> std::pin::Pin<Box<VmContext>>; 4] = [
        VmContext::new,
        || VmContext::with_vfs(crate::pal::fs::default_vfs()),
        || VmContext::with_module(module()),
        || VmContext::with_extensions(None, crate::pal::fs::default_vfs(), Default::default()),
    ];
    for construct in constructors {
        let ctx = construct();
        assert!(crate::corelib::application::state(&ctx).is_ok());
        assert!(program_args(&ctx).unwrap().is_empty());
        set_program_args(&ctx, vec!["arg".into()]).unwrap();
        let worker = VmContext::new_with_core(ctx.core_arc());
        assert_eq!(program_args(&worker).unwrap(), ["arg"]);
    }
}

#[test]
fn application_arguments_share_with_workers_and_isolate_other_vms() {
    let parent = VmContext::new();
    let other = VmContext::new();
    assert!(program_args(&parent).unwrap().is_empty());
    set_program_args(&parent, vec!["first".into(), "second".into()]).unwrap();
    let snapshot = program_args(&parent).unwrap();
    let core = parent.core_arc();
    std::thread::spawn(move || {
        let worker = VmContext::new_with_core(core);
        assert_eq!(program_args(&worker).unwrap(), ["first", "second"]);
        set_program_args(&worker, vec!["replacement".into()]).unwrap();
    }).join().unwrap();
    assert_eq!(program_args(&parent).unwrap(), ["replacement"]);
    assert!(program_args(&other).unwrap().is_empty());
    assert_eq!(snapshot, ["first", "second"], "a returned snapshot is independent");
}
