//! Per-type and package initialization facts shared by one VM.
//! Pending counters prove absence of work only when zero; failed types keep their
//! barrier active. Execution, parked waits and exception construction belong to exec/init.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// 类级 attr-ref 哨兵名。**必须与编译器侧 `IrStaticCtor.Sentinel` 逐字一致**
/// （`src/compiler/z42.package/src/IrModule.z42`）——两处手写同一个字符串是漂移源，
/// 故各自只写一次、并在此标明对应关系。
pub const CCTOR_SENTINEL: &str = "$Cctor";

/// 包级初始化伪类型的**短名**（add-module-init-hook）。FQ 名 = `<ns>.$Module`。
/// **必须与编译器侧 `ModuleInitScan.PseudoTypeName` 逐字一致**
/// （`src/compiler/z42c.semantics/src/Validation/ModuleInitScan.z42`）——同 `CCTOR_SENTINEL`，
/// 两处手写同一个字符串是漂移源，故各自只写一次并互相标明。
pub const MODULE_PSEUDO_TYPE: &str = "$Module";

/// 这个类型 FQ 名是不是包级初始化伪类型。
pub fn is_module_pseudo_type(type_fq: &str) -> bool {
    match type_fq.rsplit_once('.') {
        Some((_, last)) => last == MODULE_PSEUDO_TYPE,
        None => type_fq == MODULE_PSEUDO_TYPE,
    }
}

/// 包初始化伪类型 `module_fq`（`<ns>.$Module`）是否**覆盖**符号 `sym_fq`。
///
/// 判据 = 命名空间前缀 `<ns>.`，与编译器**放置**伪类型的规则同源
/// （`ModuleInitSynth.Emit` 用 CU 的 `g.Ns` 限定）——不是第二套约定。
/// 无命名空间的裸 `$Module` 覆盖一切。尾点不可省：`Demo.Lib.` 不匹配 `Demo.Library.X`。
/// 归属判定：`sym_fq` 是否属于某个包 —— **先确认是哪个包，再无关命名空间**。
///
/// # 为什么不能按命名空间前缀判（这是本函数存在的全部理由）
///
/// 旧判据是「`<ns>.$Module` 覆盖 `<ns>.` 开头的一切」。而 `$Module` 落在哪个 ns，由**谁写了
/// `[ModuleInit]` 那个 CU** 决定，与「这个包拥有哪些符号」没有关系 ⇒ 两个方向都会错：
///
/// - **漏判**：`z42.compression` 的初始化器写在 `namespace Std.Compression` ⇒ 前缀
///   `Std.Compression.`；失败后触达**同包**的 `Std.Archive.ZipReader` 不重抛，在一个初始化
///   失败的包上继续跑。
/// - **过判**：同一个包写在裸 `namespace Std`（实测 **11 个包**都有这样的 CU：z42.core 77 个、
///   z42.net 8 个、z42.io 6 个……`z42.compression` 自己也有）⇒ 前缀 `Std.` 命中
///   `Std.IO.Console.WriteLine`，那是 **z42.io** 的符号 ⇒ 一个包的失败毒掉另一个包。
///
/// 根因：**命名空间在 z42 里不是包的边界**（同 E0497 那条诊断反复强调的判据：
/// 「看类型的归属包，不是 `using` 的命名空间」）。所以任何形式的前缀匹配都不可能精确 ——
/// 连「按包的真实符号算出 owner 前缀集合」也不行，因为那个集合照样会含 `Std.`。
///
/// ⇒ 判据换成**成员归属**：登记时记下该包实际拥有的符号名，判定查集合。
///
/// # `sym_fq` 的三种形态
///
/// 屏障的三类触达点分别传：类型 FQN（`Ns.T`）、静态字段 FQN（`Ns.T.F`）、函数 FQN
/// （`Ns.F$n` 或 `Ns.T.M$n`）。故先查本身，不中则**逐段剥末段**再查，以命中字段/方法的 owner。
///
/// ⚠️ 泛型实例化名里含 `.`（`Std.List<Std.IO.Foo>`）时剥段会剥出无意义的中间串 —— 那只会
/// **查不中**（集合里没有这种串），不会误判；类型 FQN 本来就走第一步的精确匹配。
pub fn module_owns_symbol(owned: &FxHashSet<String>, sym_fq: &str) -> bool {
    if owned.contains(sym_fq) {
        return true;
    }
    let mut rest = sym_fq;
    while let Some((head, _)) = rest.rsplit_once('.') {
        if owned.contains(head) {
            return true;
        }
        rest = head;
    }
    false
}

