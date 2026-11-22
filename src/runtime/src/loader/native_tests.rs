//! Loader-side path-policy tests. The install-transaction tests moved with
//! `interop::install` (see interop/install_tests.rs).

use super::*;
use crate::vm::VmContext;

#[test]
fn host_registers_named_paths_and_workers_share_the_service() {
    let ctx = VmContext::new();
    let worker = VmContext::new_with_core(ctx.core_arc());
    assert!(std::ptr::eq(
        ctx.ext::<SearchPaths>().unwrap(),
        worker.ext::<SearchPaths>().unwrap()
    ));
}

#[test]
fn denied_runtime_resolution_does_not_request_vm_services() {
    let policy = crate::loader::NativeLoading {
        allow_runtime: false,
        ..Default::default()
    };
    let error = resolve_symbol(
        &policy,
        "z42_native_denied_services_fixture",
        "missing",
        || panic!("denied resolution must not request the cache"),
        || panic!("denied resolution must not request search paths"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("disabled by this VM"));
}
