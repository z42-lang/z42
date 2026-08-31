# Design: foreach over IEnumerable（IEnumerator 协议 path）

> 状态：🔵 DRAFT 待审批（原 5 个 Open Question 已由 User 敲定，逐条回填进各 Decision）| 类型：lang / vm
>
> 自包含、无前置依赖：enumerator 实例是 `var __e = coll.GetEnumerator()` 得到的**标量** struct（非数组），跨包实例化正常工作，不涉及任何 layout 修复前置。

## Architecture

foreach 绑定/发射走**三-path 决策树**，优先级从上到下（首个命中即用）：

```
foreach (T x in coll)
  │
  ├─[1] coll.Type() is Z42ArrayType ?
  │       └─ YES → 数组 path（现状）
  │                ArrayLenInstr 求长 + ConstI32 计数器 + ArrayGetInstr 取元素
  │                StmtEmitter.z42:288-290, :308-310
  │
  ├─[2] classDef has get_Item(int)  AND  (Count field OR Count method) ?
  │       └─ YES → 索引鸭子 path（现状；List/其它索引容器）
  │                Count field/VCall 求长 + get_Item VCall 取元素
  │                StmtBinder.z42:42-45  /  StmtEmitter.z42:281-290, :305-307
  │
  └─[3] classDef implements IEnumerable<T>（含基类/接口链）?      ← 本 change 新增
          └─ YES → IEnumerable 接口 path（兜底：仅实现 IEnumerable、无整数索引面）
                   //【Decision 8：pattern-based 无装箱】__e 取 GetEnumerator() 的
                   //  具体返回类型（如 ListEnumerator<T> struct），非 IEnumerator<T> 接口。
                   var __e = coll.GetEnumerator();   // __e : <具体 enumerator struct 类型>
                   try   { while (__e.MoveNext()) { var x = __e.Current; <body> } }
                   finally { __e.Dispose(); }
                   MoveNext/Current/Dispose 直接在具体 struct 上派发（零装箱）
                   + 既有 try/finally 下沉（StmtEmitter._emitTry）
          └─ NO  → 报错（既有 unsupported foreach 目标诊断）
```

关键性质：**List 具 Count+索引器 → 命中 [2]，永不进 [3]**，即使它显式 `: IEnumerable<T>`；[3] 纯粹兜底"无整数索引面"的可迭代类型。

## Decisions

### Decision 1: 识别优先级 = 数组 → 索引鸭子 → IEnumerable（User 已锁定）
**问题**：新 IEnumerable path 插在决策树哪个位置？
**选项**：(A) 接口优先（C# 之外的纯协议驱动）；(B) 数组 → 索引鸭子 → 接口兜底。
**决定**：**B**。理由：与 C# 哲学一致（C# 也优先数组/索引器的 pattern-based foreach）；且对 List/索引容器**比 C# 更快**——直接 `Count`+`get_Item` VCall，省掉 `GetEnumerator()` 构造 + 每步 `MoveNext`/`Current` 两次 VCall + enumerator 装箱。接口 path 只服务"仅实现 IEnumerable、无索引"的类型。**此决策不可推翻。**

### Decision 2: 接口 path 完整发射 try/finally + Dispose（User 已锁定）
**问题**：`IEnumerator : IDisposable`（`IEnumerator.z42:14`），foreach-over-IEnumerable 是否发 `Dispose`？
**决定**：**发**，且用 try/finally 保证异常/break/return/正常结束**所有**离开路径都调 `Dispose`。降级形态：
```
var __e = coll.GetEnumerator();
try { while (__e.MoveNext()) { var x = __e.Current; <body> } }
finally { __e.Dispose(); }
```
理由：对齐 C# foreach 语义（编译器为 `IEnumerator<T>` 生成 `finally { e?.Dispose(); }`）；复用 `StmtEmitter._emitTry` 既有 finally 下沉 + finally 栈（`StmtEmitter.z42:355-455`），非局部退出经 finally 已被现有机制正确处理。**此决策不可推翻。**

