# Proposal: 拒绝未知字符串转义 + 补全标准转义集

## Why

z42 词法器对字符串/字符字面量里的**未知转义序列**（如 `\U`、`\D`、`\q`）采取「丢反斜杠、只留后一字符」的静默降级（`Lexer.DecodeString` else 分支）。这是一个**静默数据损坏**陷阱：

- 用户在 REPL 写 `File.Exists("C:\Users\...\bin\z42i.exe")`，路径里每个 `\X` 都被吞掉反斜杠 → 实际得到 `C:Users...binz42i.exe` → 文件找不到，却毫无报错。必须写成 `\\` 才对。
- C# 对同样的输入直接报编译错误 CS1009「Unrecognized escape sequence」，强制用 `\\` 或 verbatim/raw 串。z42 本应对齐（`E0102 InvalidEscape` 已在 `DiagnosticCodes` 和 `docs/design/compiler/error-codes.md` 里**设计并文档化**，例子正是 `\q`），但词法器**从未真正 emit 它**。

同时暴露出第二个问题：z42 当前识别的转义集只有 `\n \t \r \\ \" \' \0`，**缺 C 系标准的 `\a \b \f \v`**。这导致 `src/libraries/z42.json/JsonParser.z42:255-256` 与 `z42.toml/TomlParser.z42` 里的 `'\b'` / `'\f'` char 字面量被解成字母 `b` / `f`，**z42 的 JSON/TOML 解析器处理 `\b`/`\f` 转义当前是错的**（产出字母而非控制字符）——一个已存在的潜伏 bug。

不做的后果：静默路径/字符串损坏持续存在；JSON/TOML `\b`/`\f` 解析持续错误。

## What Changes

1. **补全标准单字符转义集**，对齐 C#：新增 `\a`(0x07) `\b`(0x08) `\f`(0x0C) `\v`(0x0B)。加上原有的 `\n \t \r \0 \\ \" \'`，构成完整的 C 系单字符转义集。
2. **词法器对未知转义 emit `E0102 InvalidEscape`**（此前从未发出）：普通字符串、字符字面量、插值串文本段里出现不在合法集内的 `\X` → 报错，消息对齐 C# 措辞。
3. `DecodeString` 的 else 静默分支不再是"正常路径"——合法程序里所有转义都已被词法器校验；解码器只做映射。
4. 顺带修正 JSON/TOML 解析器：`'\b'`/`'\f'` 现在正确解为 0x08/0x0C，`\b`/`\f` 转义解析变正确（无需改这两个库的源码，行为随 `DecodeString` 补全而自动修正）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | ① `DecodeString` 补 `\a \b \f \v` 映射（用码点构造，避免自举鸡蛋，见 design D3）；② `_lexString`/`_lexChar`/`_lexInterpolated`/`_skipNestedString` 扫描到 `\X` 时校验，非法则 `_diags.Error(E0102, ...)` + span |
| `src/compiler/z42c.syntax/src/Parser.z42` | MODIFY | **根因修复（实施期扩 Scope，User 批准）**：Parser 从不并入 `_lx.Diagnostics()` → lexer 诊断（E0101/E0102/E0103）端到端全丢，E0102 不生效。加 merge-once 守卫 `_ensureLexDiagsMerged`，在 `ParseCompilationUnit` 收尾 + `Diagnostics()` 访问器并入，不扰动 REPL incomplete 判定（详见 design D5） |
| `src/compiler/z42c.core/src/DiagnosticBag.z42` | MODIFY | 加 `MergeFrom(other)`：追加另一 bag 的 items（不复制 incomplete 标志） |
| `src/compiler/z42c.syntax/tests/lexer/lexer_tests.z42` | MODIFY | 追加词法单测：控制转义解码正确 + 非法转义报 E0102（并入现有 lexer 测试文件，免新 toml 包） |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | 只读引用 | `InvalidEscape = "E0102"` 已存在，不改 |
| `docs/design/compiler/error-codes.md` | MODIFY | E0102 从"已设计未实现"标记为已启用；补全合法转义集说明 |
| `docs/design/language/` 对应字符串字面量页 | MODIFY | 记录合法转义集 + 未知转义报错 + raw 串逃生舱（具体页在 design 阶段定位） |
| `src/tests/` JSON/TOML e2e（如已有 `\b`/`\f` 用例）| MODIFY | 若现有 expected 依赖旧的错误行为，更新为正确的控制字符输出 |

**只读引用**：

- `src/compiler/z42c.syntax/src/ExprParser.z42` — `_unescapeChar` 调 `DecodeString`，理解 char 字面量解码链
- `src/compiler/z42c.syntax/src/Parser.z42` — 字符串/char 字面量解析入口
- `src/libraries/z42.json/src/JsonParser.z42` / `z42.toml/src/TomlParser.z42` — 确认 `'\b'`/`'\f'` 行为随补全自动修正，无需改源

## Out of Scope

- **数字/Unicode 转义** `\0` 八进制扩展、`\uXXXX`、`\xXX`、`\UXXXXXXXX` — 维持 `Lexer.z42:9` 已记的 Deferred。本次收紧后它们会**报 E0102**（诚实：明确告知暂不支持，而非静默产出 `u`+字面）。列入 Deferred 段。
- Raw 串 `"""..."""` 与插值串 `{expr}` 洞内表达式的解析规则不变。

## Open Questions

- [ ] 合法转义集是否就取 C# 单字符全集（`\a \b \f \n \r \t \v \0 \\ \" \'`）？（design 推荐：是）
- [ ] 未知转义用 **error**（阻断编译）还是 warning？（推荐 error，对齐 C#，根除静默损坏）
