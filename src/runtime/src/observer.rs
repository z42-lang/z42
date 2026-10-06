//! `RuntimeObserver` — push-based event stream for **non-GC** runtime activity.
//!
//! Symmetric pair to existing [`crate::gc::types::GcObserver`]: GcObserver
//! handles allocation / collection events; `RuntimeObserver` covers
//! everything else (module loads, JIT compiles, exception traffic, native
//! calls). The two trait families are kept separate so existing GcObserver
//! consumers don't need to change.
//!
//! CoreCLR parallel: EventPipe (provider-based). docs/review.md Part 4 D3
//! Phase 1 (2026-05-26) — final remaining Part 4 ops/devex item.
//!
//! # Phase 1 scope (this commit)
//!
//! Lands the trait + enum + registration plumbing on `VmCore` + one demo
//! emit site (`ModuleLoaded`, fired from `main.rs` after each successful
//! `metadata::load_artifact`). All other event variants are listed only
//! as Phase 2 placeholders; the enum carries a `Custom { source, payload }`
//! escape hatch so experimental code can emit without bumping the public
//! event enum.
//!
//! Phase 2 (independent refactors):
//! - [`RuntimeEvent::JitCompiled`] ← `jit::compile_module`
//! - [`RuntimeEvent::ExceptionThrown`] / `ExceptionCaught` ← `exception::*`
//! - [`RuntimeEvent::NativeCallEntered`] ← `interp::exec_native`
//!
//! # Concurrency
//!
//! Observers are `Send + Sync` so they can sit in async runtimes / metrics
//! pipelines. The registry is a `Mutex<Vec<Arc<dyn RuntimeObserver>>>` on
//! `VmCore` (process-wide per VM). Fire-event snapshots the Vec under lock
//! then releases before invoking callbacks — reentrant `add_runtime_observer`
//! inside a callback is safe (won't deadlock or skip events for the current
//! fire).

use std::sync::Arc;

