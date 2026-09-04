//! 解析路径：函数 / 类型的按需解析（命名空间路由 → 候选包加载 → Fallback-B 到不动点 →
//! 基类链补齐），以及类初始化状态 `InitState`。注册与加载在 `super::registry`。

use super::*;

impl LazyLoader {
    /// Look up a function by FQ name; triggers lazy load if needed.
    pub fn resolve_function(&mut self, func_name: &str) -> Option<Arc<Function>> {
        if let Some(f) = self.function_table.get(func_name) {
            return Some(Arc::clone(f));
        }
        // cache-failed-name-resolution: a name that already lost the full walk
        // under this registry state loses it again — answer from the set instead
        // of re-scanning every declared-but-unloaded zpkg. Dominant case: the
        // synthesized `<Class>..ctor$0` of a **ctor-less class**, looked up once
        // per `new` by both interp and JIT.
        if self.is_known_unresolved_function(func_name) {
            return None;
        }
        // Strategy C: precise routing by namespace prefix
        if let Some(ns) = namespace_prefix(func_name) {
            for zpkg_file in self.candidates_for_namespace(&ns) {
                let _ = self.load_zpkg_file(&zpkg_file);
                if let Some(f) = self.function_table.get(func_name) {
                    return Some(Arc::clone(f));
                }
            }
        }
        // Fallback B: try every remaining declared-but-not-loaded zpkg.
        //
        // defer-class-initialization (2026-09-04): **到不动点**。`declared_zpkgs` 是
        // 增量集合——加载一个包会把它自己的依赖注册成新候选。一轮扫完不等于闭包：
        // xtask 的 `Std.IO.Process` 就出现在「8 个直接依赖都加载完、z42.io 才刚成为候选」
        // 的窗口里，单轮 Fallback-B 会漏掉它并返回 None。变更前这个窗口被 boot 期
        // `force_load_all_declared()` 掩盖（它在任何用户代码前就把闭包铺满了）。
        loop {
            let remaining = self.remaining_declared();
            if remaining.is_empty() { break; }
            let mut progressed = false;
            for zpkg_file in remaining {
                if self.load_zpkg_file(&zpkg_file).is_ok() { progressed = true; }
                if let Some(f) = self.function_table.get(func_name) {
                    return Some(Arc::clone(f));
                }
            }
            // 无进展就停：加载失败的包会一直留在 `remaining_declared()` 里
            // （只有成功才标记 loaded），不设这个闸门会原地死循环。
            if !progressed { break; }
        }
        self.note_unresolved_function(func_name);
        None
    }

    /// defer-class-initialization: 只读探测——该类是否已注册（不触发任何加载）。
    pub(crate) fn has_type(&self, class_name: &str) -> bool {
        self.type_registry.contains_key(class_name)
    }

    /// Look up a class TypeDesc by FQ name; triggers lazy load if needed.
    /// L3-G4d: also triggers the zpkg load for the owning namespace so the
    /// first `new Stack<int>()` on an imported generic class resolves.
    pub fn resolve_type(&mut self, class_name: &str) -> Option<Arc<TypeDesc>> {
        // cache-failed-name-resolution: see `resolve_function`. Misses here are
        // just as hot — `obj_new` probes the class name on every allocation of a
        // type that lives outside the merged module's registry.
        if self.is_known_unresolved_type(class_name) {
            return None;
        }
        let Some(td) = self.resolve_type_raw(class_name) else {
            self.note_unresolved_type(class_name);
            return None;
        };
        // 快路径：基类链已完整 ⇒ `load_zpkg_file` 末尾的 fixup 已经收敛过，直接返回。
        // （不能无条件跑 fixup —— 那是 O(registry) 的扫描，会压在每次跨包类型查找上。）
        if self.base_chain_complete(class_name) {
            return Some(td);
        }
        // defer-class-initialization (2026-09-04): 返回前保证**基类闭包已就位**。
        //
        // `needs_fixup` 对「基类还没加载」的子类返回 false（"base still unresolvable"），
        // 于是一个跨包子类会以**未合并布局**被返回——基类字段没有槽位，构造函数里
        // `base(name)` 的写入被丢弃，读出来全是 null（golden `vcall_base_fallback`：
        // `Rex says: Woof!` 变成 `null says: Woof!`，虚方法派发本身是对的）。
        // 变更前 boot 期 force-load 把所有包铺满，fixup 在任何用户代码前就收敛了。
        // 现在按需加载，必须在这里显式补齐（CLR 同样保证加载一个类型先加载其基类型）。
        self.ensure_base_chain_loaded(class_name);
        self.type_registry.get(class_name).map(Arc::clone)
    }

