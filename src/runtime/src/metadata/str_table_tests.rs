//! P1-2 PR 7: `StrTable` — one string id space per VM, interned GC string per id, a hit is a
//! lock-free read.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn module_with_pool(pool: &[&str]) -> Arc<Module> {
    Arc::new(Module {
        name: "Entry".to_string(),
        string_pool: pool.iter().map(|s| s.to_string()).collect(),
        classes: vec![],
        functions: vec![],
        type_registry: Default::default(),
        func_index: Default::default(),
    })
}

/// An allocator that counts its calls (strings are leaked: no heap in these tests).
fn counting(n: &AtomicUsize) -> impl Fn(&str) -> Str + '_ {
    move |t| {
        n.fetch_add(1, Ordering::Relaxed);
        Str::new_leaked(t)
    }
}

fn same(a: Str, b: Str) -> bool {
    a.var_ref().ptr_eq(&b.var_ref())
}

#[test]
fn entry_ids_name_the_module_pool() {
    let m = module_with_pool(&["alpha", "beta"]);
    let t = StrTable::new(Some(Arc::clone(&m)));
    assert_eq!(t.len(), 2);
    assert!(t.is_entry(&m));
    assert_eq!(t.text(0), Some("alpha"));
    assert_eq!(t.text(1), Some("beta"));
    assert_eq!(t.text(2), None);
    assert!(!t.is_entry(&module_with_pool(&["alpha", "beta"])), "identity, not equality");
    assert!(!StrTable::new(None).is_entry(&m));
}

#[test]
fn first_execution_interns_and_later_ones_hit_without_allocating() {
    let t = StrTable::new(Some(module_with_pool(&["alpha"])));
    let n = AtomicUsize::new(0);
    assert!(t.get(0).is_none(), "nothing interned before the first execution");
    let a = t.get_or_intern(0, counting(&n)).expect("in range");
    assert_eq!(a.as_str(), "alpha");
    let b = t.get_or_intern(0, counting(&n)).expect("in range");
    assert!(same(a, b), "a hit returns the interned handle");
    assert!(same(a, t.get(0).expect("interned")));
    assert_eq!(n.load(Ordering::Relaxed), 1, "allocated once");
    assert_eq!(t.interned_count(), 1);
}

#[test]
fn appended_pools_number_after_the_entry_pool_and_ids_are_never_reused() {
    let t = StrTable::new(Some(module_with_pool(&["e0", "e1"])));
    assert_eq!(t.append(vec!["p0".into(), "p1".into()]), Some(2));
    assert_eq!(t.append(vec![]), Some(4), "an empty pool takes no id");
    assert_eq!(t.append(vec!["q0".into()]), Some(4));
    assert_eq!(t.len(), 5);
    assert_eq!(t.text(1), Some("e1"));
    assert_eq!(t.text(3), Some("p1"));
    assert_eq!(t.text(4), Some("q0"));
    let n = AtomicUsize::new(0);
    assert_eq!(t.get_or_intern(3, counting(&n)).map(|s| s.as_str().to_string()).as_deref(), Some("p1"));
    assert!(t.get_or_intern(5, counting(&n)).is_none(), "past the end");
    assert_eq!(n.load(Ordering::Relaxed), 1);
}

#[test]
fn reserved_ids_resolve_to_nothing() {
    let t = StrTable::new(None);
    t.reserve(3);
    assert_eq!(t.len(), 3);
    t.reserve(2);
    assert_eq!(t.len(), 3, "reserve never shrinks");
    let n = AtomicUsize::new(0);
    assert!(t.text(1).is_none());
    assert!(t.get_or_intern(1, counting(&n)).is_none());
    assert_eq!(n.load(Ordering::Relaxed), 0, "nothing to allocate for a reserved id");
    assert_eq!(t.append(vec!["p".into()]), Some(3), "packages number after the reserved ids");
    assert_eq!(t.text(3), Some("p"));
}

#[test]
fn scan_roots_visits_each_interned_string_once() {
    let t = StrTable::new(Some(module_with_pool(&["a", "b", "c"])));
    let n = AtomicUsize::new(0);
    for id in [2, 0, 2, 2, 0] {
        t.get_or_intern(id, counting(&n));
    }
    let mut seen = Vec::new();
    t.scan_roots(|s| seen.push(s.as_str().to_string()));
    assert_eq!(seen, ["c", "a"], "publication order, no duplicates, never-executed ids absent");
}

#[test]
fn racing_first_executions_publish_one_string() {
    let t = Arc::new(StrTable::new(Some(module_with_pool(&["shared"]))));
    let n = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let got: Vec<Str> = (0..8)
        .map(|_| {
            let (t, n, barrier) = (Arc::clone(&t), Arc::clone(&n), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                t.get_or_intern(0, counting(&n)).expect("in range")
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().expect("thread"))
        .collect();
    assert!(got.iter().all(|s| same(*s, got[0])), "every thread gets the winner's string");
    assert_eq!(t.interned_count(), 1, "only the winner is a root");
    assert!(n.load(Ordering::Relaxed) >= 1);
}
