# 提案：修复跨 zpkg TypeId 撞键导致的静默错发

- **类型**：`vm`（VM 执行语义 —— 虚派发 / 字段访问的目标选择）
- **状态**：DRAFT，待 User 确认
- **分支 / worktree**：`fix-crosspkg-typeid-collision` / `../z42-assert`

---

## 一句话

**`TypeDesc.id`（`TypeId`）只在单个 zpkg 内唯一，但 `VCallIC` / `FieldIC` 两个内联缓存
把它当全局类型身份用** —— 两个不同 zpkg 的类拿到同一个 id 时，同一调用点会把 A 类的
receiver 派发到 B 类的方法（或读到 B 类的字段槽）。**已实测复现，非推测。**

---

## 现场：从 z42.core 删一个类就让自举链崩

删掉 `Std.SkipSignal` 这一个类（zero 调用点变化，`Assert.Skip` 改抛 `Exception`），
`xtask build stdlib` 从 z42.project 起连锁全红：

```
Error: uncaught exception: Std.Exception: __file_read_text: arg 0 expected string,
  got Object(... "Z42.Syntax.CompilationUnit" ...)
  at Z42.Semantics.ParallelFor.Run$3$i32$i32$IParallelBody (ParallelFor.z42:62:13)
  at Z42.Semantics.IrDump.BuildPackageCus$9$... (IrDump.z42:229:13)
```

`ParallelFor.z42:62` 是 `body.Run(si)` —— 一个 `IParallelBody` 接口调用。仓里有两个实现：

| 类 | 所在 zpkg | 字段槽 0 | `Run(int i)` 首行 |
|---|---|---|---|
| `Z42.Semantics.CompileCuTask` | z42c.semantics | `CompilationUnit[] _cus` | `_compileCu(this._cus[i], …)` |
| `Z42.Driver.SrcReadHashTask` | z42c.driver | `string[] _srcs` | `File.ReadAllText(this._srcs[i])` |

receiver 是 `CompileCuTask`，却跑了 `SrcReadHashTask.Run` → `this._srcs[i]` 读到槽 0 的
`_cus[i]` = `CompilationUnit` → `File.ReadAllText(CompilationUnit)`。
（`File.ReadAllText` 是 `[Native]` extern、不压栈帧，所以栈里看不到那一层。）

**探针实证**（临时在 PIC 命中处校验 callee 归属）：

```
PIC-MISMATCH recv=Z42.Semantics.CompileCuTask recv_id=139
             method=Run -> callee=Z42.Driver.SrcReadHashTask.Run
```

`vcall_ic_hit` 只在 `entry.type_id == receiver_type_id` 时才返回缓存 ⇒ 这行日志本身就
证明**两个类的 TypeId 都是 139**。

### 三道对照（都跑过，见 tasks.md 的复跑配方）

| 实验 | 结果 |
|---|---|
| origin/main `54c8a1df` 原样 | ✅ 25/25 |
| 只删 `Assert.Skip` **方法**、`SkipSignal` 类保留 | ✅ 绿 ⇒ 与方法集合无关 |
| 只删 `SkipSignal` **类**、方法保留 | ❌ 红 ⇒ 根因是**类集合** |
| 未改动 main + 同一探针 VM | **0 条 mismatch、25/25 绿** ⇒ latent-but-live |

最后一行是关键：今天 main 是**碰巧**没撞上，删一个类就足以把 id 对齐。

---

## 根因

```
loader/type_registry.rs:23   let mut next_type_id: u32 = 0;    // ← 每个 Module 都从 0 开始
loader/type_registry.rs:66   let type_id = TypeId(next_type_id); next_type_id += 1;
```

- `TypeId` 的文档契约就是**「per module」**（`tokens.rs`：*Identifies one ClassDef /
  TypeDesc in `Module.classes` (per module)*）。这一点本身没错。
- 会重新发号、让 id 在消费模块内唯一的 `Module::register_lazy_type`
  （`bytecode/module.rs:69`）**生产代码里从不调用** —— 全仓只有 `loader_tests.rs` 调它。
  跨 zpkg 的 `TypeDesc` 由 `vm_context/lookup.rs:181 try_lookup_type` **原样返回**，
  保留外来模块的 local id。
- 而两个内联缓存把这个 per-module id 当**全局**身份：

| 缓存 | key | 撞键后果 |
|---|---|---|
| `VCallIC`（`interp/vcall_resolve.rs:64,169` + `jit/helpers/vcall.rs:51`） | `TypeDesc.id.0` | **调用错方法**（本次现场） |
| `FieldIC`（`interp/exec_object.rs:193,213,307,331` + `jit/helpers/object_field.rs:125,149,218,243`） | `TypeDesc.id.0` | **读写错字段槽 —— 静默数据损坏，不崩不报错** |

