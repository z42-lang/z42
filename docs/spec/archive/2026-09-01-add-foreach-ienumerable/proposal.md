# Proposal: foreach 端到端支持 IEnumerator 协议 + List/Dictionary 实现 IEnumerable

> 状态：🔵 DRAFT 待审批 | 类型：lang / vm（需规范先行）

## Why

z42 的 `foreach` 当前**只认两种迭代目标**（`StmtBinder._bindForeach`，`src/compiler/z42c.semantics/src/StmtBinder.z42:15-47`；`StmtEmitter._emitForeach`，`src/compiler/z42c.semantics/src/StmtEmitter.z42:272-337`）：

1. **数组 path**（`IsClassIter=false`）：`ArrayLenInstr` 求长 + `ConstI32` 计数器 + `ArrayGetInstr` 取元素（`StmtEmitter.z42:289`、`:309`）。
2. **索引鸭子 path**（`IsClassIter=true`）：类型带 `get_Item(int)` 且带 `Count` 字段或方法（检测在 `StmtBinder.z42:42-45`）→ `Count` field/`VCall` 求长（`StmtEmitter.z42:283/286`）+ `get_Item` `VCall` 取元素（`StmtEmitter.z42:307`）。

**局限**：任何"只实现了 `IEnumerable<T>`、但没有 `Count`+`get_Item` 索引面"的类型（链表、树遍历、集合视图、未来的 LINQ/iterator 链）都**无法** `foreach`——绑定阶段既不是数组也匹配不到索引鸭子协议，落到 `StmtBinder.z42:312` 的 "unsupported statement"（实际是 elem type 推断失败 + codegen 走错分支）。

接口定义其实**早已就位**但从未被 foreach 消费：

- 编译器内建接口面 `BuiltinTypeDefs._builtinInterfaces` 已注册 `IEnumerable<T>.GetEnumerator()`（`src/compiler/z42c.semantics/src/BuiltinTypeDefs.z42:52-54`）、`IEnumerator<T>.MoveNext()/get_Current`（`:55-58`）、`IDisposable.Dispose()`（`:49-51`）。
- 协议 stdlib 定义 `src/libraries/z42.core/src/Protocols/IEnumerable.z42:12-14` 与 `IEnumerator.z42:14-17` 完整（`IEnumerator<T> : IDisposable`），且 `IEnumerator.z42` 文件头明确写着 "**foreach codegen 升级（识别 IEnumerator 路径）是独立后续工作**"——本 change 正是补上这条路径。

本 change 补上**第三条 foreach path**：经 `IEnumerable<T>.GetEnumerator()` → `IEnumerator<T>.MoveNext()/Current/Dispose()` 迭代任意实现了 `IEnumerable` 的类型，并顺带让 `List`/`Dictionary` 显式实现 `IEnumerable`（它们继续走索引快路径，见 Out of Scope 交互说明）。

> **自包含、无前置依赖**：enumerator 实例是 `var __e = coll.GetEnumerator()` 得到的**标量** struct（非数组），跨包实例化正常工作；本 change 不依赖任何前置 layout 修复。

## What Changes

