# Tasks: 拆 Parser God-Class（review P1-2，refactor）

> 状态：🟡 进行中 | 创建：2026-07-12 | 子系统：compiler(syntax)
> 同 P1-1 手法：EmitContext 式 mediator——Parser 自身作 mediator（持游标状态
> _lx/_pos/_pendingGt/_diags + 原语 _peek/_advance/_expect），子解析器持 _p 反向引用。

## 进度概览（每步独立不动点 + 单独 commit）
- [x] 1. 抽 `TypeParser`（类型解析 8 方法）—— ✅ 不动点 7/7 + golden 138/138（Parser 1743→1607）
- [x] 2. 抽 `ExprParser`（表达式 Pratt 9 方法）—— ✅ 不动点 7/7 + golden 138/138（Parser 1607→1207）
- [ ] 3. 抽 `StmtParser`（语句递归下降）
- [ ] 4. 抽 `DeclParser`（顶层声明/成员）→ Parser 收敛为 mediator + 3 公开入口

## 备注
- 游标原语 _peek/_peekAt/_advance/_expect/_expectSemi + 共享 _pendingGt/_diags 改 public 供 _p 触达。
- 命名先查 collision。

> **踩坑（step2）**：脚本 find_method 按 `name(` 子串匹配，误中 `ParseExpression(){…_parseExpr(0)}` 体内引用；且 naive 花括号计数不跳 string/char 字面量（`_parseInterpolated` 的 `'{'`）→ span 错乱。改**声明锚定正则 + string-aware 花括号计数**根治。
