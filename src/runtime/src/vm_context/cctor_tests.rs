//! Unit tests for `vm_context::cctor`.

use super::*;

// add-module-init-hook：失败语义（失败是终态、不重试、每次重抛）已由既有的
// `failed_type_reports_error_on_every_later_access` 覆盖 —— 包初始化器复用同一套
// `claim`/`finish`，不另造一份重复断言。这里只测本变更**真正新增**的判定。
// fix-module-init-ownership：判据从「命名空间前缀」换成「**包的符号集合**」。
// 旧版本钉的是前缀语义，所以整段重写 —— 保留的是同一个意图：失败的包只毒它自己。
#[test]
fn module_owns_only_its_own_symbols() {
    // 一个包真实拥有的东西：类型 FQN 与函数 FQN（登记时从整包的类型表+函数表算出）。
    let owned: FxHashSet<String> = [
        "Demo.MiFail.Touch$1",      // 自由函数
        "Demo.MiFail.Api",          // 类型（其静态方法按 owner 判）
        "Demo.MiFail.Sub.Widget",   // 嵌套 ns 下的类型
        "Other.Ns.Sibling",         // ⭐ 同包的**兄弟命名空间** —— 旧前缀判据在这里漏判
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    assert!(module_owns_symbol(&owned, "Demo.MiFail.Touch$1"), "自由函数：精确命中");
    assert!(module_owns_symbol(&owned, "Demo.MiFail.Api"), "类型：精确命中");
    assert!(module_owns_symbol(&owned, "Demo.MiFail.Api.Get$1"), "静态方法：剥末段落到 owner 类型");
    assert!(module_owns_symbol(&owned, "Demo.MiFail.Api.Level"), "静态字段：同上");
    assert!(module_owns_symbol(&owned, "Demo.MiFail.Sub.Widget"), "嵌套 ns 下的类型");

    // ⭐ 这一条是本次修复的要点：同包的兄弟命名空间**必须**判成自己的。
    // 旧判据（前缀 `Demo.MiFail.`）在这里返回 false ⇒ 失败的包上继续跑。
    assert!(
        module_owns_symbol(&owned, "Other.Ns.Sibling"),
        "同包的兄弟命名空间要算自己的 —— 旧前缀判据在这里漏判"
    );

    // ⭐ 另一个方向：不属于本包的，一概不算 —— 即便命名空间前缀看起来像。
    assert!(!module_owns_symbol(&owned, "Std.IO.Console.WriteLine$1"), "别的包不许被毒");
    assert!(!module_owns_symbol(&owned, "Demo.MiFailure.X"), "名字前缀相近但不是成员");
    assert!(!module_owns_symbol(&owned, "Demo.MiFail"), "命名空间本身不是成员");
    assert!(!module_owns_symbol(&owned, "Demo.MiFail.NotMine"), "同 ns 下但不属于本包");
}

// 过判方向的回归钉子：**裸 `Std` 那一格**。实测 11 个包都有 `namespace Std;` 的 CU，
// 所以「初始化器恰好写在裸 Std 里」是写得出来的形态，而旧判据会因此毒掉所有 `Std.*`。
#[test]
fn bare_std_module_init_does_not_poison_other_packages() {
    // z42.compression 的符号（它自己也有一个裸 `Std` 的 CU）。
    let owned: FxHashSet<String> = ["Std.Compression.Deflate", "Std.Archive.ZipReader", "Std.Zip$0"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(module_owns_symbol(&owned, "Std.Archive.ZipReader"), "同包，兄弟 ns");
    assert!(module_owns_symbol(&owned, "Std.Zip$0"), "同包，直接落在裸 Std 下");
    // z42.io 的符号 —— 前缀 `Std.` 会命中，成员判定不会。
    assert!(
        !module_owns_symbol(&owned, "Std.IO.Console.WriteLine$1"),
        "别的包的符号：前缀判据会误命中，成员判据不会"
    );
}

#[test]
fn module_pseudo_type_is_recognised_by_suffix() {
    // 编译器侧合成 `<ns>.$Module`；无 namespace 时就是裸 `$Module`。
    assert!(is_module_pseudo_type("Demo.Lib.$Module"));
    assert!(is_module_pseudo_type("$Module"));
    assert!(!is_module_pseudo_type("Demo.Lib.Module"));
    assert!(!is_module_pseudo_type("Demo.$ModuleThing"));
    assert!(!is_module_pseudo_type("Demo.Lib.C"));
}

#[test]
fn empty_registry_is_never_pending() {
    // 没有任何 cctor 的程序：热路径的门恒 false ⇒ 屏障免费。
    let r = CctorRegistry::default();
    assert!(!r.any_pending());
}

#[test]
fn register_makes_pending_and_finish_clears_it() {
    let r = CctorRegistry::default();
    r.register("A.C", "A.C.C$0");
    assert!(r.any_pending(), "登记后必须待初始化");
    let claimed = r.claim("A.C").unwrap();
    assert_eq!(claimed.as_deref(), Some("A.C.C$0"), "认领应交回 cctor 函数名");
    r.finish("A.C", None);
    assert!(!r.any_pending(), "全部跑完后计数归零 ⇒ 屏障重新免费");
    assert!(r.is_done("A.C"));
}

#[test]
fn register_is_idempotent() {
    let r = CctorRegistry::default();
    r.register("A.C", "f");
    r.register("A.C", "f");
    assert_eq!(r.registered_count(), 1);
    r.finish("A.C", None);
    assert!(!r.any_pending(), "重复登记不得把计数抬高到清不干净");
}

#[test]
fn second_claim_after_done_is_noop() {
    let r = CctorRegistry::default();
    r.register("A.C", "f");
    assert!(r.claim("A.C").unwrap().is_some());
    r.finish("A.C", None);
    assert!(r.claim("A.C").unwrap().is_none(), "已 Done 的类型不得再跑一次");
}

#[test]
fn reentrant_claim_on_same_thread_is_allowed() {
    // C# 语义：cctor 递归触发自身 → 放行（可能看到部分初始化），不死锁。
    let r = CctorRegistry::default();
    r.register("A.C", "f");
    assert!(r.claim("A.C").unwrap().is_some());
    assert!(r.claim("A.C").unwrap().is_none(), "同线程重入必须放行而非阻塞");
}

#[test]
fn failed_type_reports_error_on_every_later_access() {
    // C# 语义：cctor 抛异常 → 类型不可用，后续访问一律报错（不重试）。
    let r = CctorRegistry::default();
    r.register("A.C", "f");
    assert!(r.claim("A.C").unwrap().is_some());
    r.finish("A.C", Some("boom".to_string()));
    assert!(r.any_pending(),
        "失败的类型必须让门继续开着——否则屏障短路、Failed 分支永远检查不到，\
         失败类型会静默变回可用");
    assert_eq!(r.claim("A.C").unwrap_err(), "boom");
    assert_eq!(r.claim("A.C").unwrap_err(), "boom", "失败态必须稳定，不得重试");
}

#[test]
fn unregistered_type_never_blocks() {
    let r = CctorRegistry::default();
    r.register("A.C", "f");
    assert!(r.claim("A.Other").unwrap().is_none(), "没有 cctor 的类型不进表 ⇒ 不受影响");
}

/// runtime-audit A.5: a free function's derived "owner" is its namespace.
/// Looking that up with the loading `try_lookup_type` walked every
/// declared-but-unloaded package (Fallback B) before answering no, so a
/// single free-function call loaded the whole stdlib.
#[test]
fn free_function_barrier_loads_no_package() {
    use crate::metadata::namespace_index::ZpkgCandidate;
    let zpkg = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../compiler/z42.package/tests/fixtures/zpkg-format/packed-minimal/source.zpkg");
    let ctx = crate::vm_context::VmContext::new();
    ctx.install_lazy_loader_with_deps(
        Vec::new(), 0,
        vec![("packed.zpkg".to_string(), ZpkgCandidate { file_path: zpkg, namespaces: vec!["Packed".to_string()] })],
        Vec::new(),
    );
    // Some type with a pending static constructor keeps the global gate open.
    ctx.core.cctors.register("Other.C", "Other.C.$cctor");

    let cell = crate::metadata::bytecode::OwnerInitCell::default();
    assert!(ctx.ensure_callee_owner_init("Demo.Greet$1", &cell).is_ok());
    let loaded = ctx.core.lazy_loader.read().as_ref().map(|l| l.loaded_zpkgs.len());
    assert_eq!(loaded, Some(0), "the barrier for a free function must not load packages");
    assert!(matches!(cell.get(), Some(None)), "a free function is cached as exempt");
}

#[test]
fn owner_class_split() {
    assert_eq!(owner_class_of_static_field("A.B.C.Field"), Some("A.B.C"));
    assert_eq!(owner_class_of_static_field("Field"), None);
    assert_eq!(owner_class_of_static_func("A.C.M$2"), Some("A.C"));
}

/// The static-field barrier remembers its owner answer per field id, so a hot
/// static access does not re-derive the owner from the name each time.
#[test]
fn static_barrier_caches_the_owner_answer_per_field_id() {
    let ctx = crate::vm_context::VmContext::new();
    // Some type with a pending static constructor keeps the global gate open.
    ctx.core.cctors.register("Other.C", "Other.C.$cctor");
    let other = ctx.resolve_static_field_id("Demo.Holder.Other");
    let id = ctx.resolve_static_field_id("Demo.Holder.Count");
    assert!(ctx.ensure_static_owner_init("Demo.Holder.Count", Some(id.0)).is_ok());
    use crate::vm_context::static_table::StaticOwner;
    let owner = |id| ctx.core.statics.cell(id).map(|c| c.owner());
    assert!(matches!(owner(id), Some(StaticOwner::Exempt)), "owner without a cctor is cached as exempt");
    assert!(matches!(owner(other), Some(StaticOwner::Unknown)), "other ids stay unknown");
    ctx.core.statics.forget_owners();
    assert!(matches!(owner(id), Some(StaticOwner::Unknown)), "a module reload forgets the answers");
}

/// 他线程在跑 cctor 时，`claim_with` 阻塞到 `finish`，被唤醒而不是轮询；每轮等待前都调了 `park`。
#[test]
fn waiting_claim_parks_and_wakes_on_finish() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let r = Arc::new(CctorRegistry::default());
    r.register("A.C", "A.C.$cctor");
    assert!(r.claim("A.C").unwrap().is_some(), "本线程认领");

    let parks = Arc::new(AtomicUsize::new(0));
    let waiter = {
        let (r, parks) = (Arc::clone(&r), Arc::clone(&parks));
        std::thread::spawn(move || r.claim_with("A.C", || { parks.fetch_add(1, Ordering::SeqCst); }))
    };
    let start = std::time::Instant::now();
    while parks.load(Ordering::SeqCst) == 0 {
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "等待者没进入等待");
        std::thread::yield_now();
    }
    r.finish("A.C", None);
    assert_eq!(waiter.join().unwrap(), Ok(None), "终态 Done ⇒ 等待者无事可做");
    assert!(start.elapsed() < std::time::Duration::from_secs(5), "finish 必须唤醒等待者");
}

