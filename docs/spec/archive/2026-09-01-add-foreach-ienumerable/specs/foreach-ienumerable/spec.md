# Spec: foreach over IEnumerable（IEnumerator 协议 path）

> 状态：🔵 DRAFT 待审批。场景描述最终目标语义（可验证行为）。

## ADDED Requirements

### Requirement: foreach 识别 IEnumerable 接口目标

#### Scenario: foreach 遍历仅实现 IEnumerable 的自定义类
- **WHEN** 一个类 `C` 实现 `IEnumerable<T>`（提供 `GetEnumerator()` 返回一个 `IEnumerator<T>`），且**不**具备 `Count`+`get_Item` 索引面，对 `C` 实例 `foreach (var x in c) { use(x); }`
- **THEN** 编译通过（不再报 unsupported）；运行时按 enumerator 产出的顺序对每个元素执行一次循环体；元素静态类型 = `IEnumerator<T>.Current` 的 `T`

#### Scenario: 循环变量类型正确推断
- **WHEN** `foreach (var x in c)`，`c : IEnumerable<string>`
- **THEN** `x` 的静态类型为 `string`（经 `get_Current` 返回类型 / `IEnumerable<T>` 类型实参推断），可当 `string` 使用

#### Scenario: 显式元素类型标注
- **WHEN** `foreach (string x in c)`（非 `var`）
- **THEN** 使用标注类型 `string`；与 enumerator 元素类型不一致时按既有赋值/转换规则处理

### Requirement: 接口 path 完整发射 Dispose（try/finally）

#### Scenario: 正常迭代结束调用 Dispose
- **WHEN** foreach-over-IEnumerable 正常迭代完毕（`MoveNext()` 返回 false）
- **THEN** enumerator 的 `Dispose()` 恰好被调用一次（循环体后、控制流离开 foreach 前）

#### Scenario: 循环体抛异常时 finally 仍调用 Dispose
- **WHEN** foreach-over-IEnumerable 的循环体中途抛出异常
- **THEN** 异常向外传播前，enumerator 的 `Dispose()` 仍被调用一次（finally 语义）

#### Scenario: break / return 离开 foreach 时调用 Dispose
- **WHEN** 循环体内 `break` 或 `return` 提前离开 foreach-over-IEnumerable
- **THEN** 离开前 `Dispose()` 被调用（复用非局部退出经 finally 的机制）

### Requirement: 空集合边界

#### Scenario: 空 IEnumerable 不进入循环体但调用 Dispose
- **WHEN** `c.GetEnumerator()` 的首个 `MoveNext()` 即返回 false
- **THEN** 循环体一次都不执行；`get_Current` 不被调用；`Dispose()` 仍被调用一次

### Requirement: 迭代目标识别优先级（数组 → 索引鸭子 → IEnumerable）

#### Scenario: 数组仍走数组 path
- **WHEN** `foreach (var x in arr)`，`arr` 是数组类型
- **THEN** 走 `ArrayLen`+`ArrayGet` 数组 path，不调用任何 `GetEnumerator`

#### Scenario: List 仍走索引快路径，不经 GetEnumerator
- **WHEN** `foreach (var x in list)`，`list : List<T>`（同时具 `Count`+索引器**和** `IEnumerable<T>`）
- **THEN** 命中索引鸭子 path（`Count` + `get_Item` VCall），**不**调用 `list.GetEnumerator()`、不构造 enumerator（比 C# 少一层间接）

#### Scenario: Dictionary 经 GetEnumerator 槽游标遍历（若无索引 T 面）
- **WHEN** `foreach (var kv in dict)`，`dict : Dictionary<K,V>`（无 `get_Item(int)` 整数索引面、实现 `IEnumerable<KeyValuePair<K,V>>`）
- **THEN** 走 IEnumerable 接口 path（不命中整数索引鸭子）；enumerator 持 Dict 引用 + 槽游标扫 `occupied[]`（零额外分配，design Decision 6），产出各 `KeyValuePair<K,V>`；顺序不保证稳定

