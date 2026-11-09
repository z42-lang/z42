//! Process-lifetime panic forwarding for `extern "C"` shims.
//!
//! The VM installs one hook at boot so `z42-rs` shim recovery
//! (`native_helpers::set_panic`, emitted by `#[z42::methods]`) can forward
//! panic text into the VM's per-thread last-error slot WITHOUT a link-time
//! `extern "C"` symbol. Link-time host symbols cannot be resolved by DLLs
//! linked below the VM (z42-repl's cdylib, component libraries) on Windows,
//! and the reference rides along even when unreferenced once the linker
//! merges codegen units.
//!
//! The hook pointer is per-image: it is read by `z42-rs` code statically
//! linked into the same image as the VM that installed it (test binaries,
//! statically registered modules). A separately dlopened module image has
//! its own unset instance and forwards nowhere — cross-image forwarding
//! needs an explicit host-protocol entry, not a silent extern.

use core::sync::atomic::{AtomicUsize, Ordering};

type PanicHook = fn(&str);

static PANIC_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install the process-lifetime panic forwarder. First installation wins;
/// every VM installs the same forwarding function, so repeats are no-ops.
pub fn install_panic_hook(hook: PanicHook) {
    let _ = PANIC_HOOK.compare_exchange(0, hook as usize, Ordering::Relaxed, Ordering::Relaxed);
}

/// Forward a shim panic message to the installed hook, if any.
pub fn forward_panic(msg: &str) {
    let encoded = PANIC_HOOK.load(Ordering::Relaxed);
    if encoded != 0 {
        let hook: PanicHook = unsafe { core::mem::transmute(encoded) };
        hook(msg);
    }
}
