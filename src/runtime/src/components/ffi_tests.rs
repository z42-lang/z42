use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn first() -> *const super::super::abi::ComponentDescriptor {
    CALLS.fetch_add(1, Ordering::SeqCst);
    std::ptr::null()
}
unsafe extern "C" fn conflicting() -> *const super::super::abi::ComponentDescriptor { std::ptr::null() }

#[test]
fn c_registration_is_lazy_idempotent_and_independent_of_host_errors() {
    let id = ComponentId::new(0x1796c0ffee);
    crate::host::error::set_error(crate::host::error::Z42HostStatus::BadConfig, "host sentinel");
    let before = crate::host::error::snapshot_last_error();
    unsafe {
        assert_eq!(z42_component_register_static(id, None), ComponentStatus::INVALID_ARGUMENT);
        assert!(super::super::linked::find(id).is_none());
        assert_eq!(z42_component_register_static(id, Some(first)), ComponentStatus::OK);
        assert_eq!(z42_component_register_static(id, Some(first)), ComponentStatus::OK);
        assert_eq!(z42_component_register_static(id, Some(conflicting)), ComponentStatus::ERROR);
    }
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    let after = crate::host::error::snapshot_last_error();
    assert_eq!(before.code, after.code);
    assert_eq!(before.message, after.message);
    crate::host::error::clear_error();
}

#[cfg(unix)]
#[test]
fn unified_native_header_exposes_host_and_linkage_without_extra_includes() {
    use std::{io::Write, process::{Command, Stdio}};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let compile = |source: &str| {
        let mut child = Command::new("cc")
            .args(["-x", "c", "-std=c11", "-Werror", "-fsyntax-only"])
            .arg("-I").arg(root.join("include"))
            .arg("-I").arg(root.join("src/components"))
            .arg("-").stdin(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    };
    let host = compile("#include \"z42_host.h\"\nZ42HostRef host;\n");
    assert!(host.status.success(), "{}", String::from_utf8_lossy(&host.stderr));
    let composition = compile("#include \"z42_host.h\"\nuint32_t init(void) { return z42_component_register_static(Z42_COMPONENT_INTERP, z42_interp_init); }\n");
    assert!(composition.status.success(), "{}", String::from_utf8_lossy(&composition.stderr));
}

#[test]
fn shared_registration_records_a_distinct_origin_without_initializing() {
    let id = ComponentId::new(0x1802c0ffee);
    unsafe {
        assert_eq!(z42_component_register_shared(id, None), ComponentStatus::INVALID_ARGUMENT);
        assert_eq!(z42_component_register_shared(id, Some(first)), ComponentStatus::OK);
        assert_eq!(z42_component_register_shared(id, Some(first)), ComponentStatus::OK);
        assert_eq!(z42_component_register_static(id, Some(first)), ComponentStatus::ERROR);
    }
    assert_eq!(super::super::linked::origin(id), crate::loader::NativeOrigin::SharedLink);
}
