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

#[test]
fn worker_retains_facade_resources_until_last_context_drop() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use crate::pal::fs::Vfs;
    struct Service(Arc<AtomicUsize>);
    impl Drop for Service {
        fn drop(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let fs = Arc::new(crate::pal::vfs::MemoryFs::new());
    fs.mount("/owned", b"shared".to_vec());
    let vfs: Arc<dyn Vfs> = fs;
    let mut extensions = crate::vm::ext::VmExtensions::new();
    extensions.register(Service(drops.clone())).unwrap();
    let primary = VmContext::with_extensions(None, vfs.clone(), extensions);
    let core = Arc::downgrade(&primary.core_arc());
    let worker = VmContext::new_with_core(primary.core_arc());
    assert_eq!(worker.core.threads.vm_contexts.lock().len(), 2);
    assert!(std::ptr::eq(primary.ext::<Service>().unwrap(), worker.ext::<Service>().unwrap()));
    drop(primary);
    assert!(core.upgrade().is_some());
    assert_eq!(worker.core.threads.vm_contexts.lock().len(), 1);
    assert!(Arc::ptr_eq(worker.vfs(), &vfs));
    assert_eq!(worker.vfs().read("/owned").unwrap(), b"shared");
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(worker);
    assert!(core.upgrade().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