/// 单个类型的 cctor 状态。
#[derive(Debug, Clone)]
pub enum CctorState {
    /// 尚未运行。
    NotRun,
    /// 正在运行（记住是哪个线程，用于**重入放行**：C# 允许 cctor 递归触发自身，
    /// 此时直接放行、可能看到部分初始化的状态，而不是死锁）。
    Running(std::thread::ThreadId),
    /// 已成功完成。
    Done,
    /// 运行时抛了异常。C# 语义：该类型此后**不可用**，任何访问都抛包装异常。
    /// 载荷是原始异常的文本，用于构造包装异常的 message。
    Failed(String),
}

/// 一个有静态构造器的类型的登记项。
#[derive(Debug, Clone)]
pub struct CctorEntry {
    /// cctor 的发射函数名（FQ）——来自编译期 `$Cctor` 哨兵的载荷。
    /// 带着它就不必在运行期按约定拼名，避免编译期命名与运行期拼名两处规则漂移。
    pub func: String,
    pub state: CctorState,
}

/// Package initializer and the exact set of symbols its package owns.
#[derive(Debug)]
struct ModuleInitEntry {
    desc: Arc<crate::object::TypeDesc>,
    owned: FxHashSet<String>,
}

/// Per-VM initialization facts for types that declare a static constructor.
/// Registration follows loader-write-lock -> map-lock order; no data operation
/// accesses the loader while holding the map. The execution layer shares map and
/// Condvar within this crate so it can claim and wait under that same lock.
pub struct CctorRegistry {
    /// 类 FQ → 登记项。
    pub(crate) map: Mutex<FxHashMap<String, CctorEntry>>,
    /// 与 `map` 配对：`finish` 把某个类型推到终态时唤醒在 `claim` 里等它的线程。
    pub(crate) done_cv: std::sync::Condvar,
    /// 还没到达终态（Done / Failed）的类型数。**热路径唯一要读的东西**。
    ///
    /// 只在 `== 0` 方向被信任（「可证无事可做」）；非 0 时一律走慢路复核，
    /// 故不存在「读到过期非 0 值」导致的正确性问题——只会多做一次查表。
    pending: AtomicUsize,
    /// unify-static-init-into-cctor（7.3）：初始化代际。`static_fields_clear()` 清空全部
    /// 静态槽时递增，使所有 `TypeDesc` 上的无锁快路标记一次性失效、类型初始化器随之重跑。
    /// 从 1 起（`TypeDescCold::init_gen` 默认 0 = 从未初始化，不会与任何有效代际相等）。
    generation: AtomicU32,
    /// add-module-init-hook: the `<ns>.$Module` pseudo-types of every loaded package,
    /// in load order. Unlike ordinary types these are **not** left to the access-point
    /// barrier — they are driven eagerly right after their package is loaded.
    module_inits: Mutex<Vec<ModuleInitEntry>>,
    /// How many of `module_inits` have not reached `Done`. Same idiom as `pending`:
    /// trusted only in the `== 0` direction, so the barrier is one relaxed load once
    /// every package initializer has run.
    ///
    /// 失败是**终态**，不计入这里 —— 它计进 `module_failed`。此前把 `Failed` 也算作
    /// 「未完成」，门因此永远开着，于是**全程序每一次调用**都重跑屏障、重抛那个包的失败
    /// （连 `catch` 块里的 `Console.WriteLine` 也躲不过）—— 见 `module_failed` 的注释。
    module_pending: AtomicUsize,
    /// fix-module-init-failure-scope: 已登记且**已失败**的包初始化器数。
    ///
    /// 与 `module_pending` 分开计，是因为两者驱动的是两件不同的事：
    /// - `module_pending != 0` ⇒ 还有初始化器**要跑**，任何屏障点都该把它们跑掉（「加载即执行」）；
    /// - `module_failed   != 0` ⇒ 有包处于失败终态，只有**触达该包**的屏障该重抛。
    ///
    /// 合并成一个计数就是这条缺陷的由来：一个包失败后，`Std.IO.Console.WriteLine` 这种
    /// 与它毫无关系的调用也会被判成「触达」。文档一直写的是「之后每一次**触达**都会再抛」
    /// （`docs/reference/src/language/module-initializers.md`），实现却做成了「每一次调用」。
    module_failed: AtomicUsize,
    /// 屏障纪元：一次「放行过的屏障可能不再放行」的事件就加一（从 1 起，单元的 0 = 从未放行）。
    /// 事件 = 登记一个静态构造器 / 包初始化器、包初始化失败、重跑代际递增、显式模块（重）加载清空属主缓存。
    /// 静态字段单元记下它放行时的纪元（`static_table::StaticCell::barrier_ready`），相等即整个屏障是空操作——
    /// interp 与 JIT 内联快路读的是同一对字。只在上述冷事件里递增（每次都伴随加载或失败），不会回绕。
    barrier_epoch: AtomicU32,
}