/// Push-based runtime event. Phase 1 shipped `ModuleLoaded` + `Custom`;
/// Phase 2 (2026-05-26) wired 4 more covering JIT compile / exception
/// throw+catch / native FFI call sites.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// A `.zbc` / `.zpkg` artifact was loaded into the VM. Fired by
    /// `main.rs` for eager loads (z42.core + user artifact) and by the
    /// lazy loader for on-demand zpkg resolution. Captures canonical
    /// path + on-disk byte size (None when reading from buffer).
    ModuleLoaded {
        name:       String,
        byte_size:  Option<u64>,
    },

    /// A module's JIT compilation finished. Fired from `jit::run` after
    /// `compile_module` returns. Phase 2 emits one event per
    /// compile_module call (covering all module functions); per-function
    /// granularity is deferred. `function_count` is how many functions
    /// Cranelift translated; `duration_us` is wall-clock compile time.
    /// Phase 2 (2026-05-26).
    JitModuleCompiled {
        module_name:     String,
        function_count:  u32,
        duration_us:     u64,
    },

    /// A user exception was thrown via `Terminator::Throw` or JIT's
    /// `set_exception` bridge. `class` is the exception's declared
    /// type-name (e.g. `"Std.Exception"` / user subclass); `message`
    /// is the first 256 chars of the exception's Message field
    /// (truncated to avoid huge payloads in event firehose). Phase 2.
    ExceptionThrown {
        class:    String,
        message:  String,
    },

    /// A user exception was caught by a `try { ... } catch` clause.
    /// `class` is the catch-clause's declared type; `frames_unwound`
    /// is how many z42 call-stack frames were popped between throw
    /// and catch (0 = same-frame catch). Phase 2.
    ExceptionCaught {
        class:           String,
        frames_unwound:  u32,
    },

    /// A native FFI call was dispatched via `Instruction::CallNative`.
    /// `module` is the native library name (e.g. `"libz42_compression"`);
    /// `symbol` is the resolved `extern "C"` symbol called. Phase 2.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Test-only observer: counts events received.
    #[derive(Debug, Default)]
    struct CountingObserver {
        count: AtomicUsize,
    }
    impl RuntimeObserver for CountingObserver {
        fn on_event(&self, _event: &RuntimeEvent) {
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Test-only observer: records every event's variant tag.
    #[derive(Debug, Default)]
    struct RecordingObserver {
        seen: parking_lot::Mutex<Vec<String>>,
    }
    impl RuntimeObserver for RecordingObserver {
        fn on_event(&self, event: &RuntimeEvent) {
            let tag = match event {
                RuntimeEvent::ModuleLoaded { name, .. }       => format!("ModuleLoaded({name})"),
                RuntimeEvent::JitModuleCompiled { module_name, .. } => format!("JitModuleCompiled({module_name})"),
                RuntimeEvent::ExceptionThrown { class, .. }   => format!("ExceptionThrown({class})"),
                RuntimeEvent::ExceptionCaught { class, .. }   => format!("ExceptionCaught({class})"),
                RuntimeEvent::NativeCallEntered { module, symbol } => format!("NativeCallEntered({module}/{symbol})"),
                RuntimeEvent::Custom { source, .. }           => format!("Custom({source})"),
            };
            self.seen.lock().push(tag);
        }
    }

    #[test]
    fn empty_registry_fires_zero() {
        let r = RuntimeObserverRegistry::new();
        let n = r.fire(&RuntimeEvent::Custom { source: "test", message: "hello".into() });
        assert_eq!(n, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn single_observer_receives_event() {
        let r = RuntimeObserverRegistry::new();
        let obs = Arc::new(CountingObserver::default());
        r.add(obs.clone());

        let n = r.fire(&RuntimeEvent::ModuleLoaded {
            name: "z42.core.zpkg".into(),
            byte_size: Some(123456),
        });
        assert_eq!(n, 1);
        assert_eq!(obs.count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn multiple_observers_all_fire() {
        let r = RuntimeObserverRegistry::new();
        let a = Arc::new(CountingObserver::default());
        let b = Arc::new(CountingObserver::default());
        let c = Arc::new(CountingObserver::default());
        r.add(a.clone());
        r.add(b.clone());
        r.add(c.clone());

        r.fire(&RuntimeEvent::Custom { source: "x", message: "y".into() });

        assert_eq!(a.count.load(Ordering::Relaxed), 1);
        assert_eq!(b.count.load(Ordering::Relaxed), 1);
        assert_eq!(c.count.load(Ordering::Relaxed), 1);
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn event_payload_preserved() {
        let r = RuntimeObserverRegistry::new();
        let obs = Arc::new(RecordingObserver::default());
        r.add(obs.clone());

        r.fire(&RuntimeEvent::ModuleLoaded { name: "user.zbc".into(),    byte_size: None });
        r.fire(&RuntimeEvent::Custom       { source: "demo", message: "hi".into() });
        r.fire(&RuntimeEvent::ModuleLoaded { name: "z42.io.zpkg".into(), byte_size: Some(42) });

        let seen = obs.seen.lock().clone();
        assert_eq!(seen, vec![
            "ModuleLoaded(user.zbc)".to_string(),
            "Custom(demo)".to_string(),
            "ModuleLoaded(z42.io.zpkg)".to_string(),
        ]);
    }

    #[test]
    fn phase2_variants_round_trip_through_recorder() {
        let r = RuntimeObserverRegistry::new();
        let obs = Arc::new(RecordingObserver::default());
        r.add(obs.clone());

        r.fire(&RuntimeEvent::JitModuleCompiled { module_name: "mymod".into(), function_count: 10, duration_us: 100 });
        r.fire(&RuntimeEvent::ExceptionThrown   { class: "Std.IO.IOException".into(), message: "boom".into() });
        r.fire(&RuntimeEvent::ExceptionCaught   { class: "Std.Exception".into(), frames_unwound: 3 });
        r.fire(&RuntimeEvent::NativeCallEntered { module: "libz".into(), symbol: "z_init".into() });

        let seen = obs.seen.lock().clone();
        assert_eq!(seen, vec![
            "JitModuleCompiled(mymod)".to_string(),
            "ExceptionThrown(Std.IO.IOException)".to_string(),
            "ExceptionCaught(Std.Exception)".to_string(),
            "NativeCallEntered(libz/z_init)".to_string(),
        ]);
    }

    #[test]
    fn fire_snapshot_allows_reentrant_add() {
        // Observer that adds another observer mid-fire — must not deadlock.
        // We verify by simply not deadlocking; the new observer joins the
        // list and will be called on the *next* fire, not the current one.
        #[derive(Debug)]
        struct AdderObserver {
            registry: Arc<RuntimeObserverRegistry>,
            late:     Arc<CountingObserver>,
            fired:    AtomicUsize,
        }
        impl RuntimeObserver for AdderObserver {
            fn on_event(&self, _: &RuntimeEvent) {
                // Only add once
                if self.fired.fetch_add(1, Ordering::Relaxed) == 0 {
                    self.registry.add(self.late.clone());
                }
            }
        }

        let r = Arc::new(RuntimeObserverRegistry::new());
        let late = Arc::new(CountingObserver::default());
        let adder = Arc::new(AdderObserver {
            registry: r.clone(),
            late: late.clone(),
            fired: AtomicUsize::new(0),
        });
        r.add(adder.clone());

        // First fire: only adder gets event; adder enrolls `late`
        let n1 = r.fire(&RuntimeEvent::Custom { source: "first", message: "".into() });
        assert_eq!(n1, 1);
        assert_eq!(late.count.load(Ordering::Relaxed), 0);

        // Second fire: both adder and late receive
        let n2 = r.fire(&RuntimeEvent::Custom { source: "second", message: "".into() });
        assert_eq!(n2, 2);
        assert_eq!(late.count.load(Ordering::Relaxed), 1);
    }
}