- **foreach 识别新增第三 path**：`StmtBinder._bindForeach` 在"数组 → 索引鸭子"之后，新增"检测目标类型是否（直接或经基类/接口链）实现 `IEnumerable<T>`"的兜底分支；elem type 从 `IEnumerator<T>.get_Current`（或 `IEnumerable<T>` 的类型实参）推断。
- **优先级严格为 数组 → 索引鸭子（Count+get_Item）→ IEnumerable 接口**（见 design Decision 1）。List/Dictionary 带 `Count`+索引器 → 继续命中索引鸭子 path，**不经 enumerator**，比 C# 更快。
- **接口 path 的 codegen 完整发射 try/finally + Dispose**（`IEnumerator : IDisposable`），复用 `StmtEmitter._emitTry` 已有的 finally 下沉机制（`StmtEmitter.z42:384-455`、finally 栈 `:355-381`）。降级形态见 design Decision 2。
- **enumerator 载体 = `[Record] struct`**（对齐 stdlib `ValueTuple2..8` 先例 `src/libraries/z42.core/src/ValueTuple.z42:13-27`，与 C# BCL struct enumerator 一致；见 design Decision 3）。
- **pattern-based 无装箱（design Decision 8）**：foreach 取目标类型上**返回具体 struct 的 `GetEnumerator`**，`__e` 取该具体 enumerator struct 类型（非 `IEnumerator<T>` 接口）→ `MoveNext`/`Current`/`Dispose` 直接在具体 struct 上派发，热路径零装箱。与 Decision 4 不冲突：Decision 4 的"接口静态类型调用时装箱"针对别人以 `IEnumerable<T>` 静态类型调 `GetEnumerator` 的场景，foreach 本身走 pattern-based。
- **`List<T>` / `Dictionary<TKey,TValue>` 显式 `: IEnumerable<T>`** 并新增 `GetEnumerator()`（返回具体 struct 类型）+ 对应 enumerator struct（放 `Std.Collections` 独立文件，design Decision 5）。List 为 index-based enumerator；Dict 为**槽游标零分配** enumerator（持 Dict 引用 + 槽游标扫 `occupied[]`，design Decision 6）。
- **`BoundForeach` 三 path 表示 = 拆独立节点（design Decision 7）**：现 `BoundForeach`（`IsClassIter`/`CountIsField`，`src/compiler/z42c.semantics/src/BoundStmt.z42:214-227`）不动、继续承载数组/索引两 path；新增独立 `BoundForeachEnumerable` 节点 + `StmtEmitter._emitForeachEnumerable` 独立发射函数。

**预期无 zbc/zpkg 格式变更**：`GetEnumerator`/`MoveNext`/`get_Current`/`Dispose` 全部降级为**既有** `VCallInstr`（接口/虚方法 VCall 已在用，见 `get_Item` 的 `StmtEmitter.z42:307`）；try/finally 是既有基本块下沉。纯 codegen + stdlib 源码变更，不新增 IR 指令、不动二进制格式（design 中确认并说明）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | `_bindForeach` 新增 IEnumerable 检测分支（优先级排最后）+ elem type 经 `get_Current`/接口实参推断 |
| `src/compiler/z42c.semantics/src/StmtEmitter.z42` | MODIFY | `_emitStmt` 新增 `BoundForeachEnumerable` 分派 + 新增 `_emitForeachEnumerable`：`GetEnumerator` + try/finally 包裹 `while(MoveNext){ Current }` + `finally{ Dispose }`（现 `_emitForeach` 不动） |
| `src/compiler/z42c.semantics/src/BoundStmt.z42` | MODIFY | 新增 `BoundForeachEnumerable` 节点（含 `EnumeratorType` = 具体 struct 类型）；现 `BoundForeach` 不动（Decision 7） |
| `src/libraries/z42.core/src/Collections/List.z42` | MODIFY | `: IEnumerable<T>` + `GetEnumerator()` 返回 `ListEnumerator<T>` |
| `src/libraries/z42.core/src/Collections/Dictionary.z42` | MODIFY | `: IEnumerable<KeyValuePair<..>>` + `GetEnumerator()` 返回 `DictionaryEnumerator<..>` + 内部槽的 internal 访问器（供 enumerator 读 occupied/keys/values/capacity，Decision 6） |
| `src/libraries/z42.core/src/Collections/ListEnumerator.z42` | NEW | `Std.Collections` 的 `[Record] struct ListEnumerator<T>`（`_list`/`_index`，index-based；Decision 5） |
| `src/libraries/z42.core/src/Collections/DictionaryEnumerator.z42` | NEW | `Std.Collections` 的 `[Record] struct DictionaryEnumerator<K,V>`（持 Dict 引用 + 槽游标，零分配；Decision 5/6） |
| `src/runtime/tests/golden/run/foreach_ienumerable/` | NEW | 端到端 golden：自定义仅实现 IEnumerable 的类正确迭代 + Dispose 被调 |
| `examples/foreach_ienumerable.z42` + `.z42.toml` | NEW | 示例：自定义 iterable 的 foreach |
| `src/libraries/z42.core/tests/list_enumerator.z42` | NEW | List/Dict GetEnumerator + enumerator struct 单测 |
| `docs/book/src/compiler/source-compile.md` | MODIFY | 新增 "foreach 三-path 下沉" 机制小节（现已有 finally 下沉小节，`:49-60`，同页顺延） |
| `docs/book/src/language/foreach.md` | NEW | 语言参考页：foreach 三种迭代目标 + 优先级 + Dispose 语义（现 `language/README.md:30` 标 ⬜ 占位） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂 `language/foreach.md` 目录项 |

