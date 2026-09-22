//! 静态构造函数（cctor）的按类型初始化状态机（add-static-constructors）。
//!
//! # C# 语义
//!
//! 每个类型的静态构造器**至多执行一次**，在**首次使用该类型之前**（创建实例 / 读写其静态
//! 字段 / 调用其静态方法）。抛异常则该类型被标记为**失败**，后续任何访问抛包装异常
//! （对标 C# `TypeInitializationException`）。
//!
//! # 为什么屏障几乎免费（方案 B 的实现）
//!
//! C# 的 cctor 屏障之所以零成本，是 JIT 在初始化完成后把检查从机器码里 patch 掉。
//! **z42 的 JIT 没有代码 patch / 失效机制**，所以不能照抄。
//!
//! 这里用 `cctor_pending`（无锁 `AtomicUsize`，记「还没跑完的 cctor 类型数」）当门：
//!
//! ```text
//! if cctor_pending() != 0 { ensure_type_init(class) }   // 热路径全部代价
//! ```
//!
//! - 程序里**没有**静态构造器 → 计数恒 0 → 屏障是一次 relaxed load，实际免费
//! - 有静态构造器 → 只在它们**跑完之前**付查表代价；**全部跑完后计数归零，屏障重新免费**
//!
//! 比原方案 B 的承诺更强：不只「只有有 cctor 的类型付」，而且「付的时段也有界」。
//!
//! 这个「无锁镜像只用于 `== 0`（可证无事可做）方向」的手法不是新发明——旁边的
//! `pending_type_init_count` / `running_static_inits` 用的是同一套 idiom，见
//! `vm_context/types.rs` 的 cache-failed-name-resolution 注释。
//!
//! # 与既有 `defer-class-initialization` 的区别
//!
//! 既有机制把「静态字段引用触发类初始化」放在**名字→id 解析**这条每名一次的冷路径上，
//! 理由写在 `VmCore::pending_type_inits` 的注释里：热路径 `static_get_by_id` 因
//! `Value::Null` 是合法值而**无法区分「未初始化」**。
//!
//! 本状态机不靠值推断状态——状态是显式的——所以可以把屏障放在**真正的访问点**，
//! 拿到 C# 的「首次使用前」而不是「首次提及时」。

use rustc_hash::FxHashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// 类级 attr-ref 哨兵名。**必须与编译器侧 `IrStaticCtor.Sentinel` 逐字一致**
/// （`src/libraries/z42.ir/src/IrModule.z42`）——两处手写同一个字符串是漂移源，
/// 故各自只写一次、并在此标明对应关系。
pub const CCTOR_SENTINEL: &str = "$Cctor";

/// `claim` 等待他线程跑完类型初始化器的上限。取足够宽松的值：正常的初始化器是毫秒级，
/// 撞到这个上限基本只意味着跨线程循环初始化（C# 在同样场景会直接死锁）。
const WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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

/// 全体有 cctor 的类型的状态表。**只登记有静态构造器的类型**——没有的类型
/// 根本不进这张表，也就不可能拖慢它们。
///
/// 锁顺序：惰性加载器在持有自己的写锁时调 [`Self::register`]（加载器写锁 → `map`）。
/// 反方向不存在——本类型的任何方法都**不会**在持有 `map` 时访问加载器——故不会死锁。
#[derive(Debug)]
pub struct CctorRegistry {
    /// 类 FQ → 登记项。
    map: Mutex<FxHashMap<String, CctorEntry>>,
    /// 还没到达终态（Done / Failed）的类型数。**热路径唯一要读的东西**。
    ///
    /// 只在 `== 0` 方向被信任（「可证无事可做」）；非 0 时一律走慢路复核，
    /// 故不存在「读到过期非 0 值」导致的正确性问题——只会多做一次查表。
    pending: AtomicUsize,
    /// unify-static-init-into-cctor（7.3）：初始化代际。`static_fields_clear()` 清空全部
    /// 静态槽时递增，使所有 `TypeDesc` 上的无锁快路标记一次性失效、类型初始化器随之重跑。
    /// 从 1 起（`TypeDescCold::init_gen` 默认 0 = 从未初始化，不会与任何有效代际相等）。
    generation: AtomicU32,
}

impl Default for CctorRegistry {
    fn default() -> Self {
        Self { map: Mutex::default(), pending: AtomicUsize::new(0), generation: AtomicU32::new(1) }
    }
}

impl CctorRegistry {
    /// 当前初始化代际。快路比对用；一次 relaxed load。
    #[inline(always)]
    pub fn generation(&self) -> u32 { self.generation.load(Ordering::Relaxed) }

