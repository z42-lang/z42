use super::*;
use crate::zpkg::ir::{BasicBlock, Terminator};
use crate::vm::exec::call::frames::VmFrame;
use std::sync::{Arc, Barrier, atomic::{AtomicUsize, Ordering}};

fn module(throws: bool) -> Module {
    let f = Function::new(crate::zpkg::ir::Function {
        name: "Echo".into(), param_count: 1, ret_type: "long".into(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock { label: "entry".into(), instructions: vec![],
            terminator: if throws { Terminator::Throw { reg: 0 } } else { Terminator::Ret { reg: Some(0) } } }],
        is_static: true, visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 2, cold: None, reg_types: Box::new([]), block_index: Default::default(),
        branch_targets: vec![], frame_meta: None,
    });
    Module::new(crate::zpkg::image::ModuleData { name: "Engine".into(),
        string_pool: vec![], classes: vec![], func_index: [("Echo".into(), 0)].into_iter().collect() },
        vec![f], Default::default())
}

fn returned(result: Result<ExecOutcome>) -> Value {
    match result.unwrap() { ExecOutcome::Returned(Some(v)) => v, other => panic!("{other:?}") }
}

#[test]
fn registered_engine_keeps_borrowed_argument_order_and_receiver() {
    let m = module(false); let ctx = VmContext::new(); let f = &m.functions[0];
    assert!(matches!(returned(invoke(&ctx, &m, f, &[Value::I64(17)])), Value::I64(17)));
    let regs = [Value::I64(11), Value::I64(22)];
    assert!(matches!(returned(invoke_args(&ctx, &m, f,
        CallArgs::Registers { regs: &regs, indices: &[1, 0] }, &[])), Value::I64(22)));
    assert!(matches!(returned(invoke_args(&ctx, &m, f,
        CallArgs::Receiver { receiver: Value::I64(33), regs: &regs, indices: &[1] }, &[])), Value::I64(33)));
    assert_eq!(ctx.exec.call_stack.depth(), 0);
}

#[test]
fn registered_engine_preserves_typed_throw_channel_and_frame_exit() {
    let m = module(true); let ctx = VmContext::new();
    assert!(matches!(invoke(&ctx, &m, &m.functions[0], &[Value::I64(41)]).unwrap(),
        ExecOutcome::Thrown(Value::I64(41))));
    assert_eq!(ctx.exec.call_stack.depth(), 0);
}

#[test]
fn absent_registered_backend_is_an_explicit_error() {
    let m = module(false); let ctx = VmContext::new(); let registry = EngineRegistry::new(None);
    assert!(registry.invoke(&ctx, &m, &m.functions[0], CallArgs::Values(&[]), &[])
        .unwrap_err().to_string().contains("no baseline engine"));
    assert!(registry.run(ExecMode::Interp, &ctx, &m, &m.functions[0])
        .unwrap_err().to_string().contains("no baseline engine"));
    assert!(registry.run(ExecMode::Jit, &ctx, &m, &m.functions[0])
        .unwrap_err().to_string().contains("JIT backend unavailable"));
    assert!(registry.run(ExecMode::Aot, &ctx, &m, &m.functions[0])
        .unwrap_err().to_string().contains("not implemented"));
}

struct ProbeTier { id: usize, dropped: Arc<AtomicUsize> }
impl Drop for ProbeTier { fn drop(&mut self) { self.dropped.fetch_add(1, Ordering::Relaxed); } }
impl TierBackend for ProbeTier {
    fn osr_threshold(&self) -> u32 { 77 }
    fn invoke(&self, _ctx: &VmContext, _target: TierTarget<'_>, _args: CallArgs<'_>, _method_type_args: &[String])
        -> Option<Result<ExecOutcome>> { Some(Ok(ExecOutcome::Returned(Some(Value::I64(self.id as i64))))) }
    fn osr(&self, _ctx: &VmContext, _func: &Function, _header: usize, _state: OsrState<'_>)
        -> Option<Result<ExecOutcome>> { None }
}

#[test]
fn tier_registry_race_publishes_one_owner_and_reclaims_losers() {
    let registry = Arc::new(EngineRegistry::new(None));
    let barrier = Arc::new(Barrier::new(8)); let dropped = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..8).map(|id| {
        let registry = registry.clone(); let barrier = barrier.clone(); let dropped = dropped.clone();
        std::thread::spawn(move || {
            barrier.wait();
            let _ = registry.install_tier(TierApi::new(ProbeTier { id, dropped }));
            registry.tier().unwrap().backend::<ProbeTier>().unwrap().id
        })
    }).collect();
    let ids: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(ids.iter().all(|id| *id == ids[0]));
    assert_eq!(dropped.load(Ordering::Relaxed), 7);
    drop(registry); assert_eq!(dropped.load(Ordering::Relaxed), 8);
}

#[test]
fn worker_contexts_share_their_vm_tier_and_refresh_only_at_entry() {
    let a = VmContext::new(); let b = VmContext::new();
    let worker = VmContext::new_with_core(a.core.clone());
    assert!(a.tier().is_none() && b.tier().is_none() && worker.tier().is_none());
    assert!(a.core.exec.engines.install_tier(TierApi::new(ProbeTier { id: 5,
        dropped: Arc::new(AtomicUsize::new(0)) })).is_ok());
    a.publish_tier();
    #[cfg(feature = "jit")]
    assert!(worker.tier().is_none(), "pre-existing context refreshes at its bottom frame push");
    let core = a.core.clone();
    let id = std::thread::spawn(move || {
        let ctx = VmContext::new_with_core(core);
        ctx.tier().unwrap().backend::<ProbeTier>().unwrap().id
    }).join().unwrap();
    assert_eq!(id, 5);
    worker.push_frame(VmFrame::new(std::ptr::null(), std::ptr::null()));
    assert!(std::ptr::eq(a.tier().unwrap(), worker.tier().unwrap()));
    assert_eq!(worker.tier().unwrap().osr_threshold, 77);
    worker.pop_frame(); assert!(b.tier().is_none());
    drop(a); assert_eq!(worker.tier().unwrap().backend::<ProbeTier>().unwrap().id, 5);
}

#[test]
fn neutral_tier_pointer_preserves_compiled_context_head_layout() {
    use crate::vm::exec::thread_state::JitHead;
    #[repr(C)]
    struct ExpectedHead {
        stack_limit: std::sync::atomic::AtomicUsize,
        cctors: *const crate::vm::runtime::statics::CctorRegistry,
        #[cfg(feature = "jit")]
        jit: std::sync::atomic::AtomicPtr<()>,
    }
    assert_eq!(std::mem::size_of::<JitHead>(), std::mem::size_of::<ExpectedHead>());
    assert_eq!(std::mem::align_of::<JitHead>(), std::mem::align_of::<ExpectedHead>());
    assert_eq!(std::mem::offset_of!(JitHead, stack_limit), std::mem::offset_of!(ExpectedHead, stack_limit));
    assert_eq!(std::mem::offset_of!(JitHead, cctors), std::mem::offset_of!(ExpectedHead, cctors));
    #[cfg(feature = "jit")]
    assert_eq!(std::mem::offset_of!(JitHead, jit), std::mem::offset_of!(ExpectedHead, jit));
}
