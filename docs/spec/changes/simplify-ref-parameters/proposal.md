# Proposal: 参数修饰符收敛为单一 `ref`（零契约）

## Why

三个修饰符 `ref` / `out` / `in` 目前**在 z42c 里塌缩成一个布尔**
（`MemberParser.z42:340` 三个 TokenKind → `isRef = true`；`ExprParser.z42:239` 调用点连
`refTok.Kind` 都丢了只留 `.Span`），由此产生三个缺陷，全是静默错误：

1. **调用点漏写 `ref` 不报错** —— `void Inc(ref int x)` 配 `Inc(v)` 编译通过，**写入静默丢失**。
   （`docs/spec/changes/fix-silent-semantic-gaps/` 剩余缺口第 1 条）
2. **`in` 宣称只读、零检查** —— 塌缩成 `ref` 之后 callee 可以随便写，调用方毫不知情。
   **比缺功能更糟**：写代码的人会照字面意思信它。
3. **`ref` 实参的类型检查形同虚设** —— `BoundRefArg` 一律以 `Z42UnknownType` 构造
   （`ExprTyper.z42:300`），而 `Conversion.Classify` 对 unknown 是吸收的 ⇒
   实参与形参类型不匹配**不报错**。
4. **跨包完全看不见 `ref`** —— `TsigTypeName` 只处理数组 / nullable / 泛型实参，
   **不记录 `ref`**；`ImportedSymbolLoader` 也没有 `IsRef`。这不是新发现：
   `DiagnosticCodes.z42` 的 **E0465（`ForwardNotRenderable`）注释已明确记载**
   「`ref`/`out` 在 TSIG 格式里**根本不记录**……实测调用点少写 ref 照样编译通过、修改丢失」，
   并因此整类拒绝了跨包 `[Forward]`。

### 根因：三态是自举移植时丢的

`docs/spec/archive/2026-05-05-define-ref-out-in-parameters-typecheck/` 的 Scope 表全是 `.cs` 文件
（`z42.Syntax/Lexer/TokenKind.cs`、`z42.Semantics/TypeCheck/FlowAnalyzer.cs`……）——那是**老的 C# 宿主编译器**。
自举到 z42c 之后，三态区分、`out` 的 DefiniteAssignment、`in` 写保护**全部没有被移植过来**。

> ⚠️ **起草时我写过「文档在谎报」，那句话是错的，在此更正。**
> 谎报的是**旧的** `docs/design/language/parameter-modifiers.md`（开头写「状态：编译期 + 运行时
> 全部已落地」），我是在一棵落后好几天的主树上读到它的。三书重构迁移后的
> `docs/reference/src/language/parameter-modifiers.md` **已经重写得很诚实**——开头就有 ⚠️ 框
> 「当前实现不区分这三者」，附 2026-09-17 的实测表，逐条列出「应当 vs 实际」，
> 还点明了漏写 / 多写 `ref` 是**相反的两个方向**且都不报错。
>
> 这条更正本身值得记：**核实"文档怎么说"必须在最新的树上做**，旧树里的文件可能已经被
> 重写过了。下结论前先 `git fetch`。

### 为什么现在做，以及为什么可以大改

**全仓 `ref`/`out`/`in` 的真实用法 32 处，全部在测试里，生产代码零使用。**
迁移成本 ≈ 改几个测试文件 ⇒ 简化的自由度极大。

### 明确不做的事：不动运行期调用约定

`ref` 走「入口 copy-in / 出口 copy-out」（`interp/mod.rs` 的 Decision R2 architecture E），
**这是对的，本变更不推翻**：

- 复制的是 **`Value`（16 字节）**，不是 struct blob —— blob 值 struct 在寄存器间以
  `StructRef { idx, frame_id }` 句柄流转，copy-in 拿到的是句柄，callee 直接改到 caller 的 blob 上
- 对标量而言两次 16 字节复制**比真别名更快**（真别名要每次访问都间接）
- architecture E 换来的是「callee 的 80+ 指令 handler 完全不需要感知 Ref」

⇒ 本变更**零运行期改动、零 JIT 改动、零 GC 改动**。

## What Changes

### 砍掉 `out`

`out` 的四条规则（callee 必须全路径赋值 / 进入时不可读 / caller 调用后视为已赋值 / throw 路径除外）
**全部是为了处理"未初始化内存"这一个例外**。本变更让**局部变量槽位自动取零值**，
未初始化内存在语言里不再存在 ⇒ 这个例外消失 ⇒ 堵它的四条规则一起消失。

