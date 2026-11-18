//! Loader-side path-policy tests. The install-transaction tests moved with
//! `interop::install` (see interop/install_tests.rs).

use super::*;

#[test]
fn host_registers_named_paths_and_workers_share_the_service() {
    let ctx = VmContext::new();
    assert!(path_source(&ctx).is_ok());
    let worker = VmContext::new_with_core(ctx.core_arc());
    assert!(std::ptr::eq(
        ctx.ext::<SearchPaths>().unwrap(),
        worker.ext::<SearchPaths>().unwrap()
    ));
}
