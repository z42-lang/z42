use super::*;

// ── 2. Roots ─────────────────────────────────────────────────────────────────

#[test]
fn pin_root_increments_count_and_for_each_visits() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![Value::I64(7)]);
    let _h = heap.pin_root(v);
    assert_eq!(heap.stats().roots_pinned, 1);

    let mut count = 0;
    heap.for_each_root(&mut |_v| count += 1);
    assert_eq!(count, 1);
}

#[test]
fn unpin_root_decrements_count() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![]);
    let h = heap.pin_root(v);
    heap.unpin_root(h);
    assert_eq!(heap.stats().roots_pinned, 0);
    let mut count = 0;
    heap.for_each_root(&mut |_| count += 1);
    assert_eq!(count, 0);
}

#[test]
fn enter_leave_frame_drops_intra_frame_pins() {
    let heap = ArcMagrGC::new();
    let mark = heap.enter_frame();
    let _ = heap.pin_root(heap.alloc_array(vec![]));
    let _ = heap.pin_root(heap.alloc_array(vec![]));
    assert_eq!(heap.stats().roots_pinned, 2);
    heap.leave_frame(mark);
    assert_eq!(heap.stats().roots_pinned, 0);
}

#[test]
fn nested_frames_unwind_correctly() {
    let heap = ArcMagrGC::new();
    let m1 = heap.enter_frame();
    let _  = heap.pin_root(heap.alloc_array(vec![]));
    let m2 = heap.enter_frame();
    let _  = heap.pin_root(heap.alloc_array(vec![]));
    let _  = heap.pin_root(heap.alloc_array(vec![]));
    assert_eq!(heap.stats().roots_pinned, 3);
    heap.leave_frame(m2);
    assert_eq!(heap.stats().roots_pinned, 1);
    heap.leave_frame(m1);
    assert_eq!(heap.stats().roots_pinned, 0);
}

#[test]
fn pin_outside_frame_persists() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![]);
    let _h = heap.pin_root(v);
    let mark = heap.enter_frame();
    heap.leave_frame(mark);
    // Outside-frame pin survives leave_frame
    assert_eq!(heap.stats().roots_pinned, 1);
}

// ── Interp stack scanning (Phase 3f) ─────────────────────────────────────────

#[test]
fn frame_held_outer_with_inner_chain_protected_by_stack_scan() {
    // Phase 3f bug 场景：frame.regs 持 outer，outer 通过 slot 间接持 inner
    // （inner 没在 reg 里）。如果 scanner 不扫 frame regs，outer 在 unreachable
    // 集里，inner 通过 outer.slot 引用被算 internal 减除 → tentative=0 → inner
    // 被错误清空。Phase 3f 把 frame regs 暴露给 scanner 修复此问题。
    //
    // 本测试用 set_external_root_scanner 直接模拟：把"frame regs Vec" 通过
    // Rc<RefCell> 共享给 scanner 闭包（与 vm_context.rs 中实际 raw-ptr 实现等价）。
    let heap = ArcMagrGC::new();

    let inner = heap.alloc_object(dummy_type_desc("Inner"), vec![Value::I64(42)], NativeData::None);
    let outer = heap.alloc_object(dummy_type_desc("Outer"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &outer else { panic!() };
        g.borrow_mut().refs[0] = inner.clone();
    }

    let frame_regs: std::sync::Arc<parking_lot::Mutex<Vec<Value>>>
        = std::sync::Arc::new(parking_lot::Mutex::new(vec![outer.clone()]));

    {
        let fr = frame_regs.clone();
        heap.set_external_root_scanner(Box::new(move |visit| {
            for v in fr.lock().iter() {
                visit(v);
            }
        }));
    }

    drop(outer);
    drop(inner);

    heap.collect_cycles();

    // Verify: outer / inner 数据 intact（inner.refs[0] = 42 未被错误清空）
    let regs = frame_regs.lock();
    let Value::Object(outer_gc) = &regs[0] else { panic!() };
    let inner_val = outer_gc.borrow().refs[0].clone();
    let Value::Object(inner_gc) = &inner_val else {
        panic!("inner should still be Object, got {:?}", inner_val);
    };
    assert_eq!(inner_gc.borrow().refs[0], Value::I64(42),
        "Phase 3f: transitively reachable inner survives collect when outer in frame regs");
}

// ── External root scanner (Phase 3d.1) ───────────────────────────────────────

#[test]
fn external_root_scanner_called_during_collect() {
    let heap = ArcMagrGC::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    heap.set_external_root_scanner(Box::new(move |_visit| {
        c.fetch_add(1, Ordering::SeqCst);
    }));

    heap.collect_cycles();
    assert!(calls.load(Ordering::SeqCst) >= 1, "scanner invoked during mark phase");
}

#[test]
fn cycle_reachable_via_external_scanner_is_preserved() {
    use parking_lot::Mutex as StdMutex;
    use std::sync::Arc as StdArc;

    let heap = ArcMagrGC::new();

    // 模拟 VmContext.static_fields 风格的外部容器
    let external: StdArc<StdMutex<Vec<Value>>> = StdArc::new(StdMutex::new(Vec::new()));

    // scanner 把 external 中所有 Value 暴露为 roots
    {
        let ext = external.clone();
        heap.set_external_root_scanner(Box::new(move |visit| {
            for v in ext.lock().iter() {
                visit(v);
            }
        }));
    }

    // 构造一个 a-b 环
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &a else { panic!() };
        g.borrow_mut().refs[0] = b.clone();
        let Value::Object(g) = &b else { panic!() };
        g.borrow_mut().refs[0] = a.clone();
    }

    // 把 a 放到 external（模拟 static_fields 持有），drop 本地强引用
    external.lock().push(a.clone());
    drop(a);
    drop(b);

    heap.collect_cycles();

    // a 应该仍然存活（external scanner 把它喂进 reachable）
    // b 也存活（a.refs[0] = b 链可达）
    let mut count = 0;
    heap.iterate_live_objects(&mut |_| count += 1);
    assert_eq!(count, 2, "external-rooted cycle survives collect");

    // 关键：a 的 slots 应该 intact（不被误清）
    let alive_a = external.lock()[0].clone();
    let Value::Object(g) = &alive_a else { panic!() };
    let slot0 = g.borrow().refs[0].clone();
    assert!(matches!(slot0, Value::Object(_)),
        "a.refs[0] still references b (NOT cleared by collector)");
}

#[test]
fn cycle_unreachable_from_external_scanner_still_collected() {
    use parking_lot::Mutex as StdMutex;
    use std::sync::Arc as StdArc;

    let heap = ArcMagrGC::new();
    let external: StdArc<StdMutex<Vec<Value>>> = StdArc::new(StdMutex::new(Vec::new()));
    {
        let ext = external.clone();
        heap.set_external_root_scanner(Box::new(move |visit| {
            for v in ext.lock().iter() { visit(v); }
        }));
    }

    // 构造 a-b 环但**不**把 a 放进 external
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &a else { panic!() };
        g.borrow_mut().refs[0] = b.clone();
        let Value::Object(g) = &b else { panic!() };
        g.borrow_mut().refs[0] = a.clone();
    }
    drop(a); drop(b);

    heap.collect_cycles();

    // 没在 external 里 → 仍是 unreachable → 被收集
    let mut count = 0;
    heap.iterate_live_objects(&mut |_| count += 1);
    assert_eq!(count, 0, "non-rooted cycle still collected");
}