    /// 基类链上的每一级是否都已在 registry 中（短走，链深级别，无分配）。
    fn base_chain_complete(&self, class_name: &str) -> bool {
        let mut cur = class_name;
        for _ in 0..64 {
            let Some(td) = self.type_registry.get(cur) else { return true };
            let Some(base) = td.base_name.as_deref() else { return true };
            if !self.type_registry.contains_key(base) { return false; }
            cur = base;
        }
        true
    }

    /// 加载 `class_name` 的整条基类链所在的包，然后把继承 fixup 跑到不动点。
    /// 迭代（非递归）向上走，`resolve_type_raw` 只负责加载、不再回调本函数。
    fn ensure_base_chain_loaded(&mut self, class_name: &str) {
        let mut cur = class_name.to_string();
        // 上限 = 类型数 + 8：良构继承链的深度远小于此，超出说明元数据有环，
        // 停下让 fixup 的收敛检查报错，而不是在这里空转。
        for _ in 0..(self.type_registry.len() + 8) {
            let Some(td) = self.type_registry.get(&cur) else { break };
            let Some(base) = td.base_name.clone() else { break };
            if !self.type_registry.contains_key(&base) {
                self.resolve_type_raw(&base);
            }
            if !self.type_registry.contains_key(&base) { break; }
            cur = base;
        }
        let cap = self.type_registry.len() + 8;
        for _ in 0..cap {
            if crate::metadata::loader::try_fixup_inheritance(&mut self.type_registry) == 0 {
                break;
            }
        }
    }

    /// 纯查找 + 按需加载，**不**补基类链（避免与 `ensure_base_chain_loaded` 互递归）。
    fn resolve_type_raw(&mut self, class_name: &str) -> Option<Arc<TypeDesc>> {
        if let Some(td) = self.type_registry.get(class_name) {
            return Some(Arc::clone(td));
        }
        // Strategy C: use the class's enclosing namespace (strip last segment)
        if let Some((ns, _)) = class_name.rsplit_once('.') {
            for zpkg_file in self.candidates_for_namespace(ns) {
                let _ = self.load_zpkg_file(&zpkg_file);
                if let Some(td) = self.type_registry.get(class_name) {
                    return Some(Arc::clone(td));
                }
            }
        }
        // defer-class-initialization: 原生类型关键字名不可能是 zpkg 导出类名。
        if is_primitive_keyword_name(class_name) {
            return None;
        }
        // Fallback B 到不动点——理由同 `resolve_function`。
        loop {
            let remaining = self.remaining_declared();
            if remaining.is_empty() { return None; }
            let mut progressed = false;
            for zpkg_file in remaining {
                if self.load_zpkg_file(&zpkg_file).is_ok() { progressed = true; }
                if let Some(td) = self.type_registry.get(class_name) {
                    return Some(Arc::clone(td));
                }
            }
            if !progressed { return None; }
        }
    }

}

/// defer-class-initialization: 单个 `__static_init__` 的执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InitState {
    /// 正在执行；记录持有线程，用于区分「同线程重入」与「他线程需等待」。
    Running(std::thread::ThreadId),
    /// 已执行完毕。
    Done,
}

/// defer-class-initialization: 编译器原生类型关键字名。这些名字不可能是 zpkg
/// 导出的类名，解析失败时直接返回，不进 Fallback-B 全量扫描（实测：一次
/// `resolve_type("int")` 会把全部未加载包挨个加载一遍）。
pub(super) fn is_primitive_keyword_name(name: &str) -> bool {
    !name.contains('.')
        && matches!(
            name,
            "int" | "long" | "short" | "byte" | "sbyte"
                | "uint" | "ulong" | "ushort"
                | "float" | "double" | "bool" | "char"
                | "string" | "object" | "void"
        )
}

/// Extract the namespace prefix from a fully-qualified function name.
/// E.g. "Std.IO.Console.WriteLine" → Some("Std.IO")
///      "Std.Assert.Equal"         → Some("Std")
///      "main"                     → None (no namespace)
pub(crate) fn namespace_prefix(func_name: &str) -> Option<String> {
    // A qualified function name has the form: <ns>.<Class>.<method>
    //                                         or <ns>.<func>
    // Strategy: strip the last two segments (Class.method), keep the rest.
    let dots: Vec<usize> = func_name.match_indices('.').map(|(i, _)| i).collect();
    if dots.len() < 2 {
        // "Class.method" — no explicit namespace. Use first segment as candidate.
        return dots.first().map(|&i| func_name[..i].to_string());
    }
    Some(func_name[..dots[dots.len() - 2]].to_string())
}