### 只读引用

- `src/compiler/z42c.semantics/src/BuiltinTypeDefs.z42:49-58` — IEnumerable/IEnumerator/IDisposable 内建接口面（方法名 = `GetEnumerator`/`MoveNext`/`get_Current`/`Dispose`）
- `src/compiler/z42c.semantics/src/StmtEmitter.z42:384-455` — `_emitTry` finally 下沉范式（接口 path 的 try/finally 复用）
- `src/compiler/z42c.semantics/src/StmtEmitter.z42:305-323` — 值 struct 局部/元素处理先例（enumerator `__e` 作为具体 struct 局部的分配/可变字段原地更新参照）
- `src/libraries/z42.core/src/ValueTuple.z42:13-27` — `[Record] struct` 值类型先例
- `src/libraries/z42.core/src/Protocols/IEnumerable.z42` / `IEnumerator.z42` — 协议契约（本 change 的目标接口）
- `src/libraries/z42.core/src/Collections/Dictionary.z42:99-140` — `Keys()/Values()/Entries()` 的 `occupied[]` 槽扫描写法（Dict enumerator 槽游标遍历直接参照，Decision 6）

## Out of Scope

- **LINQ / iterator chain**（`Select`/`Where`/惰性管道）——本 change 只做 foreach 消费 IEnumerable，不做算子。
- **`yield` / iterator generator**（编译器合成状态机 enumerator）——独立后续。
- **`IComparer` / `IEqualityComparer` 等其它协议接入 foreach**——无关。
- **List/Dict 改走 enumerator path**：决策 1 下二者继续走索引快路径（不经 `GetEnumerator`），`GetEnumerator` 仅在"以 `IEnumerable<T>` 接口静态类型 / 泛型约束 / 未来 LINQ"调用时用到（届时装箱，与 C# 一致）。本 change **不**把 List/Dict 的 foreach 改成 enumerator。
- **非泛型 `IEnumerable` / `IEnumerator`**（无 `<T>`）——z42 内建面只有泛型版本，不引入非泛型。
- **`foreach` 对 `IEnumerable` 求值一次 vs 多次的可空/副作用语义细化**——沿用 C#，不额外规定。

## Open Questions（已由 User 敲定，回填进 design Decision）

- [x] **enumerator struct namespace / 文件落点** → **选项 A**：`Std.Collections` 独立文件（`ListEnumerator.z42` / `DictionaryEnumerator.z42`）。（design Decision 5）
- [x] **Dictionary enumerator 遍历策略** → **选项 B（槽游标零分配）**：enumerator 持 Dict 引用 + 槽游标扫 `occupied[]`，不用 `Entries()` 快照；需向 enumerator 暴露 Dict 内部槽（internal 访问器）。（design Decision 6）
- [x] **`BoundForeach` 三-path 表示** → **选项 B（拆独立节点）**：新增 `BoundForeachEnumerable` + `_emitForeachEnumerable`，现 `BoundForeach` 不动。（design Decision 7）
- [x] **`GetEnumerator` 返回类型解析** → **pattern-based 无装箱**：`__e` 取具体 enumerator struct 类型，直接派发，热路径零装箱；与 Decision 4 的接口静态类型装箱场景不冲突。（design Decision 8）
- [x] **两阶段自举纪律** → 维持"预期不触发、实施时 `xtask test bootstrap` 复核"，无需改结论。（design Decision 9）