### Decision 3: enumerator 载体 = `[Record] struct`（User 已锁定）
**问题**：List/Dict 的 enumerator 用 class 还是 struct？
**选项**：(A) class（引用语义，堆分配）；(B) `[Record] struct`（值语义，内联 blob）。
**决定**：**B**。理由：z42 struct 已是**真值语义**（内联字节 blob，见 `docs/book/src/runtime/struct-value-semantics.md`），stdlib 已有 `[Record] struct` 先例 `ValueTuple2..8`（`ValueTuple.z42:13-27`）；对齐 C# BCL 的 `List<T>.Enumerator` 等 struct enumerator，迭代热路径零堆分配。**此决策不可推翻。**

### Decision 4: List/Dict foreach 走索引、struct enumerator 仅接口/泛型/LINQ 时用到（User 已锁定）
**问题**：既然给 List 加了 struct enumerator，foreach-over-List 是否改走 enumerator？
**决定**：**不改**。Decision 1 下 foreach-over-List 命中索引 path，**不经 enumerator**。struct enumerator 主要在"以 `IEnumerable<T>` 接口静态类型 / 泛型约束 `where T: IEnumerable<U>` / 未来 LINQ"调用 `GetEnumerator()` 时用到——那时 struct→接口**装箱**（既有 struct 值语义装箱行为），与 C# 一致。**此决策不可推翻。**

### Decision 5: enumerator struct 的 namespace / 文件落点（User 已定：选项 A）
**问题**：`ListEnumerator` / `DictionaryEnumerator` 放哪？
**选项**：(A) `Std.Collections` 独立文件（`ListEnumerator.z42` / `DictionaryEnumerator.z42`）；(B) 与容器同文件；(C) 嵌套类型（若 z42 支持并合适）。
**决定**：**A**——`Std.Collections` namespace，各自独立文件 `ListEnumerator.z42` / `DictionaryEnumerator.z42`（与 List/Dict 同 ns，便于 `[Record] struct` 主构造器书写、便于跨包引用）。

### Decision 6: Dictionary enumerator 遍历策略（User 已定：选项 B，槽游标零分配）
**问题**：Dict enumerator 如何产出 `KeyValuePair<K,V>`？
**选项**：(A) 基于既有 `Entries()` 快照数组（`Dictionary.z42:128-140`）——实现最简，enumerator 内持 `KeyValuePair<K,V>[]` + index；代价是一次性分配 `Count` 大小数组。(B) enumerator 内持 Dict 引用 + 槽游标，遍历 `occupied[]`（`Dictionary.z42` 内部槽）——零额外分配，但需向 enumerator 暴露内部槽字段。
**决定**：**B**——`DictionaryEnumerator<K,V>` 内持 Dict 引用 + 槽游标（int），`MoveNext` 前进游标跳过空槽（扫 `occupied[]` 到下一个 true），`Current` 从 `keys[slot]`/`values[slot]` 组装 `KeyValuePair<K,V>`；**零额外分配**（不快照）。产出顺序不保证稳定（同 `Keys()` 注释 `Dictionary.z42:96-98`）。内部槽字段暴露方式见 Implementation Notes。