    /// `static_fields_clear()` 专用：静态槽被清空 ⇒ 已跑过的类型初始化器**必须重跑**。
    /// 递增代际使全部快路标记失效，并把 map 里的终态条目复位成 `NotRun`。
    ///
    /// ⚠️ `Failed` 也复位：槽位既已清空，之前那次失败的结论不再适用于新一代；
    /// 若它仍会失败，重跑时会再次失败并重新登记。
    pub fn reset_for_rerun(&self) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        let mut revived = 0usize;
        for e in m.values_mut() {
            if matches!(e.state, CctorState::Done | CctorState::Failed(_)) {
                e.state = CctorState::NotRun;
                revived += 1;
            }
        }
        self.pending.fetch_add(revived, Ordering::Release);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// 热路径的门。`false` ⇒ 全程序没有任何待初始化的 cctor，屏障可直接跳过。
    #[inline(always)]
    pub fn any_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) != 0
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
    }

    /// 认领一个类型的初始化权。返回：
    /// - `Ok(Some(func))` — 本线程认领成功，**调用方负责跑 `func` 并回调 `finish`**
    /// - `Ok(None)`       — 无需动作（未登记 / 已 Done / 本线程重入）
    /// - `Err(msg)`       — 该类型此前初始化失败，或等待超时
    ///
    /// # 跨线程等待（unify-static-init-into-cctor 7.3）
    ///
    /// 他线程正在跑时**阻塞等待到终态**，对齐 C# 的「每类型恰好一次、其他线程看到的
    /// 一定是初始化完成后的状态」。
    ///
    /// 此前这里直接 `Ok(None)` 放行（注释写着「v1 不引入跨线程等待」），代价是并发首次
    /// 访问可能读到部分初始化状态。改前那条路是安全的——静态字段初始化器住在包级
    /// `__static_init__` 里，由 `init_batch_inflight` 那套静止判定保护；本变更把它们搬进
    /// 类型初始化器之后，那套保护不再覆盖，缺口立刻暴露成真实失败
    /// （cross-zpkg `static_init_concurrent`：两个工作线程同时首次触达，一个读到 Null）。
    ///
    /// ## 为什么不会死锁
    ///
    /// 1. **等待期间不持有 `map` 锁**——每轮都在独立作用域里取锁判定、出作用域即释放。
    /// 2. **同线程重入不等待**（`Running(me)` 直接放行），C# 允许 cctor 递归触发自身。
    /// 3. **有界等待**：超时后返回 `Err` 而不是继续放行。跨线程的循环类型初始化
    ///    （T1 初始化 A 要 B、T2 初始化 B 要 A）在 C# 里同样会死锁；这里把它变成一条
    ///    **会响的错误**而不是挂死，也不是静默读到半成品——两害相权取其轻。
    pub fn claim(&self, class_fq: &str) -> Result<Option<String>, String> {
        let me = std::thread::current().id();
        let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
        let mut spins: u32 = 0;
        loop {
            // 判定与认领在同一把锁里完成；**出作用域即释放**，等待绝不持锁。
            {
                let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
                match m.get_mut(class_fq) {
                    None => return Ok(None),
                    Some(e) => match &e.state {
                        CctorState::Done => return Ok(None),
                        CctorState::Failed(msg) => return Err(msg.clone()),
                        CctorState::Running(tid) if *tid == me => return Ok(None),
                        CctorState::Running(_) => {}   // 他线程在跑 → 落到下面等待
                        CctorState::NotRun => {
                            let f = e.func.clone();
                            e.state = CctorState::Running(me);
                            return Ok(Some(f));
                        }
                    },
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!(
                    "timed out waiting for the type initializer of `{class_fq}` to finish on \
                     another thread (possible circular type initialization across threads)"
                ));
            }
            // 前若干轮纯 yield（初始化器通常很短），之后退避到 sleep 免得空转烧 CPU。
            spins = spins.saturating_add(1);
            if spins < 64 {
                std::thread::yield_now();
            } else {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
        }
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

impl crate::vm_context::VmContext {
    /// 登记一个类型的静态构造器（幂等；没有 cctor 的类型直接返回）。
    ///
    /// **必须早于该类型被使用**——`pending` 门只在「已登记」的前提下才有意义：
    /// 若等到屏障里才登记，门会在首次访问时读到 0 而直接放行，屏障形同虚设。
    /// 故登记点有两处，都是「类型进入可见范围」的那一刻，合起来覆盖所有可达类型：
    ///   - 急切：`app.rs` 在模块合并后扫一遍 registry（本函数）
    ///   - 惰性：`LazyLoader::insert_type`——加载进来的类型**入表即登记**
    ///
    /// fix-crosspkg-static-call-cctor：惰性这一处以前挂在 `try_lookup_type` 上。跨包静态方法
    /// 调用只查函数、从不查类型 ⇒ 类型没登记 ⇒ 门读到 0 ⇒ 静态 ctor（连同注入其体首的静态字段
    /// 初始化器）不执行，且结果随「之前有没有别的类型被查过」而变。
    pub fn register_cctor_of(&self, td: &crate::metadata::TypeDesc) {
        if let Some(f) = td.cctor_func() {
            tracing::debug!("cctor-register: type `{}` -> `{}`", td.name, f);
            self.core.cctors.register(&td.name, f);
        }
    }

    /// 全程序是否还有未初始化的静态构造器。**热路径的门**：`false` ⇒ 屏障可直接跳过。
    #[inline(always)]
    pub fn any_cctor_pending(&self) -> bool { self.core.cctors.any_pending() }

    /// **静态字段访问的 cctor 屏障**（interp 与 JIT **共用同一实现**）。
    ///
    /// 两个后端共用一份，是因为「两后端语义一致」正是这个特性最容易出错的地方——
    /// 各写一份迟早漂移。JIT 侧此前完全没有屏障，导致默认模式下静态构造器根本不跑
    /// （`__static_init__` 被强制走解释器，而用户代码走 JIT，两条路走的不是同一个
    /// StaticGet 实现），实测才发现。
    ///
    /// 热路径代价 = 一次 relaxed load：`any_cctor_pending()` 在「程序里没有静态构造器」
    /// 和「所有静态构造器都已跑完」两种情况下都为假，也就是绝大多数时间。
    pub fn ensure_static_owner_init(&self, field: &str) -> Result<(), String> {
        if !self.core.cctors.any_pending() { return Ok(()); }
        let Some(owner) = owner_class_of_static_field(field) else { return Ok(()) };
        // 先查主模块 registry、再回落惰性加载器：`try_lookup_type` 只问惰性加载器，
        // 主合并模块里的类型不在它的索引里（同 ensure_type_init 里函数查找那条注释）。
        if let Some(m) = self.module() {
            if let Some(td) = m.type_registry.get(owner) {
                return self.ensure_type_init(td);
            }
        }
        match self.try_lookup_type(owner) {
            Some(td) => self.ensure_type_init(&td),
            None => Ok(()),
        }
    }

    /// **静态方法调用的 cctor 屏障**（interp 与 JIT 共用）。
    ///
    /// C# 把「调用该类型的静态方法」也算首次使用。实例方法走 VCall 不经这里；即便
    /// 去虚化后走 Call 也无害——能拿到实例就说明类型已初始化过。
    ///
    /// 调 cctor 自身时会命中「本线程重入」分支而放行，不会递归。
    pub fn ensure_callee_owner_init(&self, func_fq: &str) -> Result<(), String> {
        if !self.core.cctors.any_pending() { return Ok(()); }
        let Some(owner) = owner_class_of_static_func(func_fq) else { return Ok(()) };
        if let Some(m) = self.module() {
            if let Some(td) = m.type_registry.get(owner) {
                return self.ensure_type_init(td);
            }
        }
        match self.try_lookup_type(owner) {
            Some(td) => self.ensure_type_init(&td),
            None => Ok(()),
        }
    }

    /// **cctor 屏障**：确保 `td` 这个类型的静态构造器已经跑过（C# 的「首次使用前」）。
    ///
    /// 调用点必须是「首次使用该类型」的地方：创建实例 / 读写其静态字段 / 调其静态方法。
    ///
    /// # 为什么绝大多数类型零代价
    ///
    /// 第一行 `td.cctor_func()` 对没有静态构造器的类型直接返回 `None`（冷区多半是
    /// `None`，访问器一个 `Option` 判断就结束）→ 立即返回。**方案 B 的兑现点就是这一行。**
    ///
    /// # 登记为什么可以惰性做
    ///
    /// 屏障手上已经有 `TypeDesc`，而 `register` 是幂等的 —— 于是「在哪登记」这个
    /// 难题自解：不必在每个 type-registry 插入点（急切一处 + 跨包惰性两处）各挂钩子，
    /// 首次触达时登记即可。少一处要维护的枚举，就少一处将来会漏的地方。
    pub fn ensure_type_init(&self, td: &crate::metadata::TypeDesc) -> Result<(), String> {
        let Some(func) = td.cctor_func() else { return Ok(()) };
        // unify-static-init-into-cctor（7.3）：无锁快路。`register` + `claim` 各要一次
        // mutex + 一次哈希查表，而「该类型早就初始化完了」是绝对多数情形。
        // 快路只信 `true`（已确知跑完）方向，`false` 一律走慢路复核 —— 与 `pending`
        // 门「只信 == 0 方向」是同一套手法，但**粒度是类型而非全程序**，因此不受
        // 「有类型从未被使用 ⇒ 全局门永远开着」的拖累。
        let gen = self.core.cctors.generation();
        if td.init_done_in(gen) { return Ok(()); }
        let class_fq = td.name.clone();
        self.core.cctors.register(&class_fq, func);

        let claimed = match self.core.cctors.claim(&class_fq) {
            Ok(None) => return Ok(()),           // 已完成 / 本线程重入 / 他线程在跑
            Ok(Some(f)) => f,
            // C# 语义：cctor 抛过异常的类型此后不可用，**每次**访问都失败且不重试。
            Err(prior) => return Err(format!(
                "the type initializer for `{class_fq}` threw an exception: {prior}"
            )),
        };

        tracing::debug!("running static ctor `{claimed}` for type `{class_fq}`");
        // 跑 cctor。
        //
        // ⚠️ 查找必须**先走主模块的 func_index、再回落惰性加载器**。
        // `try_lookup_function` 只问惰性加载器，而主合并模块里的普通函数**不在**它的索引里
        // （`__static_init__` 之所以能被它找到，是因为那些名字会被显式压进 pending 队列）。
        // 只用 try_lookup_function 的话，同模块的静态构造器会「明明存在却 not found」——
        // 实测：`C.C$0` 当 entry 能正常跑，屏障里却查不到。
        let run = |f: &crate::metadata::bytecode::Function,
                   module: &crate::metadata::Module| -> Option<String> {
            match crate::interp::exec_function(self, module, f, &[]) {
                Ok(crate::interp::ExecOutcome::Returned(_)) => None,
                // 取异常对象的 Message 字段；非异常值（裸抛）才回落到值的文本形式。
                // 直接 value_to_str 会得到 `Std.Exception{...}` 这种对象转储，不是消息。
                Ok(crate::interp::ExecOutcome::Thrown(v)) => Some(
                    crate::exception::read_message(&v, module)
                        .unwrap_or_else(|| crate::interp::value_to_str(&v))),
                Err(e) => Some(format!("{e:#}")),
            }
        };
        let err = match self.module() {
            Some(module) => match module.func_index.get(claimed.as_str()).copied() {
                Some(i) => run(&module.functions[i], module),
                None => match self.try_lookup_function(&claimed) {
                    Some(f) => run(f.as_ref(), module),
                    // 哨兵载荷指向一个不存在的函数 = 编译期与运行期失配，必须响，别静默放过。
                    None => Some(format!("static ctor `{claimed}` not found")),
                },
            },
            None => Some("no module installed".to_string()),
        };

        self.core.cctors.finish(&class_fq, err.clone());
        // unify-static-init-into-cctor（7.3）：**只有成功才打快路标记**。失败的类型
        // 必须每次都回到慢路，才能命中 `claim` 的 `Failed` 分支重新抛出（C# 语义：
        // 失败是终态、后续每次访问都抛、永不重试）。置位了就再也抛不出来了。
        if err.is_none() { td.mark_init_done(gen); }
        match err {
            None => Ok(()),
            Some(msg) => Err(format!(
                "the type initializer for `{class_fq}` threw an exception: {msg}"
            )),
        }
    }
}

/// 构造 C# 语义的 `Std.TypeInitializationException`。
///
/// 为什么要类型化而不是抛裸字符串：裸 `Value::Str` 只能被**无类型** `catch {}` 捕获，
/// 永远匹配不上 `catch (TypeInitializationException e)` 甚至 `catch (Exception e)`。
/// 逐级回落（TypeInitializationException → Exception → 裸串）保证 stdlib 缺任一类时
/// 仍能把错误传出去，而不是静默吞掉。
pub fn make_type_init_exception(
    vm: &crate::vm_context::VmContext,
    module: &crate::metadata::Module,
    msg: &str,
) -> crate::metadata::Value {
    if let Ok(e) = crate::exception::make_stdlib_exception(
        vm, module, "Std.TypeInitializationException", msg.to_string()) {
        return e;
    }
    if let Ok(e) = crate::exception::make_stdlib_exception(
        vm, module, "Std.Exception", msg.to_string()) {
        return e;
    }
    crate::metadata::Value::Str(msg.into())
}

/// 从静态字段的全限定名取所属类 FQ：`A.B.C.Field` → `A.B.C`。
/// 无点 → `None`（全局静态字段，不属于任何类）。
pub fn owner_class_of_static_field(field_fq: &str) -> Option<&str> {
    field_fq.rfind('.').map(|i| &field_fq[..i])
}

/// 从静态方法的全限定名取所属类 FQ。与静态字段同构，但方法名可能带 `$N` mangle，
/// 那部分在最后一段里，不影响按最后一个 `.` 切分。
pub fn owner_class_of_static_func(func_fq: &str) -> Option<&str> {
    func_fq.rfind('.').map(|i| &func_fq[..i])
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn owner_class_split() {
        assert_eq!(owner_class_of_static_field("A.B.C.Field"), Some("A.B.C"));
        assert_eq!(owner_class_of_static_field("Field"), None);
        assert_eq!(owner_class_of_static_func("A.C.M$2"), Some("A.C"));
    }
}
