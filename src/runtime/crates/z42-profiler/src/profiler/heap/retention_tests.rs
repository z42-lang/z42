//! Unit tests for the retention graph logic (add-heap-retention-diagnostics).
//! Pure `RetentionGraph` behaviour (L1 direct referrers / L2 reverse-BFS roots);
//! the GC-first accuracy + heap walk are covered by the e2e golden.

use super::*;

fn ret(id: usize, name: &str) -> RetainerInfo {
    RetainerInfo { kind: RetainerKind::Object, type_name: name.to_string(), id }
}

#[test]
fn direct_referrers_returns_parents() {
    let mut g = RetentionGraph::new();
    g.add_edge(100, ret(1, "Demo.A")); // 1 → 100
    g.add_edge(100, ret(2, "Demo.B")); // 2 → 100
    g.add_edge(200, ret(1, "Demo.A")); // 1 → 200 (unrelated to target 100)

    let mut names: Vec<String> = g.direct_referrers(100).iter().map(|r| r.type_name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["Demo.A".to_string(), "Demo.B".to_string()]);
    assert!(g.direct_referrers(999).is_empty());
}

#[test]
fn direct_referrers_dedups_by_id() {
    let mut g = RetentionGraph::new();
    g.add_edge(100, ret(1, "Demo.A"));
    g.add_edge(100, ret(1, "Demo.A")); // same parent via two slots → one entry
    assert_eq!(g.direct_referrers(100).len(), 1);
}

#[test]
fn retaining_roots_direct() {
    let mut g = RetentionGraph::new();
    g.add_root_edge(100, RootKind::StaticField);
    let roots = g.retaining_roots(100);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].kind, RootKind::StaticField);
}

#[test]
fn retaining_roots_transitive() {
    let mut g = RetentionGraph::new();
    // static → a(1) → b(2): root reaches b via a
    g.add_root_edge(1, RootKind::StaticField);
    g.add_edge(2, ret(1, "Demo.A")); // 1 → 2
    let kinds: Vec<RootKind> = g.retaining_roots(2).iter().map(|r| r.kind).collect();
    assert_eq!(kinds, vec![RootKind::StaticField]);
}

#[test]
fn retaining_roots_dedup_by_kind_and_sorted() {
    let mut g = RetentionGraph::new();
    g.add_root_edge(1, RootKind::StackFrame);
    g.add_root_edge(1, RootKind::StackFrame); // dup kind
    g.add_edge(2, ret(1, "A")); // 1 → 2
    g.add_root_edge(2, RootKind::StaticField);
    let kinds: Vec<RootKind> = g.retaining_roots(2).iter().map(|r| r.kind).collect();
    // deterministic, sorted by discriminant: StaticField(0) then StackFrame(1)
    assert_eq!(kinds, vec![RootKind::StaticField, RootKind::StackFrame]);
}

#[test]
fn no_roots_for_unrooted_object() {
    let mut g = RetentionGraph::new();
    g.add_edge(2, ret(1, "A")); // 1 → 2, but nothing roots 1
    assert!(g.retaining_roots(2).is_empty());
}

#[test]
fn retaining_roots_handles_cycles() {
    let mut g = RetentionGraph::new();
    // cycle a(1) ↔ b(2), rooted at a
    g.add_root_edge(1, RootKind::Pinned);
    g.add_edge(2, ret(1, "A")); // 1 → 2
    g.add_edge(1, ret(2, "B")); // 2 → 1 (cycle)
    // BFS must terminate + still find the root
    let kinds: Vec<RootKind> = g.retaining_roots(2).iter().map(|r| r.kind).collect();
    assert_eq!(kinds, vec![RootKind::Pinned]);
}