### Decision 7: foreach-over-IEnumerable 表示 —— **binder 脱糖**（User 2026-09-01 改定，取代原「独立节点」）
**问题**：`BoundForeach` 现用 `IsClassIter`+`CountIsField` 两 bool（`BoundStmt.z42:219`）表达数组/索引两 path，如何加第三 path（IEnumerable）？
**选项**：(A) 独立 `BoundForeachEnumerable` 节点 + 独立 `_emitForeachEnumerable` emitter 直接发 IR（原 DRAFT 锁定）；(B) **binder 脱糖**成既有 AST 再走 `_bindStmt`。
**决定（改定）**：**B——binder 脱糖**。实施途中发现 (B) 更省更安全、且对外行为与 (A) 完全一致（User 经 AskUserQuestion 裁定改用 B）。`StmtBinder._bindForeach` 加**早期分流**：目标既非数组、也无 `get_Item`+`Count` 整数索引面、但有 `GetEnumerator()` → 调 `_bindForeachEnumerable`，把 foreach 脱糖成
```
{ var __e = <Iter>.GetEnumerator();
  try { while (__e.MoveNext()) { <T> x = __e.Current; <Body> } }
  finally { __e.Dispose(); } }
```
的**既有 AST 节点**（VarDecl / TryCatch(finally) / While / Call / Member），再交 `_bindStmt`。
**收益**：`var __e = ...GetEnumerator()`（struct 值返回 sret）、try/finally 栈、break/continue/return 经 finally、方法/属性派发**全部复用已验证的既有 lowering**——**零新 Bound 节点、零新 emitter 函数**，`_emitForeach`（数组/索引计数器循环）完全不动 → byte-identical 保持（z42c 源自身 foreach 均走数组/索引 path，从不进脱糖分支）。`__e` 名以 `span.Start` 唯一化（防嵌套 foreach 在函数级平表寄存器冲突）。
**依赖的编译器修复（实施中发现，同 PR 一并落地）**：foreach 脱糖用 `__e.Current`（属性 getter），触发两个 struct 属性 getter codegen 缺口——① struct 成员读误当字段发 `struct_fget_prim @-1`（已由 PR #359 `fix-struct-property-getter` 修，in-app）；② **imported 泛型 struct 属性 getter 返回类型未 `_substGeneric` 替换 → 松绑 Unknown → sret 失配 `StructCopy got Null`**（`MemberResolver.z42` Z42InstantiatedType 分支只查字段不查属性 getter；本 change 补属性 getter + 替换，`DictionaryEnumerator<K,V>.Current→KeyValuePair<K,V>` 跨包解锁）。

### Decision 8: `GetEnumerator` 返回类型解析（User 已定：pattern-based 无装箱）
**问题**：接口 path 里 `__e = coll.GetEnumerator()`，`__e` 的类型取 `IEnumerator<T>`（接口，struct enumerator 装箱）还是具体 struct 类型（无装箱，C# pattern-based 优先此路）？
**决定**：**pattern-based 无装箱**——foreach 识别目标类型上**返回具体 struct 的 `GetEnumerator`**，`__e` 取该**具体 enumerator struct 类型**（非 `IEnumerator<T>` 接口 → 不装箱），`MoveNext`/`get_Current`/`Dispose` 直接在具体 struct 上派发。与 C# pattern-based foreach 一致、迭代热路径零装箱。
**与 Decision 4 的关系（点明，二者不冲突）**：Decision 4 说的"struct enumerator 以 `IEnumerable<T>` 接口静态类型调用时装箱"仍成立——那是**别人**（泛型约束 `where T: IEnumerable<U>` / 未来 LINQ / 显式接口变量）以接口静态类型调用 `GetEnumerator` 的场景。而 **foreach 语句本身走 pattern-based、取具体 struct 类型、不装箱**。两条路径针对不同调用点，各自成立，不矛盾。

### Decision 9: 无 zbc/zpkg 格式变更（确认）
**问题**：本 change 是否触及二进制格式 / 自举纪律？
**确认**：**不触及**。`GetEnumerator`/`MoveNext`/`get_Current`/`Dispose` 全部降级为**既有** `VCallInstr`（`get_Item` 已同机制 `StmtEmitter.z42:307`）；try/finally 是既有基本块 + 异常表下沉。无新 IR 指令、无 writer/reader 变更 → **无 zbc/zpkg minor bump**。自举纪律（bootstrap-seed）：新 codegen 是编译器**能力**（support），z42c 源自身 foreach 均走数组/索引 path（不用接口 path），stdlib 新增的 `GetEnumerator`/enumerator struct 由自建当前 z42c 编译、z42c 运行期不加载 → 预期**不触发** axis②（stdlib API 面）/ axis④（z42c 运行期自依赖库）。**待实现时以 `xtask test bootstrap` 复核确认可单 PR 落地。**

