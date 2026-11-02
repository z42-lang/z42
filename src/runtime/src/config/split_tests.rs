use super::*;
use crate::gc::{ArcMagrGC, GcMode, MagrGC};

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
    let heap = ArcMagrGC::new(config);
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
    let untouched = RuntimeConfig::from_getter(|_| None);
    assert_eq!(untouched.gc_config(), GcConfig::default());
}
