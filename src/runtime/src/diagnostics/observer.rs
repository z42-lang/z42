//! Non-GC runtime events and per-VM subscriptions.
//!
//! The registry snapshots its subscribers before calling them. Subscriptions
//! added from a callback take effect on the next dispatch.

use std::sync::Arc;

/// Push-based event about activity in a VM.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// Startup or lazy loading published a Z42 program. Byte size is
    /// present when the publisher has it.
    ModuleLoaded {
        name:       String,
        byte_size:  Option<u64>,
    },

    /// JIT setup for a module finished. The function count measures the module
    /// size and the duration measures setup. Function translation can be lazy.
    JitModuleCompiled {
        module_name:     String,
        function_count:  u32,
        duration_us:     u64,
    },

    /// A managed exception was thrown. The interpreter limits the message
    /// prefix to 256 UTF-8 bytes and appends an ellipsis when truncated.
    ExceptionThrown {
        class:    String,
        message:  String,
    },

    /// A user exception was caught by a `try { ... } catch` clause.
    /// `class` is the catch-clause's declared type; `frames_unwound`
    /// is how many z42 call-stack frames were popped between throw
    /// and catch (0 = same-frame catch).
    ExceptionCaught {
        class:           String,
        frames_unwound:  u32,
    },

    /// Dispatch entered a native method or a named native module export.
    NativeCallEntered {
        module:  String,
        symbol:  String,
    },

    /// Generic escape hatch. Lets experimental / internal code emit
    /// events without bumping this enum. Use sparingly — public stable
    /// events graduate to dedicated variants.
    Custom {
        source:  &'static str,
        message: String,
    },
}

/// Subscriber for [`RuntimeEvent`]. `Send + Sync` so observers can live
/// in cross-thread metrics / telemetry pipelines.
pub trait RuntimeObserver: std::fmt::Debug + Send + Sync {
    /// Called once per event. **Must NOT panic** — the fire loop does not
    /// catch unwinds, and a panicking observer aborts the process. Keep
    /// callbacks fast: heavy work belongs in a worker thread fed via channel.
    fn on_event(&self, event: &RuntimeEvent);
}

/// In-process registry, one per `VmCore`. Snapshots the observer list
/// under its own lock before firing so callbacks may re-enter
/// [`RuntimeObserverRegistry::add`] without deadlocking.
#[derive(Debug, Default)]
pub struct RuntimeObserverRegistry {
    inner: parking_lot::Mutex<Vec<Arc<dyn RuntimeObserver>>>,
    /// Mirror of `inner.len()`, readable without the lock. Nearly every run has
    /// no observer; this lets `fire` — and callers that would allocate to build
    /// an event — return after one relaxed load.
    count: std::sync::atomic::AtomicUsize,
}

impl RuntimeObserverRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an observer. Subsequent `fire(...)` calls will deliver to it.
    pub fn add(&self, obs: Arc<dyn RuntimeObserver>) {
        let mut g = self.inner.lock();
        g.push(obs);
        self.count.store(g.len(), std::sync::atomic::Ordering::Release);
    }

    /// Whether any observer is registered. Check it before building an event
    /// that costs something (strings, allocations).
    #[inline]
    pub fn has_observers(&self) -> bool {
        self.count.load(std::sync::atomic::Ordering::Relaxed) != 0
    }

    /// Snapshot + dispatch. Returns the count of observers that received
    /// the event — useful for unit tests; non-test callers ignore.
    pub fn fire(&self, event: &RuntimeEvent) -> usize {
        if !self.has_observers() { return 0; }
        self.dispatch(event)
    }

    #[inline]
    pub(super) fn fire_with(&self, build: impl FnOnce() -> RuntimeEvent) {
        if self.has_observers() {
            self.dispatch(&build());
        }
    }

    fn dispatch(&self, event: &RuntimeEvent) -> usize {
        let snapshot: Vec<Arc<dyn RuntimeObserver>> = {
            let g = self.inner.lock();
            g.iter().cloned().collect()
        };
        let n = snapshot.len();
        for o in &snapshot {
            o.on_event(event);
        }
        n
    }

    /// Number of currently-registered observers (test introspection).
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Publish completed loads without access to the subscription registry.
pub(crate) struct ModuleLoadEvents<'a>(&'a RuntimeObserverRegistry);

impl ModuleLoadEvents<'_> {
    pub(crate) fn loaded(&self, name: String, byte_size: Option<u64>) {
        self.0.fire(&RuntimeEvent::ModuleLoaded { name, byte_size });
    }
}

impl RuntimeObserverRegistry {
    pub(crate) fn module_loads(&self) -> ModuleLoadEvents<'_> {
        ModuleLoadEvents(self)
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "observer_tests.rs"]
mod tests;