## Implementation Notes

### BoundStmt：新增 `BoundForeachEnumerable` 节点（`BoundStmt.z42`；Decision 7）
- 现 `BoundForeach`（`:214-227`，字段 `VarName`/`VarType`/`Collection`/`Body`/`IsClassIter`/`CountIsField`）**不动**，继续承载数组 + 索引鸭子两 path。
- 新增独立 `BoundForeachEnumerable : BoundStmt`，字段至少含：`VarName`、`VarType`（元素类型）、`Collection`（`BoundExpr`）、`Body`（`BoundStmt`）、`EnumeratorType`（`GetEnumerator()` 返回的**具体 struct 类型**，Decision 8）——供 emitter 以具体类型 VCall `MoveNext`/`get_Current`/`Dispose`、并知道 `__e` 的 struct 类型（分配/StructCopy）。

### StmtBinder：新增 IEnumerable 检测分支（`StmtBinder._bindForeach`, `StmtBinder.z42:15-47`）
- 现结构：先判 `Z42ArrayType`（`:21`）→ 再判 `Z42ClassType` 的 `get_Item` 求 elem type（`:22-29`）；下方 `:36-45` 判 `IsClassIter`（get_Item + Count field/method）。
- 新增：在 `feIsClassIter` 判定**为 false 时**（既非数组、也无 `get_Item`+`Count` 索引面），检测 `feClassDef` 是否实现 `IEnumerable<T>`——遍历其接口/基类链（复用类型系统里已有的接口实现查询；`feClassDef` 已从 `Z42ClassType`/`Z42InstantiatedType` 取出，`:40-41`）。
- **pattern-based 解析（Decision 8）**：命中后，解析目标类型上的 `GetEnumerator()` 方法，取其**具体返回类型**（如 `ListEnumerator<T>` struct）作为 enumerator 类型；elem type 从该 enumerator 的 `get_Current` 返回类型（或 `IEnumerable<T>` 实参 `T`）推断（对齐 `get_Item` elem 推断 `:25-28` 的写法）。
- 结果：产出 **`BoundForeachEnumerable`**（带 `EnumeratorType` = 具体 struct 类型），而非 `BoundForeach`。

### StmtEmitter：新增 `_emitForeachEnumerable`（`StmtEmitter.z42`；Decision 7/8）
- `_emitStmt`（`:15-132`）新增分派：`else if (s is BoundForeachEnumerable) { this._emitForeachEnumerable(...); }`（现 `BoundForeach` 分支 `:111-112` 不动）。
- 现 `_emitForeach`（`:272-337`）**不改**（继续服务数组/索引两 path）；新增独立 `_emitForeachEnumerable`：
  1. `collReg = Emit(fe.Collection)`。
  2. `eReg = Alloc(...)`（按具体 enumerator struct 类型分配，Decision 8 无装箱 → `__e` 是本帧具体 struct 值/句柄，参照 `_emitForeach` 里 struct 局部处理 `:305-323` + `BoundVarDeclStmt` 的 struct 分配 `:36-51`）；`VCallInstr(eReg, collReg, "GetEnumerator", [], 0)`（若 `GetEnumerator` 是普通方法且去虚化，可能降级为 `CallInstr`——依类型系统裁定，实现时按现有 call 发射规则）。
  3. **倾向做法**：把「while 循环体 + `finally{ __e.Dispose() }`」组装成一个合成 `BoundTry`（tryBody = while, hasFinally = `__e.Dispose()` 调用），交给现成 `_emitTry`（`:384-455`）复用 finally 下沉 + finally 栈——最省新代码，break/continue/return/异常经 finally 自动正确。
  4. while 体：`fe_cond` 块 `VCall MoveNext` on `eReg` → `BrCondTerm(cond, fe_body, fe_end)`；`fe_body` 块 `VCall get_Current` on `eReg` → `_writeBack(fe.VarName, curReg)` → `_emitStmt(fe.Body)` → `BrTerm(fe_cond)`；`PushLoop(fe_cond, fe_end, ...)` 使 break/continue 正确（参照 `:301`）。
  5. finally 块：`VCall Dispose` on `eReg`。
