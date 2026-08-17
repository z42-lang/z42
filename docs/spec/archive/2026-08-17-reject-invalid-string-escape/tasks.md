# Tasks: 拒绝未知字符串转义 + 补全标准转义集

> 状态：🟢 已完成 | 创建：2026-08-17 | 完成：2026-08-17

## 进度概览
- [x] 阶段 1: 补全 DecodeString 转义集（自举安全）
- [x] 阶段 2: 词法器未知转义校验 → E0102
- [x] 阶段 2.5: 根因修复——lexer 诊断端到端上浮（实施期扩 Scope，User 批准）
- [ ] 阶段 3: 测试与文档

## 阶段 2.5: lexer 诊断上浮（fix-lexer-diags-dropped）
- [x] 2.5.1 `DiagnosticBag.MergeFrom(other)`：追加 items（不复制 incomplete 标志）
- [x] 2.5.2 `Parser` 加 `_lexMerged` 守卫 + `_ensureLexDiagsMerged()`（收尾并入 lexer 诊断）
- [x] 2.5.3 在 `ParseCompilationUnit` 收尾 + `Diagnostics()` 访问器触发（幂等；不扰 REPL incomplete）
- [x] 2.5.4 手动验 REPL（重建 toolchain 后）：`"C:\Users"`→E0102、`"a\qb"`→E0102、`(1+⏎2)` 续行→3 ✓；`\b` 为合法退格

## 阶段 1: 补全 DecodeString
- [x] 1.1 `Lexer.z42` DecodeString 加 `\a \b \f \v` 映射，控制字符用码点构造（`_ctrlChar(int)` + `String.FromChars`，不用 `\b` 字面量，见 design D3）
- [x] 1.2 确认码点→char 构造 API：`(char)code` cast + `String.FromChars`（stdlib 已有惯用法，自举 5/5 gen1==gen2 验证无鸡蛋）

## 阶段 2: 未知转义校验
- [x] 2.1 `Lexer.z42` 加 `_isKnownEscape(char)` 共享谓词 + `_validateEscape(int)` 报错助手
- [x] 2.2 `_lexString` 扫描到 `\X` 非法 → `_diags.Error(E0102, span)`（span 覆盖 `\X` 两字符）
- [x] 2.3 `_lexChar` 同上
- [x] 2.4 `_lexInterpolated` 文本段 + `_skipNestedString` 同上

## 阶段 3: 测试与验证
- [x] 3.1 追加 `z42c.syntax/tests/lexer/lexer_tests.z42`：控制转义解码 + 反例 E0102 + 经 Parser 上浮回归守卫 + 未终止串续行守卫（7 test）
- [x] 3.2 cargo build z42vm 无错（GREEN gate 内）
- [x] 3.3 xtask test compiler（自举 5/5 gen1==gen2，24 units）
- [x] 3.4 xtask test stdlib（JSON/TOML 全绿；现有 `\\b` 测的是转义反斜杠、非退格，不受影响；无 expected 需改）
- [x] 3.5 xtask test e2e + cross-zpkg（GREEN gate 内）
- [x] 3.6 spec scenarios 逐条覆盖确认（e2e：`\U`→E0102、`\\`+`\b`→路径+8；REPL：E0102+续行）
- [x] 3.7 文档同步：`language-overview.md` 转义小节（合法集/报错/raw 逃生舱）；`error-codes.md` E0102 行已准确无需改；syntax README 功能索引
- [x] 3.8 Deferred 索引：`docs/roadmap.md` 加 escape-future-numeric-unicode 行
- [x] 3.9 代码规模：Lexer.z42 精简注释回落 498 行（< 500 硬限）

## 备注
- 无 zbc/zpkg 格式 bump。
- 顺带修 JSON/TOML `\b`/`\f` 潜伏 bug（根因修复 DecodeString，不动这两个库源码）。
