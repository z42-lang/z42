//! Type initialization barriers, execution, parked waits and exception construction.
use std::sync::Arc;
use crate::vm::runtime::statics::{CctorRegistry, CctorState,
    owner_class_of_static_field, owner_class_of_static_func};

const WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Claim or wait without GC participation; only non-VM test callers use this helper.
#[cfg(test)]
pub fn claim(registry: &CctorRegistry, class_fq: &str) -> Result<Option<String>, String> {
    claim_with(registry, class_fq, || ())
}

/// Claim under the original map lock; waiters park and let the Condvar release that lock.
/// The clock is read only when another thread is running the initializer (never on wasm's fast path).
/// Release the map guard before dropping the park guard, which may wait for STW to finish.
pub fn claim_with<G>(registry: &CctorRegistry, class_fq: &str, park: impl Fn() -> G) -> Result<Option<String>, String> {
    let me = std::thread::current().id();
    // 截止时间**到真要等待时才取时钟**（fix-device-tests）：此前在入口就
    // `Instant::now()`，每次认领都读一次时钟——而 `wasm32-unknown-unknown` 上它直接 panic
    // （"time not implemented on this platform"）。本函数 09-22 引入后，wasm 上第一个带
    // 类型初始化器的类型就崩，nightly 的 test-wasm-browser 自那天起全红。只有「他线程
    // 正在跑」才需要等待，而单线程的 wasm 永远走不到那一支。
    let mut deadline: Option<std::time::Instant> = None;
    loop {
        // 判定与认领在同一把锁里完成；等待时由 `done_cv` 释放这把锁。
        let mut m = registry.map.lock().unwrap_or_else(|e| e.into_inner());
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
        let now = std::time::Instant::now();
        let deadline = *deadline.get_or_insert(now + WAIT_TIMEOUT);
        if now >= deadline {
            return Err(format!(
                "timed out waiting for the type initializer of `{class_fq}` to finish on \
                 another thread (possible circular type initialization across threads)"
            ));
        }
        let parked = park();
        let (m, _) = registry.done_cv
            .wait_timeout(m, deadline - now)
            .unwrap_or_else(|e| e.into_inner());
        drop(m);
        drop(parked);
    }
}

impl crate::vm::context::VmContext {
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
    pub fn register_cctor_of(&self, td: &crate::object::TypeDesc) {
        if let Some(f) = td.cctor_func() {
            tracing::debug!("cctor-register: type `{}` -> `{}`", td.name, f);
            self.core.runtime.cctors.register(&td.name, f);
        }
    }

    /// 全程序是否还有未初始化的静态构造器。**热路径的门**：`false` ⇒ 屏障可直接跳过。
    #[inline(always)]
    pub fn any_cctor_pending(&self) -> bool { self.core.runtime.cctors.any_pending() }

    /// 包级初始化屏障的门：`false` ⇒ 没有待跑的包初始化器、也没有失败终态的包，
    /// [`Self::ensure_module_inits`] 必然 `Ok`，调用方可整个跳过（JIT `jit_call` 的热路径）。
    #[inline(always)]
    pub fn module_init_gate_open(&self) -> bool {
        let cc = &self.core.runtime.cctors;
        cc.any_module_init_pending() || cc.any_module_init_failed()
    }

    /// 编译期判定：静态调用 `func_fq` 的类型初始化屏障（[`Self::ensure_callee_owner_init`]）恒为空操作——
    /// 属主类型已加载且没有静态构造器。属主未加载 / 自由函数一律答 `false`（保守）。
    #[cfg(feature = "jit")]
    pub fn callee_owner_init_free(&self, func_fq: &str) -> bool {
        owner_class_of_static_func(func_fq)
            .and_then(|o| self.loaded_type(o))
            .is_some_and(|td| td.cctor_func().is_none())
    }

