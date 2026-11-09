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