/// 一个线程等着另一个线程跑 cctor 时，GC 暂停请求不必等它：等待期间它算作已 park。
#[test]
fn gc_pause_does_not_wait_for_a_thread_waiting_on_a_cctor() {
    use std::sync::atomic::Ordering;
    use crate::vm_context::VmContext;
    let collector = VmContext::new();
    let core = collector.core_arc();
    core.cctors.register("A.C", "A.C.$cctor");
    assert!(core.cctors.claim("A.C").unwrap().is_some(), "收集者线程正在跑 A.C 的 cctor");

    let waiter = {
        let core = Arc::clone(&core);
        std::thread::spawn(move || {
            let w = VmContext::new_with_core(Arc::clone(&core));
            core.cctors.claim_with("A.C", || crate::gc::safepoint::NativeParkGuard::enter(&w))
        })
    };
    let start = std::time::Instant::now();
    while collector.core.parked_count.load(Ordering::Acquire) < 1 {
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "等待者没有 park");
        std::thread::yield_now();
    }
    let pause = crate::gc::safepoint::request_gc_pause(&collector).expect("uncontended");
    drop(pause);
    collector.core.cctors.finish("A.C", None);
    assert_eq!(waiter.join().unwrap(), Ok(None));
}

/// Every event that can make a passing static-field barrier act again moves the barrier epoch; the
/// events that only make barriers cheaper (a type finishing) and idempotent re-registrations do not.
#[test]
fn barrier_epoch_moves_on_every_invalidating_event() {
    let r = CctorRegistry::default();
    let moved = |f: &dyn Fn()| { let e = r.barrier_epoch(); f(); r.barrier_epoch() != e };
    assert!(moved(&|| r.register("A.C", "A.C.$cctor")), "a static constructor registered");
    assert!(!moved(&|| r.register("A.C", "A.C.$cctor")), "re-registering is a no-op");
    assert!(r.claim("A.C").unwrap().is_some());
    assert!(!moved(&|| r.finish("A.C", None)), "a type finishing only makes barriers cheaper");
    assert!(moved(&|| r.reset_for_rerun()), "a re-run generation");
    let m = Arc::new(crate::metadata::TypeDesc {
        class_flags: 0, visibility: 0, name: "Demo.$Module".to_string(), base_name: None,
        fields: Vec::new(), field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(), cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    assert!(moved(&|| r.register_module_init(&m, FxHashSet::default())), "a package initializer registered");
    r.register("Demo.$Module", "Demo.$Module.$init");
    assert!(r.claim("Demo.$Module").unwrap().is_some());
    r.finish("Demo.$Module", Some("boom".to_string()));
    assert!(moved(&|| r.refresh_module_pending()), "a package initializer failed");
    assert!(!moved(&|| r.refresh_module_pending()), "nothing changed since");
}

/// The barrier records its verdict in the field's cell only once it keeps passing for every thread.
#[test]
fn static_field_barrier_records_only_a_settled_verdict() {
    let ctx = crate::vm_context::VmContext::new();
    let id = ctx.resolve_static_field_id("Demo.Holder.Count");
    let cell = || ctx.core.statics.cell(id).unwrap();
    // No static constructor pending at all: settled.
    assert!(ctx.ensure_static_field_init("Demo.Holder.Count", Some(id.0)).is_ok());
    assert!(cell().barrier_ready(ctx.core.cctors.barrier_epoch()));
    // A constructor registered elsewhere invalidates the record; the owner (no cctor) is exempt.
    ctx.core.cctors.register("Other.C", "Other.C.$cctor");
    assert!(!cell().barrier_ready(ctx.core.cctors.barrier_epoch()));
    assert!(ctx.ensure_static_field_init("Demo.Holder.Count", Some(id.0)).is_ok());
    assert!(cell().barrier_ready(ctx.core.cctors.barrier_epoch()));
    // An explicit module (re)load forgets owners and verdicts.
    ctx.forget_static_owners();
    assert!(!cell().barrier_ready(ctx.core.cctors.barrier_epoch()));
    // A site without a resolved id passes and records nothing.
    assert!(ctx.ensure_static_field_init("Demo.Holder.Count", None).is_ok());
    assert!(!cell().barrier_ready(ctx.core.cctors.barrier_epoch()));
    // A package initializer still to run (here: running on this thread, so the barrier passes
    // re-entrantly) is not settled.
    let m = Arc::new(crate::metadata::TypeDesc {
        class_flags: 0, visibility: 0, name: "Demo.$Module".to_string(), base_name: None,
        fields: Vec::new(), field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(), cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    ctx.core.cctors.register_module_init(&m, FxHashSet::default());
    ctx.core.cctors.register("Demo.$Module", "Demo.$Module.$init");
    assert!(ctx.core.cctors.claim("Demo.$Module").unwrap().is_some());
    assert!(ctx.ensure_static_field_init("Demo.Holder.Count", Some(id.0)).is_ok());
    assert!(!cell().barrier_ready(ctx.core.cctors.barrier_epoch()), "re-entrant pass is not settled");
}

/// `callee_barrier_settled` (the call-barrier twin of the static-field settle
/// verdict, consumed by `jit::helpers::call::call_compiled`): only a barrier
/// that keeps passing until the epoch moves may be short-circuited.
#[test]
fn callee_barrier_settled_tracks_the_same_events_as_the_epoch() {
    let ctx = crate::vm_context::VmContext::new();
    let owner = crate::metadata::bytecode::OwnerInitCell::default();
    // Nothing pending anywhere: settled (this is the `any_cctor_pending == false`
    // world, where `call_compiled` never even reaches the gate).
    assert!(ctx.callee_barrier_settled(&owner));
    // A static constructor pending elsewhere + this callee's owner unresolved:
    // not settled (the full barrier resolves the owner cell first).
    ctx.core.cctors.register("Other.C", "Other.C.$cctor");
    assert!(!ctx.callee_barrier_settled(&owner));
    // Owner resolved to "no static constructor" (a free function's steady
    // state): exempt for good.
    let _ = owner.set(None);
    assert!(ctx.callee_barrier_settled(&owner));
    // A package initializer pending: never settled, owner exempt or not.
    let m = Arc::new(crate::metadata::TypeDesc {
        class_flags: 0, visibility: 0, name: "Demo.$Module".to_string(), base_name: None,
        fields: Vec::new(), field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(), cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    ctx.core.cctors.register_module_init(&m, FxHashSet::default());
    assert!(!ctx.callee_barrier_settled(&owner));
}