impl Default for CctorRegistry {
    fn default() -> Self {
        Self {
            map: Mutex::default(),
            done_cv: std::sync::Condvar::new(),
            pending: AtomicUsize::new(0),
            generation: AtomicU32::new(1),
            module_inits: Mutex::default(),
            module_pending: AtomicUsize::new(0),
            module_failed: AtomicUsize::new(0),
            barrier_epoch: AtomicU32::new(1),
        }
    }
}

impl CctorRegistry {
    /// `module_pending` / `module_failed` 的字节偏移：JIT 内联字符串原语（`jit::translate::str_intrinsics`）
    /// 在快路径上直接读这两个计数，代替 `jit_call` 的包级屏障门（[`crate::vm::context::VmContext::module_init_gate_open`]）。
    #[cfg(feature = "jit")]
    pub(crate) const MODULE_PENDING_OFFSET: usize = std::mem::offset_of!(CctorRegistry, module_pending);
    #[cfg(feature = "jit")]
    pub(crate) const MODULE_FAILED_OFFSET: usize = std::mem::offset_of!(CctorRegistry, module_failed);

    /// 当前屏障纪元（见字段注释）；一次 relaxed load。
    #[inline(always)]
    pub(crate) fn barrier_epoch(&self) -> u32 { self.barrier_epoch.load(Ordering::Relaxed) }

    /// [`Self::barrier_epoch`] for a slow path about to record a verdict under it: acquire, so every
    /// state change published before the epoch moved is visible to the barrier that follows.
    #[inline]
    pub(crate) fn barrier_epoch_acquire(&self) -> u32 { self.barrier_epoch.load(Ordering::Acquire) }

    /// 纪元字的地址：JIT 内联的静态字段快路把它烘焙进生成码（registry 与 JIT 同属 VmCore）。
    #[cfg(feature = "jit")]
    pub(crate) fn barrier_epoch_ptr(&self) -> *const AtomicU32 { &self.barrier_epoch }

    /// 让所有已记下的「屏障已放行」失效。在引发它的状态变化**之后**调用（release）：读到新纪元的线程
    /// 也看得到那次变化，于是不会拿旧结论记新纪元。
    pub(crate) fn invalidate_barriers(&self) {
        self.barrier_epoch.fetch_add(1, Ordering::Release);
    }

    /// 当前初始化代际。快路比对用；一次 relaxed load。
    #[inline(always)]
    pub fn generation(&self) -> u32 { self.generation.load(Ordering::Relaxed) }

    /// `static_fields_clear()` 专用：静态槽被清空 ⇒ 已跑过的类型初始化器**必须重跑**。
    /// 递增代际使全部快路标记失效，并把 map 里的终态条目复位成 `NotRun`。
    ///
    /// ⚠️ `Failed` 也复位：槽位既已清空，之前那次失败的结论不再适用于新一代；
    /// 若它仍会失败，重跑时会再次失败并重新登记。
    pub fn reset_for_rerun(&self) {
        {
            let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
            let mut revived = 0usize;
            for e in m.values_mut() {
                if matches!(e.state, CctorState::Done | CctorState::Failed(_)) {
                    e.state = CctorState::NotRun;
                    revived += 1;
                }
            }
            self.pending.fetch_add(revived, Ordering::Release);
        }
        self.generation.fetch_add(1, Ordering::Release);
        // fix-module-init-failure-scope：包级两个计数也镜像着 map，复位后必须跟着重算 ——
        // 否则一个曾 `Failed` 的 `$Module` 被复位成 `NotRun`，`module_pending` 却仍是 0，
        // 屏障不会再去跑它（`module_failed` 则停在 1，白走慢路）。锁已在上面的作用域里释放。
        self.refresh_module_pending();
        self.invalidate_barriers();
    }

