use super::{program_args, set_program_args};
use crate::vm::context::VmContext;

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

use crate::assembly::context::VmContextFactory as _;
