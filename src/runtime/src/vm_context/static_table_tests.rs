//! Static field cells (P1-4): ids, every value kind, re-initialisation, GC roots, and concurrent
//! writers racing collections.

use super::*;
use crate::gc::GcMode;
use crate::metadata::types::NativeData;
use crate::vm_context::VmContext;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A layout-less class with one `object` field (its synthesized layout gives it a cell).
fn td(name: &str) -> Arc<crate::metadata::TypeDesc> {
    let fields = vec![crate::metadata::types::FieldSlot { name: "f".into(), type_tag: "object".into(), visibility: 0 }];
    let field_index = fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    Arc::new(crate::metadata::TypeDesc {
        class_flags: 0, visibility: 0, name: name.to_string(), base_name: None, fields, field_index,
        vtable: Vec::new(), vtable_index: crate::metadata::NameIndex::new(), cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

#[test]
fn ids_are_dense_stable_and_lazily_allocated() {
    let t = StaticTable::default();
    assert_eq!(t.id_of("A"), None);
    let a = t.resolve("A");
    let b = t.resolve("B");
    assert_eq!((a.0, b.0), (0, 1));
    assert_eq!(t.resolve("A"), a);
    assert_eq!(t.id_of("B"), Some(b));
    assert_eq!(t.len(), 2);
    assert_eq!(t.get(StaticFieldId(99)), Value::Null, "an id without a cell reads null");
    // The cell's address never changes, however much the table grows.
    let p = t.cell(a).unwrap() as *const StaticCell;
    for i in 0..5000 { t.resolve(&format!("F{i}")); }
    assert!(std::ptr::eq(p, t.cell(a).unwrap()));
}

#[test]
fn every_value_kind_round_trips_and_a_kind_change_is_boxed() {
    let ctx = VmContext::new();
    let _heap = crate::gc::ambient::HeapGuard::enter(ctx.heap());
    let obj = ctx.heap().alloc_object(td("O"), vec![Value::Null], NativeData::None);
    let arr = ctx.heap().alloc_array(vec![Value::I64(1)]);
    let values = [
        Value::I64(-5), Value::F64(2.5), Value::Bool(true), Value::Char('é'),
        Value::Str("text".into()), obj, arr, Value::Null,
    ];
    for (i, v) in values.iter().enumerate() {
        let name = format!("K.{i}");
        ctx.static_set(&name, *v).unwrap();
        assert_eq!(ctx.static_get(&name), *v, "{v:?}");
    }
    // One field taking every kind in turn (`static object o`): each read is the last write.
    for v in values.iter().chain(values.iter().rev()) {
        ctx.static_set("K.any", *v).unwrap();
        assert_eq!(ctx.static_get("K.any"), *v, "{v:?}");
    }
}

#[test]
fn clear_resets_every_value_and_keeps_the_ids() {
    let ctx = VmContext::new();
    ctx.static_set("C.a", Value::I64(1)).unwrap();
    ctx.static_set("C.b", Value::Str("x".into())).unwrap();
    let id = ctx.resolve_static_field_id("C.a");
    ctx.static_fields_clear();
    assert_eq!(ctx.static_get("C.a"), Value::Null);
    assert_eq!(ctx.static_get("C.b"), Value::Null);
    assert_eq!(ctx.resolve_static_field_id("C.a"), id);
    // A reset cell is claimable again by any kind.
    ctx.static_set("C.a", Value::F64(0.5)).unwrap();
    assert_eq!(ctx.static_get("C.a"), Value::F64(0.5));
}

fn live(v: &Value) -> bool {
    match v {
        Value::Object(r) => r.var_ref().is_live(),
        _ => unreachable!(),
    }
}

#[test]
fn statics_are_roots_and_an_overwritten_value_is_collected() {
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::StwMarkSweep);
    let td = td("R");
    let first = ctx.heap().alloc_object(Arc::clone(&td), vec![Value::Null], NativeData::None);
    ctx.static_set("R.keep", first).unwrap();
    ctx.heap().force_collect();
    assert!(live(&first), "reachable only from a static field");
    let second = ctx.heap().alloc_object(td, vec![Value::Null], NativeData::None);
    ctx.static_set("R.keep", second).unwrap();
    ctx.heap().force_collect();
    assert!(live(&second));
    assert!(!live(&first), "the overwritten value is garbage");
}

/// Several mutator threads (each its own context on one VM) keep storing freshly allocated
/// objects and integers into shared statics and reading them back, while collections run at their
/// safepoints. Every read must be a whole value some writer stored (an integer of the expected
/// shape, a live object of the expected type) — a torn cell or a missed root fails it.
#[test]
fn concurrent_writers_never_tear_and_stay_rooted_across_collections() {
    const THREADS: usize = 4;
    const ROUNDS: usize = 3000;
    let ctx = VmContext::new();
    let core = ctx.core_arc();
    let td = td("Shared");
    // Name → id once, like the resolver; the cells are shared by all threads.
    let (ints, refs) = (ctx.resolve_static_field_id("S.int"), ctx.resolve_static_field_id("S.ref"));
    drop(ctx); // the main thread must not count as a running mutator while the workers collect
    let stop = Arc::new(AtomicBool::new(false));
    let workers: Vec<_> = (0..THREADS).map(|t| {
        let (core, td, stop) = (Arc::clone(&core), Arc::clone(&td), Arc::clone(&stop));
        std::thread::spawn(move || {
            let w = VmContext::new_with_core(core);
            for i in 0..ROUNDS {
                // A value whose halves agree: a torn read cannot.
                let n = ((t * ROUNDS + i) as i64) * 0x1_0000_0001;
                w.static_set_by_id(ints, Value::I64(n)).unwrap();
                let obj = w.heap().alloc_object(Arc::clone(&td), vec![Value::I64(n)], NativeData::None);
                w.static_set_by_id(refs, obj).unwrap(); // rooted before the next safepoint
                if i % 97 == t {
                    w.core.needs_auto_collect.store(true, Ordering::Release);
                }
                w.safepoint_skip.store(1, Ordering::Relaxed);
                crate::gc::safepoint::check_safepoint(&w);
                match w.static_get_by_id(ints) {
                    Value::I64(v) => assert_eq!(v as u64 >> 32, v as u64 & 0xFFFF_FFFF, "torn int {v:#x}"),
                    other => panic!("int static read {other:?}"),
                }
                match w.static_get_by_id(refs) {
                    // `borrow` panics on a stale handle: the object must still be alive.
                    Value::Object(r) => assert_eq!(r.borrow().type_desc.name, "Shared"),
                    other => panic!("ref static read {other:?}"),
                }
            }
            stop.store(true, Ordering::Release);
        })
    }).collect();
    for h in workers { h.join().expect("worker"); }
    assert!(stop.load(Ordering::Acquire));
    assert!(core.heap.stats().gc_cycles > 0, "collections ran during the race");
}

/// The offsets and encodings JIT-compiled `StaticGet` / `StaticSet` use (`jit::translate::static_field`)
/// against the Rust accessors: the tag / payload words of the `tparam_cell` protocol and the barrier
/// verdict word.
#[cfg(feature = "jit")]
#[test]
fn jit_layout_matches_the_accessors() {
    use crate::metadata::types::{ref_word, tparam_cell};
    let ctx = VmContext::new();
    let _heap = crate::gc::ambient::HeapGuard::enter(ctx.heap());
    let t = StaticTable::default();
    let raw64 = |c: &StaticCell, off: i32| unsafe { *((c as *const StaticCell as *const u8).add(off as usize) as *const u64) };
    let raw32 = |c: &StaticCell, off: i32| unsafe { *((c as *const StaticCell as *const u8).add(off as usize) as *const u32) };
    for (k, v, bits) in [(1, Value::I64(-3), (-3i64) as u64), (2, Value::F64(1.5), 1.5f64.to_bits()),
                         (3, Value::Bool(true), 1), (4, Value::Char('é'), 'é' as u64)] {
        let c = t.cell(t.resolve(&format!("P.{k}"))).unwrap();
        c.store(&v).unwrap();
        assert_eq!(raw64(c, StaticCell::JIT_TAG_OFFSET), tparam_cell::prim_word(k), "{v:?}: PRIM(k)");
        assert_eq!(raw64(c, StaticCell::JIT_PAYLOAD_OFFSET), bits, "{v:?}: payload");
        assert_eq!(tparam_cell::PRIM_KIND_TO_VALUE_TAG[k as usize] as usize, k as usize - 1, "{v:?}: register tag");
        c.store(&Value::Null).unwrap();
        assert_eq!(raw64(c, StaticCell::JIT_TAG_OFFSET), tparam_cell::null_word(k));
        assert_ne!(tparam_cell::null_word(k) & tparam_cell::NULL_FLAG, 0);
    }
    let s = Value::Str("s".into());
    let c = t.cell(t.resolve("R")).unwrap();
    c.store(&s).unwrap();
    assert_eq!(Some(raw64(c, StaticCell::JIT_TAG_OFFSET)), ref_word::encode(&s), "a reference is the tag word");
    // The barrier verdict.
    assert!(!c.barrier_ready(1), "never recorded");
    c.mark_barrier_ready(0x1234_5678);
    assert_eq!(raw32(c, StaticCell::JIT_READY_OFFSET), 0x1234_5678);
    assert!(c.barrier_ready(0x1234_5678));
    let reg = &ctx.core.cctors;
    assert_eq!(unsafe { (*reg.barrier_epoch_ptr()).load(Ordering::Relaxed) }, reg.barrier_epoch());
}
