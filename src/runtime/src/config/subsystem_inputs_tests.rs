use super::*;
use crate::gc::{Heap, GcMode};

#[test]
fn vm_inputs_capture_resolved_units_and_optional_trace_state() {
    let values = [
        ("Z42_GC_MODE", "stw"),
        ("Z42_GC_MAX_BYTES", "64M"),
        ("Z42_GC_LOH_BYTES", "96K"),
        ("Z42_GC_TRACE", "1"),
        ("Z42_SAMPLE_HZ", "37"),
        ("Z42_TRACE_OUT", "trace.json"),
        ("Z42_GC_PAUSE_WINDOW", "3"),
    ];
    let mut runtime = RuntimeConfig::from_getter(|key| {
        values.iter().find(|(name, _)| *name == key).map(|(_, value)| value.to_string())
    });
    let captured = runtime.vm_config();
    assert_eq!(captured.gc.mode, GcMode::StwMarkSweep);
    assert_eq!(captured.gc.pause_window, 3);
    assert_eq!(captured.gc_max_bytes, Some(64 * 1024 * 1024));
    assert_eq!(captured.process_loh_bytes, Some(96 * 1024));
    assert!(captured.gc_trace);
    assert_eq!(captured.sample_hz, Some(37));
    assert!(captured.trace_timeline);

    runtime.sample_hz = None;
    runtime.trace_out = None;
    assert_eq!(captured.sample_hz, Some(37));
    assert!(captured.trace_timeline);
    assert!(!runtime.vm_config().trace_timeline);
    runtime.trace_out = Some("".into());
    assert!(runtime.vm_config().trace_timeline);
}

#[test]
fn parsed_defaults_match_standalone_heap_policy() {
    let runtime = RuntimeConfig::from_getter(|_| None);
    assert_eq!(runtime.gc_config(), GcConfig::default());
    assert_eq!(RuntimeConfig::default().gc_config(), GcConfig::default());
}

#[test]
fn resolved_overrides_reach_the_heap_without_changing_other_inputs() {
    let values = [
        ("Z42_GC_MODE", "stw"),
        ("Z42_GC_MINOR_THRESHOLD", "0.5"),
        ("Z42_GC_PAUSE_WINDOW", "3"),
        ("Z42_GC_SOFT_THRESHOLD", "0.25"),
        ("Z42_GC_NURSERY_BYTES", "2M"),
        ("Z42_GC_PROMOTION_AGE", "1"),
        ("Z42_GC_ADAPTIVE_PROMOTION", "0"),
        ("Z42_GC_INCREMENTAL", "0"),
        ("Z42_GC_PAUSE_TARGET_MS", "4"),
        ("Z42_GC_SLICE_MS", "0.5"),
        ("Z42_GC_PHASES", "1"),
        ("Z42_GC_VERIFY_CARDS", "1"),
        ("Z42_GC_NEAR_LIMIT_RATIO", "0.95"),
        ("Z42_GC_PRESSURE_RATIO", "0.7"),
    ];
    let runtime = RuntimeConfig::from_getter(|key| {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    });
    let config = runtime.gc_config();
    assert_eq!(
        config,
        GcConfig {
            mode: GcMode::StwMarkSweep,
            minor_threshold: 0.5,
            pause_window: 3,
            soft_threshold: 0.25,
            nursery_bytes: Some(2 * 1024 * 1024),
            promotion_age: Some(1),
            adaptive_promotion: false,
            incremental: false,
            pause_target_ms: 4.0,
            slice_ms: 0.5,
            phases: true,
            verify_cards: true,
            near_limit_ratio: 0.95,
            pressure_ratio: 0.7,
        }
    );
    let heap = Heap::new(config);
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
    let untouched = RuntimeConfig::from_getter(|_| None);
    assert_eq!(untouched.gc_config(), GcConfig::default());
}

#[test]
fn stack_allocation_policy_maps_resolved_values_without_global_state() {
    use crate::exec::call::stack_alloc::StackAllocPolicy as Policy;
    for (value, expected) in [(None, Policy::On), (Some("on"), Policy::On),
        (Some("off"), Policy::Off), (Some("0"), Policy::Off), (Some("heap"), Policy::Off),
        (Some("stats"), Policy::Stats)] {
        let mut settings = RuntimeConfig::from_getter(|_| None);
        settings.stackalloc = value.map(str::to_owned);
        let captured = settings.vm_config();
        settings.stackalloc = Some("off".into());
        assert_eq!(captured.stack_alloc, expected);
    }
}

#[test]
fn safepoint_policy_captures_resolved_value() {
    let mut settings = RuntimeConfig::from_getter(|_| None);
    settings.safepoint_throttle = 17;
    let captured = settings.vm_config();
    settings.safepoint_throttle = 31;
    assert_eq!(captured.safepoint_throttle, 17);
}
