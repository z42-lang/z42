use super::*;

#[test]
fn facade_registers_paths_and_workers_share_the_service() {
    let ctx = VmContext::new();
    assert!(source(&ctx).is_ok());
    let worker = VmContext::new_with_core(ctx.core.clone());
    assert!(std::ptr::eq(
        ctx.ext::<Paths>().unwrap(),
        worker.ext::<Paths>().unwrap()
    ));
}

use crate::assembly::context::VmContextFactory as _;
