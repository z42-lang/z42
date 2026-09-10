# Tasks: 补完测试 attribute 的实参语义校验

> 状态：🟢 已完成 | 完成：2026-09-11
> 规范：[proposal.md](proposal.md) · [specs/test-attribute-args/spec.md](specs/test-attribute-args/spec.md) · [design.md](design.md)

- [x] 1.1 `DeclEnforcer._passTestAttrEnforce` 追加**纯语法**实参检查（E0914 / E0917），
      放在 R2–R5 之后（位置违规已 return → 天然不叠加，spec A6）
- [x] 1.2 新增 `DeclEnforcer._passTestAttrSemantic(SymbolTable, CompilationUnit)`（E0913）
      + 基类链 helper（32 跳上限、短名比较、**解析不到不报**，design §2）
- [x] 1.3 `SymbolCollector` 三个入口（`CollectWithImports` / `CollectAll` / `Collect`）
      各挂一次 `_passTestAttrSemantic`，紧邻既有 `_passTestAttrEnforce`
- [x] 1.4 测试：`test_attr_enforce_tests.z42` 追加 spec A1–A6 全部场景
- [x] 1.5 文档：`docs/design/compiler/error-codes.md` / `docs/book/src/compiler/error-codes.md` /
      `docs/design/testing/testing.md` 三处把「未实现」改为已实现 + 触发条件
- [x] 1.6 GREEN：`xtask test` 全 stage（13 个）


## 验证

- **新增 17 个用例全绿**（`test_attr_enforce_tests.z42` 从 22 → 39 例），逐条对应 spec A1–A6，
  含「解析不到不报」的保守分支与两条不越界。
- `xtask test`：**全 13 stage 绿**（含自举不动点、gc modes、walkers）。
- **存量零违规**（实施前扫过）：全仓 13 处 `[Skip]` 全带 `reason`；唯一的 `[Timeout]` 是 60000；
  唯一的 `[Ignore]` 与 `[Test]` 同贴；`[ShouldThrow]` 的 `TestFailure` / `SkipSignal` 均
  `: Exception`。

## 实施记录

- `DeclEnforcer` 需补 `using Z42.IR.BinaryFormat;`（`ZbcInstr._parseIntLit` 读 `[Timeout]` 的
  整数字面量）—— E0436 当场报出。
- 孤儿检查（A2）不能放在 `_teCheck` 里：那里要求先命中 kind attr 才进，而孤儿的定义正是**没有**
  kind attr。故单列 `_teOrphanCheck`，由 `_teWalk` 对每个 `AttributedDecl` 调用（不限于方法）。