    /// **静态字段访问的 cctor 屏障**（interp 与 JIT **共用同一实现**）：静态字段屏障
    /// （`VmContext::ensure_static_field_init`）在单元里没有当前纪元的放行记录时调它。
    ///
    /// 两个后端共用一份，是因为「两后端语义一致」正是这个特性最容易出错的地方——
    /// 各写一份迟早漂移。JIT 侧此前完全没有屏障，导致默认模式下静态构造器根本不跑
    /// （`__static_init__` 被强制走解释器，而用户代码走 JIT，两条路走的不是同一个
    /// StaticGet 实现），实测才发现。
    ///
    /// 代价 = 一次 relaxed load：`any_cctor_pending()` 在「程序里没有静态构造器」
    /// 和「所有静态构造器都已跑完」两种情况下都为假。
    ///
    /// `field_id`（已解析的静态字段槽号）可用时，属主判定缓存在该字段的静态单元里
    /// （`static_table::StaticCell::owner`，整个 VM 共享、读无锁）：第一次照旧按名推属主（可能加载
    /// 属主所在的包——跨包读静态字段时它确实可能还没加载），之后一次 acquire load 命中；无 cctor 的
    /// 属主记为免检，有 cctor 的交给 `ensure_type_init`（已初始化的类型在那里两次 relaxed load 返回）。
    #[inline]
    pub fn ensure_static_owner_init(&self, field: &str, field_id: Option<u32>) -> Result<(), String> {
        use crate::vm::runtime::statics::StaticOwner;
        use crate::vm::runtime::ids::tokens::StaticFieldId;
        if !self.core.runtime.cctors.any_pending() { return Ok(()); }
        if let Some(cell) = field_id.and_then(|id| self.core.runtime.statics.cell(StaticFieldId(id))) {
            match cell.owner() {
                StaticOwner::Exempt => return Ok(()),
                StaticOwner::Check(td) => return self.ensure_type_init(td),
                StaticOwner::Unknown => {}
            }
        }
        self.resolve_static_owner_init(field, field_id)
    }

    /// The owner answer is not cached yet: derive it from the name, cache it, apply it.
    #[inline(never)]
    fn resolve_static_owner_init(&self, field: &str, field_id: Option<u32>) -> Result<(), String> {
        use crate::vm::runtime::ids::tokens::StaticFieldId;
        let owner = self.static_field_owner(field);
        if let Some(id) = field_id {
            // 基类链未合并的类型不缓存（加载器要 `Arc::get_mut` 原地补布局）。
            if !owner.as_ref().is_some_and(|td| td.base_unmerged()) {
                self.core.runtime.statics.set_owner(StaticFieldId(id), owner.clone());
            }
        }
        match owner {
            Some(td) => self.ensure_type_init(&td),
            None => Ok(()),
        }
    }

    /// 静态字段 `field` 的属主类型——只在它有静态构造器时返回。先查主模块 registry、
    /// 再回落惰性加载器：`try_lookup_type` 只问惰性加载器，主合并模块里的类型不在它的
    /// 索引里（同 ensure_type_init 里函数查找那条注释）。
    fn static_field_owner(&self, field: &str) -> Option<Arc<crate::object::TypeDesc>> {
        let owner = owner_class_of_static_field(field)?;
        let td = match self.module().and_then(|m| m.type_registry.get(owner)) {
            Some(td) => Arc::clone(td),
            None => self.try_lookup_type(owner)?,
        };
        td.cctor_func().is_some().then_some(td)
    }