`out` 的人体工学由 `ref var v` 完整保留：

```z42
if (Int32.TryParse(s, ref var n)) { return n; }   // 与 out var n 只差一个词
```

损失：callee 在某条正常返回路径忘了写出参，不再是编译错误。
评估：失败路径忘写 → 调用方按约定不用那个值，无害；成功路径忘写 → 主逻辑坏掉，任何测试都会抓到。
真正独占的价值只剩"抓一条没有测试覆盖的成功路径"。需要时可作为 lint 加回，不必占关键字。

### 砍掉 `in`

`in` 的全部价值是性能（大值类型不复制），但兑现它的只读保证需要**一整套 `readonly` 成员标注系统**
（C# 需要 `readonly struct` + 成员级 `readonly`），而且装完之后 defensive copy 陷阱依然存在。

`docs/reference/src/language/parameter-modifiers.md` 自己记录的立场是
「`in` 仅约束 slot 不可重赋，**不约束指向对象的内部状态**」—— 即 `in p` 时 `p.Mutate()` 照样生效。
所以这个洞是**设计时就知道并接受的**，不是实现遗漏。

它的性能价值由后续的只读别名优化替代（见 Out of Scope），判据是 IR 事实而非声明，
没有 transitive hole（`p.Mutate()` 会写 ⇒ 不做变换 ⇒ 自动退回复制，语义永远对）。

### 单一 `ref`：零契约

| | |
|---|---|
| 含义 | 传地址。仅此，无任何附加契约 |
| 调用方义务 | **无** —— 局部变量槽位自动取零值 |
| 被调方义务 | **无** |
| 不变式 | **语言里永远不存在未初始化的存储** |

### 调用点强制写 `ref`

形参标 `ref` ⇒ 实参必须写 `ref`，否则报错；形参未标 ⇒ 实参不得写 `ref`。
这一条修掉缺陷 ①，也是「声明改了能被发现」的保证来源（加 `ref` / 去 `ref` 都会让全部调用点报错）。

### `ref` 实参类型检查落地

`BoundRefArg` 携带 inner 的真实类型（替掉 `Z42UnknownType`），实参与形参类型必须精确匹配
（`ref` 不做隐式转换——转换会产生临时值，地址就失去意义）。修掉缺陷 ③。

### 新语法糖

- `ref var v` / `ref int v` —— 调用点声明局部 + 取零值 + 传地址
- `ref _` —— 丢弃符，编译器分配隐藏零值槽；表达「我不要这个出参」

### ~~限制到值类型~~ —— 撤销

起草时判断「引用类型本来就按引用传递，`ref` 对它只剩『重新绑定调用方的变量本身』，极罕见」。
**实测推翻**：`src/tests/optimization/escape_ref_param_writeback/source.z42` 正是靠
`void FillArray(ref string[] a) { a = new string[2]; ... }` 做「被调方替换调用方持有的对象」，
而且它是 #690（ref 写回逃逸）的回归测试——限制到值类型会把这条真实且有覆盖的能力砍掉。

⇒ **`ref` 对任何类型都可用**，与本变更前一致。原先挂在这条上的两个理由另行处理：
逃逸那条由 #690 的修复 + 该回归测试保证；与可空标记的交互走正常规则
（`ref string? s` 即「被调方可能写 null 进去」，调用方按标记模型处理）。

### 文档修正

