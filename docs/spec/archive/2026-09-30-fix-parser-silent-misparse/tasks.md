# Tasks: Parser 的静默误解析 / 崩溃（名字位、关键字判定、插值串、无初值泛型声明）

> 状态：🟢 已完成 | 创建：2026-09-30 | 完成：2026-09-30 | 归档：2026-09-30
> 分支/worktree：`fix-parser-silent-misparse` | 基于：origin/main
> 类型：`fix`（语法层；无新诊断码——沿用 E0202 ExpectedToken；无格式 bump）

**变更说明（来源：全仓编译器审查 2026-09-30，前端报告 H1–H4）：**

- **H3 无初值泛型声明**：`List<int> xs;` 此前落成比较表达式 → 语义层一串误导性 `undefined: xs`。
  泛型前瞻两支补收 `;`（nullable / 限定名两支早已如此）。
- **H4 关键字判定**：`MemberParser._isWordKeyword` 写死 TokenKind `9..93`，漏 partial/readonly/const/with/methodof
  （149+）→ 参数名写成 `with` 时脱轨级联。改查 `Lexer.IsKeywordKind`（关键字表 = 唯一真相）。
- **H1 名字位吃任意 token**：`_parseType` / `_parseQualifiedName` 无条件前进 → `public }` 吞掉类的 `}`、
  `namespace 123;` 零诊断。改为只收名字 token，否则 E0202；结构闭合符不消费（`Parser._badName`）。
- **H2 插值串**：① 手抄转义表缺 `\' \a \b \f \v`（值错）→ 与普通串共用 `LexerEscapes.DecodeOne`；
  ② 片段数组定长 64 → 超过即**编译器崩溃**（`array index 64 out of bounds`）→ 按需扩容；
  ③ 洞扫描不跳嵌套串（`{Id("}")}` 提前闭洞）→ 与 Lexer 同口径跳过；④ 洞内语法错误留在子 parser 从未并入 → 转发。

**文档影响：** `internals/compiler/source-compile.md` §语法（三处唯一真相）；`DiagnosticCodes` E0492 注释订正。

## 任务

- [x] 1 复现：`List<int> xs;` → `undefined: xs`；81 片段插值串 → 编译器崩溃（修前）
- [x] 2 H3 / H4 / H1 / H2 实施
- [x] 3 回归：`z42c.syntax/tests/stmt.z42`（无初值泛型声明 ×3）、`decl.z42`（`public }` 不吞类边界、
      `namespace 123;`、`with` 作参数名恰好一条诊断、洞内语法错误可见）、
      `src/tests/strings/interp_escapes_and_holes.z42`（转义 / 嵌套串 / 81 片段，带 `opt_all`）
- [x] 4 文档
- [x] 5 GREEN：`xtask test` 全绿