⇒ 这是**一个根因、两处受害**，必须一起修。

> 同仓已有正确做法的先例：`interp/dispatch.rs::isa_td` 的 `isa_cache` 用
> `td as *const TypeDesc`（指针）作 key，正因为它不敢信 id。

### 为什么至今没炸

多数 VCall / FieldGet 站点是**单态**的（一个站点只见过一种 receiver 类型），撞键要求
「同一个站点先后见到两个 id 相同、来自不同 zpkg 的类」。`ParallelFor.z42:62` 恰好是
仓里少见的**跨 zpkg 多态点**。这是运气，不是设计保证。

---

## 范围（Scope）

**In**

1. `TypeId` 改为**进程内全局唯一**发号，使 PIC 的 u32 key 成为真正的全局类型身份。
2. 删掉与新不变量矛盾的**死设施**：`Module::type_registry_vec` / `Module::type_by_id` /
   `Module::register_lazy_type` 及其单测（生产零消费者，留着就是「id 是本模块稠密下标」
   这个错误心智模型的来源，也是下一个人踩同一坑的入口）。
3. 把临时探针转正为**常驻断言门**：debug/测试构建下 PIC 命中时校验 callee 归属，
   撞键立刻 panic 而不是静默错发。
4. `src/tests/multi-exe/` 加一个**跨 zpkg 接口多态**回归 fixture（先在未修复 VM 上确认必红）。
5. 文档同步：`docs/design/runtime/vm-architecture.md` 的 TypeId 段 + `docs/book/` 对应机制页。

**Out（本 change 不做）**

- 不改 zbc / zpkg 格式（`TypeId` 是 load-time 概念，`#[serde(skip)]`，磁盘上不存在）。
- 不动 PIC 的槽数 / 淘汰策略 / JIT codegen 签名（key 仍是 u32）。
- 不碰编译器侧的任何类型身份逻辑（这次的坑纯在 runtime）。

---

## 为什么不选另外两条路

| 方案 | 否决理由 |
|---|---|
| **PIC key 换成 `Arc<TypeDesc>` 指针**（照抄 `isa_cache`） | 语义上最干净，但 IC entry 要从 `AtomicU32` 加宽到 `AtomicU64`，JIT codegen 里烤进机器码的 IC 布局跟着改 —— 为同一个正确性收益付大得多的代价 |
| **在 `try_lookup_type` / `obj_new` 里调 `register_lazy_type` 重新发号** | 恢复了「id 在消费模块内唯一」的旧不变量，但 `TypeDesc` 是跨模块共享的 `Arc`，重新发号意味着同一个类在不同模块里 id 不同；而 PIC 分散在多个模块里 —— **修不干净** |

选定方案（全局发号）之所以便宜，是因为一个已核实的事实：**「id 必须是本模块 0..N 稠密
下标」这条不变量没有任何生产消费者**（`type_by_id` 只在 `loader_tests.rs` 被调用）。

---

## 风险

| 风险 | 评估 |
|---|---|
| 产物字节漂移 | **无**。TypeId 是 load-time 的，`#[serde(skip)]`，不进 zbc/zpkg；已 grep 确认 `src/tests/` 无 golden 捕获 `TypeId(` |
| 热路径变慢 | **无**。发号从 `next_type_id += 1` 变成一次 `fetch_add`，只发生在 load 期（每类一次）；PIC 查找一个字都不改 |
| u32 号段耗尽 | 号段 `[0, 0x7FFF_FFFE]`（~2.1B），高位 `IMPORT_BASE` / `PRIM_TYPE_*` / `UNRESOLVED` 不受影响。REPL 反复 reload 也够用 |
| REPL 模块重载后 PIC 残留 | **变更好**：旧 id 不再被重新分配 ⇒ 陈旧 PIC 条目只会 miss（回退慢路径），不会误命中 |
| 单进程多 VM | 共用一个计数器 ⇒ 跨 VM 也唯一，比现状更强，无副作用 |

---

## 关联

- 现场发现路径：`unify-assert-api`（PR #532，已合）之后「按语义把 `SkipSignal` 归位到
  z42.test」撞到的墙
- 同族「类型身份撞键」先例：`fix-type-ref-ns-collision`（PR #353，根因在编译器
  `SymbolTable` 按裸类名 first-wins）—— 那次是**编译期**认错类型，这次是**运行期**认错类型
- `.claude/rules/common-pitfalls.md` §1：本条同属「拿一个不保证唯一/稳定的键当全局身份用」家族
