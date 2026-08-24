//! Unit tests for the safepoint sampling profiler (`gc::sampler`).
//!
//! Cover the format-critical bits that end-to-end runs can't cheaply assert:
//! folded aggregation + ordering, empty-stack safety, frame-tree interning
//! (dedup shared prefixes), and the chrome/perfetto JSON shape. E2E (a hot
//! script under `Z42_SAMPLE_HZ`) lives in the change's验证 step.

use super::*;
use std::sync::atomic::AtomicU64;

/// Unique temp path per call so parallel cargo tests don't collide.
fn temp_path(tag: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("z42_sampler_{tag}_{pid}_{n}"))
        .to_string_lossy()
        .into_owned()
}

fn arcs(names: &[&str]) -> Vec<Arc<str>> {
    names.iter().map(|s| Arc::from(*s)).collect()
}

#[test]
fn folded_aggregates_and_counts() {
    let s = Sampler::for_test(false);
    let hot = arcs(&["Main", "foo", "bar"]);
    let warm = arcs(&["Main", "foo"]);
    s.record(&hot, 0);
    s.record(&hot, 1);
    s.record(&hot, 2);
    s.record(&warm, 3);
    assert_eq!(s.sample_count(), 4, "total samples = sum of counts");

    let path = temp_path("folded");
    s.flush_folded(&path).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    // Descending by count → the 3× stack must be first.
    assert_eq!(lines[0], "Main;foo;bar 3", "hottest first, `;`-joined, count suffix");
    assert_eq!(lines[1], "Main;foo 1");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn empty_stack_produces_no_row() {
    let s = Sampler::for_test(false);
    s.record(&[], 0); // must be a no-op, never a bad "<empty> 0" row
    assert_eq!(s.sample_count(), 0);
    let path = temp_path("empty");
    s.flush_folded(&path).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.is_empty(), "no samples → empty folded file, got {body:?}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn maybe_sample_snapshots_call_stack() {
    use crate::exception::VmFrame;
    let ctx = VmContext::new();
    {
        let mut cs = ctx.call_stack.lock();
        cs.push(VmFrame::new(Arc::from("Main"), Arc::from("t.z42"), std::ptr::null(), std::ptr::null()));
        cs.push(VmFrame::new(Arc::from("foo"), Arc::from("t.z42"), std::ptr::null(), std::ptr::null()));
    }
    let s = Sampler::for_test(false);
    // No pending flag yet → no sample.
    s.maybe_sample(&ctx);
    assert_eq!(s.sample_count(), 0, "no sample until timer flags one");
    // Timer flags a sample → next safepoint snapshots the stack.
    s.force_pending();
    s.maybe_sample(&ctx);
    assert_eq!(s.sample_count(), 1);
    // Flag consumed (swap(false)) → a second call without re-flagging is a no-op.
    s.maybe_sample(&ctx);
    assert_eq!(s.sample_count(), 1, "pending flag is one-shot per tick");

    let path = temp_path("maybe");
    s.flush_folded(&path).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert_eq!(body.trim(), "Main;foo 1", "stack bottom-on-left folded key");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn trace_off_records_no_timeline() {
    let s = Sampler::for_test(false); // trace disabled
    s.record(&arcs(&["Main", "foo"]), 10);
    // folded accumulated but no per-sample timeline → trace has 0 P events.
    let path = temp_path("trace_off");
    s.flush_trace(&path).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("\"traceEvents\":[]"), "no timeline when trace off: {body}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn trace_emits_perfetto_json_with_interned_frames() {
    let s = Sampler::for_test(true);
    // Two stacks sharing the `Main;foo` prefix → frame tree dedups it.
    s.record(&arcs(&["Main", "foo", "bar"]), 100);
    s.record(&arcs(&["Main", "foo", "baz"]), 200);
    let path = temp_path("trace_on");
    s.flush_trace(&path).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();

    // Structural: object with both keys.
    assert!(body.starts_with("{\"traceEvents\":["), "trace head: {body}");
    assert!(body.contains("\"stackFrames\":{"), "has stackFrames dict");
    // Two sample events (ph:P), at the two timestamps.
    assert_eq!(body.matches("\"ph\":\"P\"").count(), 2, "one P event per sample");
    assert!(body.contains("\"ts\":100"));
    assert!(body.contains("\"ts\":200"));
    // Interning: Main, foo shared; bar, baz distinct → 4 frame nodes total.
    // Node ids are 0..=3; a `parent` link proves the tree structure.
    assert!(body.contains("\"name\":\"Main\""));
    assert!(body.contains("\"name\":\"bar\""));
    assert!(body.contains("\"name\":\"baz\""));
    assert!(body.contains("\"parent\":"), "child frames carry parent links");
    // Sanity: valid-looking JSON object bounds.
    assert!(body.ends_with("}}"), "closes traceEvents+stackFrames object");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn interned_frame_tree_shares_prefixes() {
    let mut d = SamplerData::default();
    let leaf1 = intern_stack(&mut d, &arcs(&["Main", "foo", "bar"]));
    let leaf2 = intern_stack(&mut d, &arcs(&["Main", "foo", "baz"]));
    let leaf3 = intern_stack(&mut d, &arcs(&["Main", "foo", "bar"])); // repeat
    assert_ne!(leaf1, leaf2, "bar and baz are distinct leaves");
    assert_eq!(leaf1, leaf3, "identical stack reuses the same leaf id");
    // Main(0), foo(1), bar(2), baz(3) = 4 unique nodes.
    assert_eq!(d.frames.len(), 4, "shared Main;foo prefix interned once");
}

#[test]
fn json_escape_handles_specials() {
    assert_eq!(json_escape("plain"), "plain");
    assert_eq!(json_escape("a\"b"), "a\\\"b");
    assert_eq!(json_escape("a\\b"), "a\\\\b");
    // Typical z42 generic-mangled name — no escaping needed.
    assert_eq!(json_escape("List$1.Add"), "List$1.Add");
}
