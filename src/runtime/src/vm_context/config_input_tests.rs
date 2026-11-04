use super::VmContext;
use crate::config::RuntimeConfig;
use crate::gc::GcMode;

#[test]
fn captured_vm_inputs_choose_independent_heap_modes() {
    let mut settings = RuntimeConfig::from_getter(|_| None);
    settings.gc_mode = GcMode::StwMarkSweep;
    let stw = settings.vm_config();
    settings.gc_mode = GcMode::GenerationalMarkSweep;
    let generational = settings.vm_config();
    let first = VmContext::new_internal(
        None,
        crate::pal::fs::default_vfs(),
        crate::extension_assembly::freeze(Default::default()),
        stw,
    );
    let second = VmContext::new_internal(
        None,
        crate::pal::fs::default_vfs(),
        crate::extension_assembly::freeze(Default::default()),
        generational,
    );
    assert_eq!(first.heap().mode(), GcMode::StwMarkSweep);
    assert_eq!(second.heap().mode(), GcMode::GenerationalMarkSweep);
    assert_eq!(first.heap().mode(), GcMode::StwMarkSweep);
}
