use super::{current_vm, VmGuard};
use crate::vm::context::VmContext;

#[test]
fn same_context_nesting_preserves_the_outer_activation() {
    let ctx = VmContext::new();
    assert!(current_vm().is_none());
    {
        let _outer = VmGuard::enter(&ctx);
        {
            let _inner = VmGuard::enter(&ctx);
            assert!(std::ptr::eq(current_vm().unwrap(), &*ctx));
        }
        assert!(std::ptr::eq(current_vm().unwrap(), &*ctx));
    }
    assert!(current_vm().is_none());
}

#[test]
fn cross_context_unwind_restores_the_outer_activation() {
    let outer = VmContext::new();
    let inner = VmContext::new();
    assert!(current_vm().is_none());
    {
        let _outer = VmGuard::enter(&outer);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner = VmGuard::enter(&inner);
            assert!(std::ptr::eq(current_vm().unwrap(), &*inner));
            panic!("native callback unwind");
        }));
        assert!(result.is_err());
        assert!(std::ptr::eq(current_vm().unwrap(), &*outer));
    }
    assert!(current_vm().is_none());
}

use crate::assembly::context::VmContextFactory as _;