    /// **静态方法调用的 cctor 屏障**（interp 与 JIT 共用）。
    ///
    /// C# 把「调用该类型的静态方法」也算首次使用。实例方法走 VCall 不经这里；即便
    /// 去虚化后走 Call 也无害——能拿到实例就说明类型已初始化过。
    ///
    /// 调 cctor 自身时会命中「本线程重入」分支而放行，不会递归。
    /// **包级初始化屏障**（add-module-init-hook）：把已加载但还没初始化的包跑掉。
    ///
    /// 调用点是**能抛异常的**那些屏障（静态调用 / `new` / 静态字段访问），不是加载收口点 ——
    /// 加载收口点（`try_lookup_*`）返回 `Option`，抛不出可 catch 的异常。分工因此是
    /// 「加载漏斗只登记，屏障点执行」：登记发生在解析该包符号的那一刻，执行发生在**同一次
    /// 调用内、被调代码执行之前** ⇒ 对用户仍然是「包加载后、本包任何代码跑之前」。
    ///
    /// 稳态代价：两次 relaxed load（`any_module_init_pending` + `any_module_init_failed`）。
    ///
    /// # 失败不吞、不重试 —— 但只对**触达该包**的那些屏障
    ///
    /// `sym_fq` = 这次屏障正要触达的符号（被调函数 / 被 new 的类型 / 静态字段的 FQ 名）。
    /// 它是 fix-module-init-failure-scope 的全部要点：
    ///
    /// - **还没跑的**初始化器，任何屏障点都把它们跑掉（「加载即执行」，与 `module_init_load_order`
    ///   钉住的语义一致）；这一轮跑出来的失败就在**当前这次**屏障抛出。
    /// - **已经失败的**包是终态，不再重跑；只有 `sym_fq` 归属于它时才重抛
    ///   （[`CctorRegistry::failed_module_owning`]）。
    ///
    /// 此前没有 `sym_fq`，失败包让门永远开着 ⇒ **全程序每一次调用**都重抛它，
    /// 包括用户 `catch` 块里的第一条 `Console.WriteLine`：异常看起来「抓不到」，
    /// 实际是抓到了、然后在 handler 里被一条无关调用重新抛出。
    /// 文档（`docs/reference/src/language/module-initializers.md`）一直写的是
    /// 「之后每一次**触达**都会再抛一次」——本实现现在才与之相符。
    #[inline(always)]
    pub fn ensure_module_inits(&self, sym_fq: Option<&str>) -> Result<(), String> {
        let cc = &self.core.runtime.cctors;
        // 稳态：两次 relaxed load 就返回，不进下面那个大函数（它的帧建立比这两次 load 贵）。
        if !cc.any_module_init_pending() && !cc.any_module_init_failed() {
            return Ok(());
        }
        self.ensure_module_inits_slow(sym_fq)
    }

