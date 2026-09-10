# Design: 测试 attribute 的实参语义校验

> 规范：[proposal.md](proposal.md) · [specs/test-attribute-args/spec.md](specs/test-attribute-args/spec.md)。
> 承接 [`enforce-test-attr-placement`](../../archive/2026-09-11-enforce-test-attr-placement/design.md)（#564）。

## 1. 两个相位，为什么必须分开

| 检查 | 需要什么 | 落点 |
|---|---|---|
| E0914（`[Skip]` reason / 孤儿）、E0917（`[Timeout]` 值域） | **只看同一声明上的 attr 实参** | `_passTestAttrEnforce`（既有**纯语法** pass，签名不变）|
| E0913（`[ShouldThrow<E>]` 的 E 派生 `Exception`） | **符号表 + 基类链** | **新** `_passTestAttrSemantic(SymbolTable, CompilationUnit)` |

**不把 E0913 塞进既有 pass**：那会给它加一个 `SymbolTable` 参数，把「纯语法、零依赖、first-pass 可用」
这条性质一并让掉——而位置/签名检查值得保留这条性质（它们是最该尽早报的一批）。
两个 pass 相邻挂载，代价是 ~15 行。

## 2. E0913 的判定与**刻意保守**

```
TypeArg == ""                      → 报 E0913（"type argument required"）  ← 纯语法，安全
TypeArg 在 table.Classes 里找不到  → **不报**                              ← 保守
TypeArg 找得到且基类链到不了 Exception → 报 E0913
TypeArg == "Exception"             → 合法（不必再有基类）
```

**为什么"找不到"不报**：符号表在若干场景下不完整——`Collect(cu)` 单 CU 无 imports、
跨包类型经 `_mergeImports` 后是否在表里取决于依赖是否已建。报"不存在"会误伤真实代码。
**宁可漏报也不误报**（proposal Out of Scope 已登记为已知缺口）。

基类链走法**照抄** [`TestIndexBuilder._isDescendantOf`](../../../../src/compiler/z42c.semantics/src/TestIndexBuilder.z42)：
`Classes.Get(cur) → HasBase/BaseName → _shortName → 下一跳`，**32 跳上限**防成环。
`Exception` 按**短名**比较（与 `_isDescendantOf` 一致——用户写 `class MyError : Exception`，
表里存的 BaseName 可能带命名空间）。

> **为什么不复用 TestIndexBuilder 的那份**：它是 `private` 且绑在 `IrGen` 反向引用 `_g` 上
> （IR 相位）。DeclEnforcer 在收集相位、拿到的是 `SymbolTable` 参数。两份实现各约 12 行、
> 语义相同——**这是一处刻意的重复**，登记在下方「已知重复」，等有第三个消费者时再抽公共 helper。

## 3. 短路约定（沿用 #564）

位置违规（E0911/E0912/E0915 的 R1）已经 `return`，故**实参检查天然不会叠加**在贴错位置的声明上
（spec A6）。E0914 / E0917 放在 R2–R5 之后，与它们同属「位置合法后才谈」的一层。

## 4. 实参读法

复用 #564 已有的形态判定 —— 命名实参是 `AssignExpr(IdentExpr(key), value)`
（[DeclParser.z42:56-70](../../../../src/libraries/z42c.syntax/src/DeclParser.z42)）：

| 取什么 | 怎么取 |
|---|---|
| `reason` 字符串 | 找 `AssignExpr` 且 `Target.Name == "reason"` 且 `Value is StringLitExpr` → `Lexer.DecodeString(Raw)` |
| `milliseconds` 整数 | 同上，`Value is IntLitExpr` → `ZbcInstr._parseIntLit(Value)` |
| kind attr 是否同贴 | 走 `ad.Attrs`，`HandlerRegistry.IsTestKindAttr(name)` |

> **不做常量求值**：`[Timeout(milliseconds: SOME_CONST)]` 这种写法今天
> `TestIndexBuilder._namedIntArg` 也读不到（它只认字面量）→ 与既有行为一致：
> **非字面量视为"未给出"** → 报 E0917。这与「静默降级为无超时」相比是改进；
> 若将来要支持常量，应连同 `_namedIntArg` 一起改（另案）。

## 5. 诊断消息

```
error[E0914]: `[Skip]` requires a non-empty `reason` (e.g. `[Skip(reason: "flaky on CI")]`)
error[E0914]: `[Skip]` requires `[Test]` or `[Benchmark]` on the same declaration
error[E0917]: `[Timeout]` requires a positive `milliseconds` (got: 0)
error[E0917]: `[Timeout]` requires a `milliseconds` argument (e.g. `[Timeout(milliseconds: 5000)]`)
error[E0913]: `[ShouldThrow]` requires a type argument (e.g. `[ShouldThrow<TestFailure>]`)
error[E0913]: `[ShouldThrow<Plain>]` requires a type deriving from `Exception` (got: `Plain`)
```

三个常量早已存在于 `DiagnosticCodes.z42`（`ShouldThrowTypeInvalid` / `SkipReasonMissing` /
`TimeoutValueInvalid`），**非本次新增 → 直接引用常量，无 F2 冷启动顾虑**（同 #564）。

## 6. 已知重复 / 后续

- **基类链走法两份**（本 pass 一份、`TestIndexBuilder._isDescendantOf` 一份）。第三个消费者出现时抽公共 helper。
- `TestIndexBuilder` 的 `if (r > 0)` / `if (ms > 0)` 降级分支在校验通过后不再可达，留着无害；删它属另一次清理。
- `[ShouldThrow<E>]` 中 E **不存在**时不报（§2），已知缺口。

## 7. 测试

在 #564 已建的 `test_attr_enforce_tests.z42` 上追加，逐条对应 spec A1–A6：
reason 缺失 / 空串 / 合法；`[Skip]`·`[Ignore]` 孤儿 / 与 `[Benchmark]` 同贴；
Timeout 缺参 / 0 / 负 / 正；ShouldThrow 裸用 / 非 Exception / 直接派生 / 传递派生 /
就是 Exception / **解析不到时不报**；以及 A6 的两条不越界。
