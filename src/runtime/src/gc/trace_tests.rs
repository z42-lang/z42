use super::*;

#[test]
fn human_uses_binary_units() {
    assert_eq!(human(512), "512B");
    assert_eq!(human(1024), "1.0K");
    assert_eq!(human(1024 * 1024), "1.0M");
    assert_eq!(human(3 * 1024 * 1024 * 1024), "3.0G");
}

#[test]
fn tracer_brackets_before_and_after() {
    // The tracer must survive an AfterCollect with no preceding BeforeCollect
    // (observer installed mid-cycle) without underflowing the "after" figure.
    let t = GcTracer::default();
    t.on_event(&GcEvent::AfterCollect {
        kind: GcKind::Full, freed_bytes: 4096, pause_us: 1500,
    });
    // used_before is 0 here; saturating_sub keeps it at 0 rather than wrapping.
    assert_eq!(t.used_before.load(Ordering::Relaxed), 0);
    assert_eq!(t.cycles.load(Ordering::Relaxed), 1);

    t.on_event(&GcEvent::BeforeCollect { kind: GcKind::Full, used_bytes: 8192 });
    assert_eq!(t.used_before.load(Ordering::Relaxed), 8192);
    t.on_event(&GcEvent::AfterCollect {
        kind: GcKind::Full, freed_bytes: 4096, pause_us: 900,
    });
    assert_eq!(t.cycles.load(Ordering::Relaxed), 2);
}

#[test]
fn out_of_memory_events_are_deduped() {
    // OutOfMemory fires per allocation once over budget; the tracer must print
    // the first and count the rest (this flooded stderr before it was deduped).
    let t = GcTracer::default();
    for _ in 0..5000 {
        t.on_event(&GcEvent::OutOfMemory { requested_bytes: 112, limit_bytes: 64 << 20 });
    }
    assert_eq!(t.oom_suppressed.load(Ordering::Relaxed), 5000,
        "every event is counted even though only the first prints");
    // A collection drains the counter so the next burst reports again.
    t.on_event(&GcEvent::AfterCollect { kind: GcKind::Full, freed_bytes: 0, pause_us: 1 });
    assert_eq!(t.oom_suppressed.load(Ordering::Relaxed), 0);
    t.on_event(&GcEvent::OutOfMemory { requested_bytes: 112, limit_bytes: 64 << 20 });
    assert_eq!(t.oom_suppressed.load(Ordering::Relaxed), 1);
}