    #[inline(never)]
    fn ensure_module_inits_slow(&self, sym_fq: Option<&str>) -> Result<(), String> {
        let cc = &self.core.runtime.cctors;
        if cc.any_module_init_pending() {
            let list = cc.module_init_snapshot();
            let mut first_err: Option<String> = None;
            for td in &list {
                // 失败终态不重跑：`ensure_type_init` 对它只会原样返回那条 Err，
                // 在这里收下就又变成「与被调方无关也抛」。交给下面的归属判定。
                if cc.is_module_init_failed(&td.name) { continue; }
                if let Err(e) = self.ensure_type_init(td) {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
            cc.refresh_module_pending();
            if let Some(e) = first_err { return Err(e); }
        }
        if let Some(sym) = sym_fq {
            if let Some(td) = cc.failed_module_owning(sym) {
                return self.ensure_type_init(&td);
            }
        }
        Ok(())
    }

    /// Will the callee's call barrier (module init + owner cctor, as
    /// `call.rs::call_compiled` runs it) keep passing, on every thread, until
    /// the barrier epoch moves? The same shape as `statics.rs::
    /// static_barrier_settled`: no module initializer left to run, and either
    /// no static constructor is pending at all, or this callee's owner is
    /// resolved and its initializer is done in the current generation. `false`
    /// (including an unresolved owner cell) simply keeps the caller on the
    /// full barrier — a settle record is only ever an optimization.
    pub fn callee_barrier_settled(
        &self, owner: &crate::vm::runtime::OwnerInitCell,
    ) -> bool {
        let cc = &self.core.runtime.cctors;
        if cc.any_module_init_pending() { return false; }
        if !cc.any_pending() { return true; }
        match owner.get() {
            Some(Some(td)) => td.init_done_in(cc.generation()),
            // Owner resolved and has no static constructor: exempt for good.
            Some(None) => true,
            // Unresolved: stay on the full barrier (it resolves the cell).
            None => false,
        }
    }

    ///
    /// `owner` is the callee's [`Function::owner_init`](crate::vm::runtime::Function::owner_init)
    /// cell. After the first call it holds the answer, so the steady-state cost
    /// is one `OnceLock` read — and nothing at all for free functions and
    /// methods of types without a static constructor, which is almost every call.
    #[inline]
    pub fn ensure_callee_owner_init(
        &self, func_fq: &str, owner: &crate::vm::runtime::OwnerInitCell,
    ) -> Result<(), String> {
        if !self.core.runtime.cctors.any_pending() { return Ok(()); }
        // Owner resolved and without a static constructor — almost every call.
        if let Some(None) = owner.get() { return Ok(()); }
        self.ensure_callee_owner_init_slow(func_fq, owner)
    }

    #[inline(never)]
    fn ensure_callee_owner_init_slow(
        &self, func_fq: &str, owner: &crate::vm::runtime::OwnerInitCell,
    ) -> Result<(), String> {
        if let Some(cached) = owner.get() {
            return match cached {
                Some(td) => self.ensure_type_init(td),
                None => Ok(()),
            };
        }
        let found = owner_class_of_static_func(func_fq)
            .and_then(|o| self.loaded_type(o))
            .filter(|td| td.cctor_func().is_some());
        match found {
            // The loader's inheritance fixup mutates a type through
            // `Arc::get_mut` until its base chain is merged; a cached clone
            // would block it. Run the barrier without caching this time.
            Some(td) if td.base_unmerged() => self.ensure_type_init(&td),
            found => {
                let _ = owner.set(found);
                match owner.get() {
                    Some(Some(td)) => self.ensure_type_init(td),
                    _ => Ok(()),
                }
            }
        }
    }

    /// An already-registered type from the entry module or a loaded package.
    /// Never loads: a resolved callee's package already registered its types, and
    /// a free function's apparent owner may be a namespace rather than a type.
    fn loaded_type(&self, name: &str) -> Option<Arc<crate::object::TypeDesc>> {
        if let Some(td) = self.module().and_then(|m| m.type_registry.get(name)) {
            return Some(Arc::clone(td));
        }
        self.core.loader.lazy_loader.read().as_ref().and_then(|l| l.loaded_type(name))
    }

    /// Ensure a type's initializer has completed before its first use.
    /// The slow-path registration is idempotent; visibility-time registration is
    /// still required to arm the global pending gate before any caller checks it.
    pub fn ensure_type_init(&self, td: &crate::object::TypeDesc) -> Result<(), String> {
        let Some(func) = td.cctor_func() else { return Ok(()) };
        // unify-static-init-into-cctor（7.3）：无锁快路。`register` + `claim` 各要一次
        // mutex + 一次哈希查表，而「该类型早就初始化完了」是绝对多数情形。
        // 快路只信 `true`（已确知跑完）方向，`false` 一律走慢路复核 —— 与 `pending`
        // 门「只信 == 0 方向」是同一套手法，但**粒度是类型而非全程序**，因此不受
        // 「有类型从未被使用 ⇒ 全局门永远开着」的拖累。
        let gen = self.core.runtime.cctors.generation();
        if td.init_done_in(gen) { return Ok(()); }
        let class_fq = td.name.clone();
        self.core.runtime.cctors.register(&class_fq, func);

        let claimed = match claim_with(&self.core.runtime.cctors, &class_fq, || crate::vm::threads::safepoint::NativeParkGuard::enter(self))
        {
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
        let run = |f: &crate::vm::runtime::Function,
                   module: &crate::vm::runtime::Module| -> Option<String> {
            match crate::vm::exec::engine::invoke(self, module, f, &[]) {
                Ok(crate::vm::exec::call::outcome::ExecOutcome::Returned(_)) => None,
                // 取异常对象的 Message 字段；非异常值（裸抛）才回落到值的文本形式。
                // 直接 value_to_str 会得到 `Std.Exception{...}` 这种对象转储，不是消息。
                Ok(crate::vm::exec::call::outcome::ExecOutcome::Thrown(v)) => Some(
                    crate::vm::exec::exception::read_message(&v, self, module)
                        .unwrap_or_else(|| crate::object::value::format::value_to_str(&v))),
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

        self.core.runtime.cctors.finish(&class_fq, err.clone());
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
    vm: &crate::vm::context::VmContext,
    module: &crate::vm::runtime::Module,
    msg: &str,
) -> crate::object::Value {
    if let Ok(e) = crate::vm::exec::exception::make_stdlib_exception(
        vm, module, "Std.TypeInitializationException", msg.to_string()) {
        return e;
    }
    if let Ok(e) = crate::vm::exec::exception::make_stdlib_exception(
        vm, module, "Std.Exception", msg.to_string()) {
        return e;
    }
    crate::object::Value::Str(msg.into())
}