- **注意**：`get_Current` 是属性 getter → 方法名 `get_Current`（`BuiltinTypeDefs.z42:57`）；`Dispose` 来自 `IDisposable`（`BuiltinTypeDefs.z42:50`）。

### stdlib：List / Dictionary 实现 IEnumerable（Decision 5/6）
- `List<T>`（`List.z42:13`）：加 `: IEnumerable<T>`（与现有 `where T:...` 约束共存）；`public ListEnumerator<T> GetEnumerator() { return new ListEnumerator<T>(this); }`（**返回具体 struct 类型**，Decision 8 pattern-based；同时它天然满足 `IEnumerable<T>` 契约）。
- `ListEnumerator.z42`（NEW，`Std.Collections`，Decision 5）：`[Record] struct ListEnumerator<T>`，字段 `List<T> _list; int _index;`（初值 `_index = -1`）；`MoveNext()` = `_index+1 < _list.Count` 时 `_index = _index+1; return true` 否则 false；`Current { get { return _list[_index]; } }`（走 List 索引器 `List.z42:36`）；`Dispose()` 空实现。主构造器/字段写法参照 `ValueTuple.z42:13`。
- `Dictionary<TKey,TValue>`（`Dictionary.z42:8`）：加 `: IEnumerable<KeyValuePair<TKey,TValue>>`；`public DictionaryEnumerator<TKey,TValue> GetEnumerator() { return new DictionaryEnumerator<TKey,TValue>(this); }`。
- `DictionaryEnumerator.z42`（NEW，`Std.Collections`，Decision 5/6 槽游标零分配）：`[Record] struct DictionaryEnumerator<TKey,TValue>`，字段 `Dictionary<TKey,TValue> _dict; int _slot;`（初值 `_slot = -1`）；`MoveNext()` 从 `_slot+1` 起线性扫 `occupied[]` 找下一个 true 槽，找到则置 `_slot` 返 true、扫到 `capacity` 返 false；`Current { get { return new KeyValuePair<TKey,TValue>(_dict.<keys>[_slot], _dict.<values>[_slot]); } }`；`Dispose()` 空实现。
  - **内部槽字段暴露**：Dict 的 `occupied`/`keys`/`values`/`capacity` 现为 **private**（`Dictionary.z42:9-19`）。DictionaryEnumerator 需读它们 → 需**同包内部可见性**（`internal`，enumerator 与 Dict 同 `z42.core` / 同 `Std.Collections` ns，走跨包 internal 类/friend 机制，参照 memory `add-crosspkg-internal-class`）。**具体暴露手段（改字段为 internal / 加 internal 访问器方法）在实现时确定**，倾向：为 Dict 加 internal 访问器（如 `internal bool _occupiedAt(int i)` / `internal TKey _keyAt(int i)` / `internal TValue _valAt(int i)` / `internal int _cap()`），避免直接把字段公开、也不破坏封装。

### 潜在坑：struct enumerator 的可变字段（pattern-based 局部，无装箱）
- Decision 8 下 `__e` 是**本帧的具体 struct 局部**（非装箱堆对象）。struct 值语义下 `MoveNext()` 必须**原地**修改 `__e` blob 内的 `_index`/`_slot`（可寻址 lvalue），且后续 `get_Current`/下一轮 `MoveNext` 读到同一 blob 的更新值。**实现时必须验证**：`VCall MoveNext` on 一个 struct 局部句柄能原地改其字段并被后续调用看到（否则计数器永远停在初值 → 死循环 / 只迭代一个）。这是本 change 最需要 e2e 验证的语义点——与 struct-value-semantics 的"穿过 struct 字段的原地修改（3a）/可寻址 lvalue"直接相关。

