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
use std::sync::atomic::{AtomicUsize, Ordering};

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
#[derive(Debug, Default)]
pub struct CctorRegistry {
    /// 类 FQ → 登记项。
    map: Mutex<FxHashMap<String, CctorEntry>>,
    /// 还没到达终态（Done / Failed）的类型数。**热路径唯一要读的东西**。
    ///
    /// 只在 `== 0` 方向被信任（「可证无事可做」）；非 0 时一律走慢路复核，
    /// 故不存在「读到过期非 0 值」导致的正确性问题——只会多做一次查表。
    pending: AtomicUsize,
}

impl CctorRegistry {
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
    /// - `Err(msg)`       — 该类型此前初始化失败（C# 语义：后续访问抛包装异常）
    ///
    /// ⚠️ 他线程正在跑时**返回 `Ok(None)` 而不是阻塞等待**：v1 不引入跨线程等待，
    /// 因为在持有解释器帧的情况下阻塞等待极易与既有的静态初始化排空逻辑互相死锁
    /// （那块已有 `DRAINING` / `init_batch_inflight` 一堆防嵌套机制）。代价是并发首次
    /// 访问可能看到部分初始化状态——与 C# 的强保证有差距，**必须在文档与测试里写明**。
    pub fn claim(&self, class_fq: &str) -> Result<Option<String>, String> {
        let me = std::thread::current().id();
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        match m.get_mut(class_fq) {
            None => Ok(None),
            Some(e) => match &e.state {
                CctorState::Done => Ok(None),
                CctorState::Failed(msg) => Err(msg.clone()),
                CctorState::Running(tid) if *tid == me => Ok(None), // 重入：C# 直接放行
                CctorState::Running(_) => Ok(None),                 // 他线程在跑，见上注
                CctorState::NotRun => {
                    let f = e.func.clone();
                    e.state = CctorState::Running(me);
                    Ok(Some(f))
                }
            },
        }
    }

    /// 认领方在 cctor 跑完后回调：`err = None` 成功，`Some(msg)` 失败。
    /// 两种终态都让 `pending` 减一——失败的类型不会被重试（对标 C#）。
    pub fn finish(&self, class_fq: &str, err: Option<String>) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = m.get_mut(class_fq) {
            let was_terminal = matches!(e.state, CctorState::Done | CctorState::Failed(_));
            e.state = match err {
                None => CctorState::Done,
                Some(msg) => CctorState::Failed(msg),
            };
            if !was_terminal {
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
        assert!(!r.any_pending(), "失败也是终态，计数要减");
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