> 注：Dict 是否走接口 path 取决于它是否被视作"有整数索引鸭子面"。Dict 的索引器是 `this[TKey]`（`Dictionary.z42:31`）非 `this[int]` → 不命中 `get_Item(int)` 索引鸭子 → 走接口 path。此判定请在 design/实现时对齐 `_bindForeach` 的 `get_Item` 签名匹配细节。

## IR Mapping

接口 path 的 `foreach (T x in coll) <body>` 降级为（对齐 proposal / design Decision 2）：

```
__e = coll.GetEnumerator();        // VCallInstr(dst=__e, coll, "GetEnumerator", [], 0)
try {                              // _emitTry 既有 finally 下沉（StmtEmitter.z42:384-455）
    fe_cond:
        cond = __e.MoveNext();     // VCallInstr(dst=cond, __e, "MoveNext", [], 0)
        BrCondTerm(cond, fe_body, fe_end)
    fe_body:
        x = __e.get_Current;       // VCallInstr(dst=x, __e, "get_Current", [], 0)（属性 getter）
        <body>
        BrTerm(fe_cond)
    fe_end:
} finally {
    __e.Dispose();                 // VCallInstr(_, __e, "Dispose", [], 0)
}
```

- **`GetEnumerator` / `MoveNext` / `get_Current` / `Dispose`** → 均为**既有** `VCallInstr`（接口/虚方法派发；`get_Item` 已同机制，`StmtEmitter.z42:307`）。
- **try/finally** → 既有基本块下沉 + finally 栈（`StmtEmitter._emitTry` / `_pushFinally` / `_emitPendingFinallys`，`StmtEmitter.z42:355-455`），使 `break`/`continue`/`return`/异常离开时都经 `finally{ Dispose }`。
- **`__e` 载体（pattern-based 无装箱，design Decision 8）**：`__e` 取 `GetEnumerator()` 的**具体返回类型**（如 `ListEnumerator<T>` `[Record] struct`），**非** `IEnumerator<T>` 接口 → 不装箱；`MoveNext`/`get_Current`/`Dispose` 直接在具体 struct 上派发。`__e` 是本帧的具体 struct 局部（可寻址 lvalue），`MoveNext` 原地更新其 `_index`/`_slot` 字段（参照 `StmtEmitter.z42:305-323` 的 struct 局部/可变字段处理）。
- **无新增 IR 指令**：`VCallInstr` + 既有块终结符（`BrTerm`/`BrCondTerm`/异常表）足够 → **无 zbc/zpkg 格式 bump**。

## Pipeline Steps

- [ ] Lexer —— 不涉（`foreach` token 已存在）
- [ ] Parser / AST —— 不涉（`ForeachStmt` 已存在；本 change 不改语法）
- [x] TypeChecker + Binder —— `StmtBinder._bindForeach` 新增 IEnumerable 检测分支（优先级排数组/索引鸭子之后）+ pattern-based 解析 `GetEnumerator()` 具体返回类型 + elem type 经 `get_Current`/接口实参推断；产出独立 `BoundForeachEnumerable` 节点（design Decision 7）
- [x] IR Codegen —— `StmtEmitter` 新增独立 `_emitForeachEnumerable`（现 `_emitForeach` 不动）：`GetEnumerator` + try/finally 包裹 `while(MoveNext){ get_Current; body }` + `finally{ Dispose }`
- [x] VM interp —— 无新增指令；验证既有 `VCallInstr`（具体 struct 方法派发，无装箱）+ struct 局部可变字段原地更新 + try/finally 执行链在 foreach 下正确
- [ ] JIT —— interp 全绿后再评估（interp 优先，遵循 M4 纪律）
- [x] stdlib —— `List`/`Dictionary` 实现 `IEnumerable` + `GetEnumerator()` + enumerator `[Record] struct`