`docs/reference/src/language/parameter-modifiers.md` 全文重写（三修饰符 → 单 `ref`），
移除"全部已落地"的谎报，并记录 copy-in/copy-out 是刻意选择而非缺陷。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|---|---|---|
| `src/libraries/z42c.syntax/src/Lexer.z42` | MODIFY | `out` 退出关键字表；`in` 仅保留给 `foreach` |
| `src/libraries/z42c.syntax/src/Decl.z42` | MODIFY | `Param.IsRef` 注释更正（不再是 "ref/out"） |
| `src/libraries/z42c.syntax/src/MemberParser.z42` | MODIFY | `:340` 形参侧只认 `ref`；`out`/`in` 作形参修饰符 → 报错 |
| `src/libraries/z42c.syntax/src/ExprParser.z42` | MODIFY | `:239` 调用点只认 `ref`；`ref var v` / `ref _` |
| `src/libraries/z42c.syntax/src/Ast.z42` | MODIFY | `RefArgExpr` 增 `IsDiscard` |
| `src/compiler/z42c.semantics/src/DiagnosticCodes.z42` | MODIFY | 新错误码（见 design.md §错误码） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `:300` `BoundRefArg` 带真实类型；值类型限制 |
| `src/compiler/z42c.semantics/src/CallEmitter.z42` | MODIFY | 调用点修饰符匹配 + 实参/形参类型精确匹配 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | `ref _` 隐藏槽；`ref var` 零值初始化 |
| `src/compiler/z42c.semantics/src/ForwardGenerator.z42` | MODIFY | `:384` 对 `out`/`in` 生成错关键字 —— 随三态消失自然修正 |
| `src/compiler/z42c.semantics/src/CtorInheritance.z42` | MODIFY | `:185` `IsRef` 透传不变，注释更正 |
| `src/tests/refs/**` | MODIFY | `in_param` / `out_var` 改写；新增阴性用例（见 spec） |
| `src/compiler/z42c.semantics/tests/**` | MODIFY | codegen / layout 测试里的 `out` 写法 |
| `scripts/test/xtask_*.z42` | MODIFY | 12 处 `out` 写法改写 |
| `docs/reference/src/language/parameter-modifiers.md` | MODIFY | 全文重写 |

## Out of Scope

- **跨包 `ref` 强制检查** —— 独立 follow-up change `record-ref-in-signature`。
  形参在元数据里是 `ExportedParamZ(name, typeString)`，类型是**字符串** ⇒
  记 `ref` 不必改二进制布局，像 `?` / `[]` 那样进类型串即可。但这是**前向不兼容**
  （旧工具读新包会把 `"ref int"` 当未知类型名）⇒ 按 `version-bumping.md` 需 minor bump，
  而本地直接建会撞种子/格式死锁（见 memory 的 CI artifact overlay 配方）。
  **拆出去是为了让本变更能先落地**，不是忽略它。
  ⚠️ **`enforce-value-type-non-null` 依赖它**：`Int32.TryParse(string, ref int)` 一旦跨包调用，
  没有签名里的 `ref` 就无法强制调用点写 `ref`。顺序必须是
  `simplify-ref-parameters` → `record-ref-in-signature` → `enforce-value-type-non-null`。
- **只读别名优化** —— 独立 change。着力点已定位：`CallEmitter._emitStructAwareArgs`
  对**每个** blob struct 实参无条件发 `StructAlloc + StructCopy`；若 callee 的 IR 在任何路径上
  都不写该形参，这两条可以省掉直接传句柄。**纯编译期、一处、零运行期风险**，需跨函数摘要
  （可参考 `IrEscapeSummary` 的不动点）
- **`Span<T>` / `ReadOnlySpan<T>`** —— 独立 change。GC 语言里它是普通 struct（数组句柄 + offset + len），
  不需要 `ref struct`
- **definite assignment pass** —— 独立 change。本变更**不需要**它：槽位自动零值使 `ref` 实参
  不再有"调用前必须已赋值"的义务。`W: 传 ref 前未赋值` 这条提示随 DA change 一起加
- **可空类型模型**（`?` 标记 / 流分析 / `Expect` / 砍 `??` 与 `?.`）—— 独立 change `define-null-model`
- **`ref` 局部变量 / `ref` 返回值** —— 维持 D1 / D2 延后立场
- **`ref struct` / `scoped` / lifetime 标注** —— 维持已有裁决（`feedback_leak_via_diagnostics`：
  生命周期标注永不引入；`ref struct`：GC 语言不需要）
- **`ref C.sf`（静态字段取址）** —— 需 `RefKind::Static` + opcode 0xA3，维持独立变更立场

## Open Questions

**Q1：`ref` 是否参与重载？**

| | 调用点强制写 `ref` | 参与重载 | 漏写 `ref` 的后果 | 成本 |
|---|:---:|:---:|---|---|
| **A（推荐）** | ✅ | ❌ | **报错** | 零（`_dupSigKey` 走 `MangleKey(name, ParamTypes, ParamCount)`，现状就不含修饰符） |
| **B** | ✅ | ✅ | 同时存在按值重载时 → **静默选中它** | `MangleKey` + 跨包签名 + 元数据 |

「声明改了能被发现」由**调用点强制写 `ref`** 保证，两个选项都有。
B 相对 A 只多买到「按值与按引用同名重载共存」，而这种 API 对少见且通常改名更清楚
（`Clamp` / `ClampInPlace`）。

**待 User 裁决。** 默认按 A 写 spec；选 B 则 spec §重载 一节改写 + tasks 增一个阶段。