## 实施结果与偏离（2026-09-01 落地）

- **Decision 3（`[Record] struct` enumerator）保留、实测可行**：`[Record] public struct ListEnumerator<T>(List<T> _list) { int _pos; ... }` / `DictionaryEnumerator<K,V>(Dictionary<K,V> _dict) { int _scan; int _cur; ... }`——positional 字段（容器引用）只读 + **可变 body 字段**（`_pos`/`_scan`/`_cur`，零初值语义避免 C# 的 -1 约定）+ 方法/属性，编译运行正常，struct 局部上的方法原地改字段能持久（byref-like）。
- **已实现**：foreach 第三 path 脱糖（Decision 7 = binder 脱糖）；`List<T>.GetEnumerator()` + `ListEnumerator<T>`；`Dictionary<K,V>.GetEnumerator()` + `DictionaryEnumerator<K,V>`（槽游标零分配，Decision 6）+ Dict internal 槽访问器；两个 struct 属性 getter 编译器修复（见 Decision 7 末）。
- **延后（follow-up，不阻塞本 change 核心价值）**：
  - **`List/Dict : IEnumerable<T>` 形式接口声明**——本 change 只加了 `GetEnumerator()`（pattern-based，foreach + 显式用法已全通）；形式 `: IEnumerable<T>` 声明需 z42 接受「具体 struct 返回满足接口 `IEnumerator<T> GetEnumerator()`」的契约，有接口满足性不确定，留后续。
  - **`foreach (var kv in dict)` 直接迭代 Dict**——Dict 有 `get_Item(TKey)`（键索引器）+ `Count`，命中 foreach 的**索引 path**判据（只查 `get_Item` 存在性、不查参数是否 int）→ 走 `get_Item(int)` 索引 path 而非 DictionaryEnumerator。这是**既有行为**（本 change 前 Dict 就如此，用户用 `dict.Entries()` / 显式 `GetEnumerator()`）。让直接 foreach-Dict 走 enumerator 需精化索引判据（get_Item 形参须 int），留后续。

## Testing Strategy

| 需求 | 测试类型 | 落点 |
|------|---------|------|
| 仅实现 IEnumerable 的类正确迭代 | golden run e2e | `src/runtime/tests/golden/run/foreach_ienumerable/` |
| Dispose 正常/异常/break 均调用 | golden run（Dispose 里打印/置标记断言调用） | 同上 + 多用例 |
| 空 IEnumerable 不进循环体但调 Dispose | golden run | 同上 |
| List 仍走索引、不调 GetEnumerator | golden run（GetEnumerator 里埋"不应被调"标记）/ IR 检视 | 同上 |
| struct enumerator 的可变字段原地更新（pattern-based 局部，迭代计数 == 元素数） | golden run | foreach_ienumerable golden |
| 循环变量类型推断（var / 显式） | binder/typecheck | z42c.semantics 相应测试面 |
| List/Dict GetEnumerator + enumerator struct（跨包泛型标量实例） | stdlib 单测 + 跨包 e2e | `src/libraries/z42.core/tests/list_enumerator.z42` |

**GREEN 命令**：`xtask test`（全 stage gate）+ `xtask test bootstrap`（自举越界复核，确认无 zbc/zpkg/语法越界，Decision 9）+ 若动 runtime 则另 `cargo test --lib`（本 change 预期不动 runtime）。⚠️ GREEN 前清 stale e2e：`rm -rf /tmp/z42c-e2e-*`（见 memory `stale-tmp-e2e-buildtext-false-fail`）。