    /// 热路径的门。`false` ⇒ 全程序没有任何待初始化的 cctor，屏障可直接跳过。
    #[inline(always)]
    pub fn any_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) != 0
    }

    // ── 包级初始化（add-module-init-hook）────────────────────────────────────────────────
    /// 热路径的门。`false` ⇒ 所有已加载包的初始化器都已跑完（或一个都没有），
    /// 屏障是一次 relaxed load。
    #[inline(always)]
    pub fn any_module_init_pending(&self) -> bool {
        self.module_pending.load(Ordering::Relaxed) != 0
    }

    /// 加载期登记一个包的 `<ns>.$Module` 伪类型。由 `LazyLoader::insert_type` 在自己的写锁内
    /// 调用 —— **只记账，不执行**：初始化器是用户代码，在加载锁内跑它会重入符号解析而死锁。
    /// 执行归屏障（`VmContext::ensure_module_inits`），那时锁已释放。
    /// `owned` = **这个包实际拥有的符号名集合**（类型 FQN ∪ 函数 FQN），归属判定的唯一依据。
    ///
    /// 🔴 它是**必填参数**，不是可选项：这样「某条加载路径忘了供」在**编译期**就过不去，
    /// 不存在「漏供 ⇒ 静默退回按命名空间猜」这种形态（本仓库反复吃亏的正是这类静默失效）。
    /// 调用方必须在**整包**视野下算它 —— 逐类型漏斗（`insert_type`）那时类型表还没填完。
    pub fn register_module_init(
        &self,
        desc: &Arc<crate::object::TypeDesc>,
        owned: FxHashSet<String>,
    ) {
        let mut v = self.module_inits.lock().unwrap_or_else(|e| e.into_inner());
        if v.iter().any(|e| e.desc.name == desc.name) {
            return;   // 幂等：同一包重复加载不重复登记
        }
        v.push(ModuleInitEntry { desc: Arc::clone(desc), owned });
        self.module_pending.fetch_add(1, Ordering::Release);
        self.invalidate_barriers();
    }

    /// 当前登记的全部包初始化伪类型（快照；遍历时不持锁，避免初始化器重入时自锁）。
    pub fn module_init_snapshot(&self) -> Vec<Arc<crate::object::TypeDesc>> {
        self.module_inits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|e| Arc::clone(&e.desc))
            .collect()
    }

    /// 按 map 里的**真实终态**重算 `module_pending`。
    ///
    /// ⚠️ 不能用「ensure 返回了 Ok」来判完成：初始化器内部再触发屏障时，`claim` 对**同线程重入**
    /// 直接放行并返回 Ok（C# 同款语义），此时初始化器其实还在跑。按 `Done` 判定才是真的。
    ///
    /// fix-module-init-failure-scope：`Failed` 是**终态**，从 `module_pending` 移到
    /// `module_failed`。两者驱动的是两件不同的事（见字段注释）——前者「把还没跑的跑掉」，
    /// 后者「触达失败包时重抛」，后者必须带归属判定（[`Self::failed_module_owning`]）。
    pub fn refresh_module_pending(&self) {
        let names: Vec<String> = {
            let v = self.module_inits.lock().unwrap_or_else(|e| e.into_inner());
            v.iter().map(|e| e.desc.name.clone()).collect()
        };
        let m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        let mut remaining = 0usize;
        let mut failed = 0usize;
        for n in &names {
            match m.get(n.as_str()).map(|e| &e.state) {
                Some(CctorState::Done)      => {}
                Some(CctorState::Failed(_)) => failed += 1,
                _                           => remaining += 1,
            }
        }
        let was_remaining = self.module_pending.swap(remaining, Ordering::Release);
        let was_failed = self.module_failed.swap(failed, Ordering::Release);
        // 新出现的失败 / 又有待跑的初始化器：之前放行的屏障可能不再放行。
        if failed != was_failed || remaining > was_remaining {
            self.invalidate_barriers();
        }
    }

    /// 热路径的门（失败侧）。`false` ⇒ 没有任何包处于初始化失败终态。
    #[inline(always)]
    pub fn any_module_init_failed(&self) -> bool {
        self.module_failed.load(Ordering::Relaxed) != 0
    }

    /// 这个包初始化伪类型当前是不是失败终态。
    pub fn is_module_init_failed(&self, type_fq: &str) -> bool {
        let m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        matches!(m.get(type_fq).map(|e| &e.state), Some(CctorState::Failed(_)))
    }

    /// **归属判定**：`sym_fq`（被调函数 / 被 new 的类型 / 静态字段的 FQ 名）是否落在某个
    /// **已失败**的包里；是则返回那个包的 `<ns>.$Module` 伪类型描述符，供调用方用
    /// `ensure_type_init` 重抛（复用同一条错误文案，不另写一份）。
    ///
    /// 判据 = 命名空间前缀：伪类型 FQ 是 `<ns>.$Module`，覆盖 `<ns>.` 开头的一切。这与编译器
    /// **放置**伪类型的规则同源（`ModuleInitSynth.Emit` 用 CU 的 `g.Ns` 限定），不是第二套
    /// 约定 —— 两侧不会各自漂移。
    ///
    /// ✅ 那条「已知边界」已消除：判据从命名空间前缀换成**包的符号集合**，
    /// 所以「一个包声明了互不嵌套的多个命名空间」不再影响归属 —— 同包的兄弟命名空间照样重抛，
    /// 别的包也不会被毒。见 [`module_owns_symbol`] 的头注。
    pub fn failed_module_owning(&self, sym_fq: &str) -> Option<Arc<crate::object::TypeDesc>> {
        if !self.any_module_init_failed() { return None; }
        // 先取快照再拿 map 锁：初始化器是用户代码，遍历时持 module_inits 锁会在重入时自锁。
        let entries: Vec<(Arc<crate::object::TypeDesc>, FxHashSet<String>)> = self
            .module_inits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|e| (Arc::clone(&e.desc), e.owned.clone()))
            .collect();
        let m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        for (td, owned) in entries {
            if !matches!(m.get(td.name.as_str()).map(|e| &e.state), Some(CctorState::Failed(_))) {
                continue;
            }
            // 「先确认哪个包」：查该包的符号集合，而不是比命名空间前缀。
            if module_owns_symbol(&owned, sym_fq) { return Some(td); }
        }
        None
    }

    /// 加载期登记一个「有静态构造器」的类型。幂等（同一类型重复登记不重复计数）。
    pub fn register(&self, class_fq: &str, func: &str) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if m.contains_key(class_fq) {
            return;
        }
        m.insert(
            class_fq.to_string(),
            CctorEntry { func: func.to_string(), state: CctorState::NotRun },
        );
        // 计数在插入的同一把锁下自增，与它镜像的 map 内容保持一致。
        self.pending.fetch_add(1, Ordering::Release);
        self.invalidate_barriers();
    }

    /// 认领方在 cctor 跑完后回调：`err = None` 成功，`Some(msg)` 失败。
    ///
    /// ⚠️ **只有成功才减 `pending`**。失败的类型必须让门**继续开着**——C# 语义是
    /// 「失败是终态、后续每次访问都抛」，而门一旦短路，屏障就不再执行，`claim` 的
    /// `Failed` 分支永远检查不到，失败类型会**静默变回可用**（实测：第二次读静态字段
    /// 不但没抛，还读出了 cctor 抛出前写进去的半成品值）。
    ///
    /// 代价是一旦有类型初始化失败，门就长期开着（每次静态访问多一次查表）。可以接受：
    /// 那已经是个致命错误路径，正确性优先于它之后的性能。
    pub fn finish(&self, class_fq: &str, err: Option<String>) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = m.get_mut(class_fq) {
            let was_terminal = matches!(e.state, CctorState::Done | CctorState::Failed(_));
            let failed = err.is_some();
            e.state = match err {
                None => CctorState::Done,
                Some(msg) => CctorState::Failed(msg),
            };
            if !was_terminal && !failed {
                self.pending.fetch_sub(1, Ordering::Release);
            }
        }
        drop(m);
        self.done_cv.notify_all();
    }

    /// 该类型是否已进入终态（测试用）。
    pub fn is_done(&self, class_fq: &str) -> bool {
        let m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        matches!(m.get(class_fq).map(|e| &e.state), Some(CctorState::Done))
    }

    /// 登记的 cctor 类型总数（测试用——「屏障是否真的只对有 cctor 的类型生效」要可断言）。
    pub fn registered_count(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}



/// 从静态字段的全限定名取所属类 FQ：`A.B.C.Field` → `A.B.C`。
/// 无点 → `None`（全局静态字段，不属于任何类）。
pub fn owner_class_of_static_field(field_fq: &str) -> Option<&str> {
    field_fq.rfind('.').map(|i| &field_fq[..i])
}

/// 从静态方法的全限定名取所属类 FQ。与静态字段同构，但方法名可能带 `$N` mangle，
/// 那部分在最后一段里，不影响按最后一个 `.` 切分。特化名的实参是全名
/// （`Demo.C.Echo:Demo.A.P`）、实例化 owner 的实参也是（`Demo.G<Demo.A.P>.M`）——只看顶层的点。
pub fn owner_class_of_static_func(func_fq: &str) -> Option<&str> {
    crate::metadata::index::type_names::last_top_dot(func_fq).map(|i| &func_fq[..i])
}


#[cfg(test)]
#[path = "cctor_tests.rs"]
mod tests;
