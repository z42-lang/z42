# Proposal: 补完测试 attribute 的实参语义校验（E0913 / E0914 / E0917）

## Why

[`enforce-test-attr-placement`](../../archive/2026-09-11-enforce-test-attr-placement/)（#564）补回了
**位置 + 签名**校验（E0911/E0912/E0915），但把**实参语义**三码留作跟进项。它们至今**未实现**，
而每一条对应一个**今天会静默出错**的写法：

| 写法 | 今天的结果 |
|---|---|
| `[Skip] void t()`（无 `reason`）| `_namedStrArg` 返 0 → `if (r > 0)` 不写 → TIDX entry **skipped 但没有理由**，报告里只有一句光秃秃的 skip |
| `[Skip(reason: "x")] void t()`（无 `[Test]`）| TIDX **凭空多一条 `Kind=Test` 的 skipped entry** —— 测试报告里出现一个你没写过的"测试" |
| `[Timeout(milliseconds: 0)]` / 缺参 | `if (ms > 0)` 不写 → **超时静默失效**，你以为设了超时其实没有 |
| `[ShouldThrow<NotAnException>]` | 链只含 `NotAnException`，运行期永远匹配不上 → 测试**以"没抛出预期异常"失败**，错误信息指向抛没抛，而不是"你写的类型根本不是异常" |

共性：**全部是静默降级**——编译器读到不合法的实参，选择了一个"安全"的默认值继续走，
把问题推迟到运行期，且运行期的症状都指不回病灶。这与 #564 修的是同一族问题。

`docs/design/compiler/error-codes.md` 现在（#564 改过后）把这三码明确标为「未实现（跟进项）」——
本变更兑现它。

## What Changes

- `DeclEnforcer._passTestAttrEnforce`（既有纯语法 pass）追加两组**纯语法**检查：
  - **E0914**：`[Skip]` 的 `reason` 必填且非空；`[Skip]` / `[Ignore]` 必须与 `[Test]` / `[Benchmark]` 同贴。
  - **E0917**：`[Timeout]` 的 `milliseconds` 必填且 > 0。
- 新增 `DeclEnforcer._passTestAttrSemantic(table, cu)`（**语义相**，需符号表）：
  - **E0913**：`[ShouldThrow<E>]` 的 `E` 必填；**若 `E` 在符号表中可解析**，其基类链必须到达 `Exception`。
- 三个诊断码常量早已存在于 `DiagnosticCodes.z42`（零引用），直接引用，不新增码。
- 文档：把 error-codes.md / testing.md 里的「未实现」改为已实现。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/DeclEnforcer.z42` | MODIFY | `_passTestAttrEnforce` 追加 E0914/E0917；新增 `_passTestAttrSemantic` + 基类链 helper |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | 三个入口各挂一次 `_passTestAttrSemantic` |
| `src/compiler/z42c.semantics/tests/collect/test_attr_enforce_tests.z42` | MODIFY | 追加 E0913/E0914/E0917 场景 |
| `docs/design/compiler/error-codes.md` | MODIFY | 三码从「未实现」改为已实现 + 触发条件 |
| `docs/book/src/compiler/error-codes.md` | MODIFY | E09xx 段同步 |
| `docs/design/testing/testing.md` | MODIFY | R4 行同步 |
| `docs/spec/changes/complete-test-attr-arg-checks/**` | NEW | 本变更规范 |

**只读引用**：`TestIndexBuilder.z42`（`_namedStrArg` / `_namedIntArg` / `_isDescendantOf` 的既有读法与基类链走法）、
`z42.test/src/Runner.z42`（skip / shouldThrow 的运行期语义）。

## Out of Scope

- **`[ShouldThrow<E>]` 中 E 不存在时报错**：见 design「刻意保守」——跨包 / 表不完整时有误报风险，
  只在**能解析且不派生 Exception** 时报。留作已知缺口。
- **`[TestCase(args)]`**：parser 尚不识别，属独立变更。
- 条件编译 / `[Test]` 不进 zpkg：见 `systematize-test-pipeline` S1–S3。

## Open Questions

- [ ] 无。
