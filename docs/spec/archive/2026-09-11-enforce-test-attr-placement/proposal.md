# Proposal: 编译期强制 `[Test]` 家族的位置与签名

## Why

`[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` 今天**没有任何编译期校验**。把 `[Test]` 贴在类的
实例方法上编译期全绿，`TestIndexBuilder` 照写一条 TIDX entry，运行期 `__invoke_static` 撞
`MethodInfo.Invoke: 'X' expects 1 argument(s) (incl. receiver), got 0` —— 报错信息完全指不到病灶
（"你不该把 `[Test]` 贴实例方法"）。

这套校验**本来是有的**：`E0911` / `E0912` / `E0915` 由 spec `compiler-validate-test-attributes`（R4）
落地，实施位置 `src/compiler/z42.Semantics/TestAttributeValidator.cs` —— 属**已退休的 C# 编译器**，
**自举迁移时没有移植到 z42c**。三个诊断码在 `z42c.core/src/DiagnosticCodes.z42` 里定义齐全、
**全仓零引用**，而 `docs/design/compiler/error-codes.md:139` 至今写着「R4.A **已启用**」——
**文档与实现对不上**。

**同一失败模式两年内第二次**：`BenchmarkDesugar`（form-2 `[Benchmark] void f(Bencher b)` 的脱糖 pass）
也随 C# 编译器一起被删、没移植，form-2 benchmark 从此在运行期挂，报的是**一模一样**的
`expects 1 argument, got 0`，直到 2026-07-20 才补回（见该文件头注）。不做这件事，第三次只是时间问题。

不做会怎样：新人在类里写 `[Test] void foo()`（完全合理的直觉）→ 编译通过 → 跑测试时拿到一句看不懂的
arity 错，而且 error-codes.md 会告诉他这条早就被 E0911 拦住了。

## What Changes

- `HandlerRegistry` 加 `IsTestKindAttr(name)`（4 名 kind 子集），既有 `IsTestHandlerAttr`（8 名触发集）
  改为复用它——纯重构，集合不变。
- `DeclEnforcer` 加 `_passTestAttrEnforce`：对带 kind attr 的方法声明强制五条规则
  （零接收者 / 返回 `void` / 无参数 / 非泛型 / 有方法体），全纯语法判定。
- `SymbolCollector` 三个公开入口各挂载一次，与既有三个 `*SuffixEnforce` pass 并列。
- 诊断**复用已有的 E0911 / E0912 / E0915**，不新增码。
- 修复全仓唯一存量违规（`src/tests/zbc-format/with-tidx/` fixture），重冻结其字节 golden。
- 修 `error-codes.md` 的假陈述。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/HandlerRegistry.z42` | MODIFY | 加 `IsTestKindAttr`；`IsTestHandlerAttr` 复用之 |
| `src/compiler/z42c.semantics/src/DeclEnforcer.z42` | MODIFY | 加 `_passTestAttrEnforce` + `_teWalk`/`_teCheck`/`_teKindAttr`/`_teCode`/`_teShape` |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | `CollectWithImports` / `CollectAll` / `Collect` 三处各挂一行 |
| `src/tests/zbc-format/with-tidx/source.z42` | MODIFY | 两个测试方法加 `static`（存量唯一违规） |
| `src/tests/zbc-format/with-tidx/source.zbc` | MODIFY | 重冻结（字节 golden） |
| `src/tests/zbc-format/with-tidx/expected.json` | MODIFY | 重冻结（`param_count` 1→0 / `param_types` 置空 / `is_static` false→true） |
| `src/compiler/z42c.semantics/tests/collect/test_attr_enforce_tests.z42` | NEW | 五条规则 + 覆盖表的 negative/positive 用例 |
| `docs/design/compiler/error-codes.md` | MODIFY | 修 E0911/E0912/E0915 的假陈述（实施位置指向已删除的 C# 文件）；E0913/E0914/E0917 标「未实现」 |
| `docs/design/testing/testing.md` | MODIFY | §编译期校验 指向新 pass |
| `docs/spec/changes/enforce-test-attr-placement/**` | NEW | 本变更规范（归档时移入 `archive/`） |

**只读引用**（理解上下文必须读，不修改）：

- `src/compiler/z42c.semantics/src/TestIndexBuilder.z42` — 现有「什么算一个测试」的判据
- `src/compiler/z42c.semantics/src/BenchmarkDesugar.z42` — form-2 脱糖，决定本 pass 的相位约束
- `src/libraries/z42.test/src/Runner.z42` — runner 的零接收者不变量（规则来源）
- `src/runtime/src/corelib/reflection/invoke.rs` — `__invoke_static` 的 arity 检查（失败现场）
- `src/libraries/z42c.core/src/DiagnosticCodes.z42` — 三个复用码的定义

## Out of Scope

- **通用 attribute usage 框架**（`[Usage]` / `Target` 词汇 / `Require` 谓词 / 注册表 / 跨包）——
  已存档于本目录 `design-full-usage-framework.md`，出现第二个用例再取。
- **`[Skip]` 孤儿**（无 `[Test]` 单独出现）→ E0914：会让 TIDX 多一条凭空的 skipped entry，但**不崩**。
- **`[ShouldThrow<E>]` 的 E 须继承 `Exception`** → E0913：需符号表判继承链，不是纯语法，另一个相位。
- **`[Timeout]` 值域** → E0917：实参语义，同上。
- **`[Native]` / `[Record]` / `[Deprecated]` 的位置约束**：贴错不崩、只是静默无视；审计确认存量全合规。
- **条件编译 / `[Test]` 不进 zpkg**：见 `docs/spec/changes/systematize-test-pipeline/`（S1–S3）。

## Open Questions

- [ ] 无。（相位约束、诊断码复用、fixture 重冻结方式均已在 design.md 定稿。）
