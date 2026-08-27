# Tasks: 模式匹配 D（`with` 表达式）

## 0. DRAFT + 6.5
- [x] proposal / design / spec 落地
- [x] User 6.5 确认（① 脱糖=读原字段作 ctor 实参+替换覆盖 ② 仅主构造器字段 ③ 仅 record class ④ 关键字 with）

## 1. syntax
- [x] `TokenKind.z42`：`With = 154`（末尾追加，不入 zbc）
- [x] `Lexer.z42`：`_initKeywords()` 加 `_kw("with", TokenKind.With)`
- [x] `Ast.z42`：`WithExpr { Expr Target; string[] FieldNames; Expr[] FieldValues; int FieldCount }`
- [x] `ExprParser.z42`：后缀分支（bp 85，仿 switch）+ `_parseWithBody`（复用对象初始化器字段体文法）

## 2. semantics
- [x] `ConstructTyper.z42`：`_bindWith`（record 校验 + 覆盖字段名校验 + temp 求值一次 + 按声明序造 ctor 实参
      → 脱糖 BoundSeqExpr）+ `_isOwnField` / `_findOverride` helper
- [x] `ExprTyper.z42`：`_bindExpr` dispatch 加 `if (e is WithExpr) return _construct._bindWith(...)`

## 3. vscode grammar SoT 同步
- [x] `scripts/install/xtask_install_vscode.z42`：`_kwOperatorExpr()` 加 `"with"`
- [x] 重建 xtask.zpkg（改 xtask 脚本后）→ `xtask deps install vscode` 重生成 `z42.tmLanguage.json`

## 4. 测试
- [x] `src/tests/pattern-matching/with_expr.z42`：单/多字段覆盖 / 简写 / 表达式值 / 链式 / 嵌套 record / 原对象不变
- [x] interp + jit 双绿（`Total: 2 passed`）
- [x] 负例：with 非 record（1 error）/ with 未知字段（1 error）独立验证
- [x] 回归：record 值语义 / 对象初始化器 e2e 不受影响（clean test compiler 覆盖）

## 5. GREEN + 文档 + 落地
- [x] clean `xtask build compiler` + `build stdlib`（z42c self-build 绿）
- [x] clean `xtask test compiler`（self-build + units + 自举不动点 gen1==gen2 + **vscode-syntax gate** + 回归 全绿）
- [x] `docs/book/src/language/pattern-matching.md` 补 D 节
- [ ] PR → 盯 CI（gen1==gen2 + test-vm/stdlib-jit + bootstrap-no-csharp + vscode-syntax = 权威 GREEN）→ 合并 → 删 worktree/分支

## 备注（本次踩坑）
- **stale-zbc 假失败**：with_expr 首跑 `q.Y` 期望 99 得 2——dump-ir 证明脱糖 IR 正确（`new Point($t.X, 99)`）；
  实为 `--no-build` 用了未重建的 zbc。rm 目标 zbc 重跑即 2/2 双绿。（同 B 教训。）
- **改 xtask 脚本后须重建 xtask.zpkg**：`_kwOperatorExpr` 加 `with` 后 `deps install vscode` 仍报 ghost——
  运行的是旧 xtask.zpkg。用 fresh driver 重建 xtask.zpkg 再生成 grammar。
- **新关键字半径**：Lexer + TokenKind + vscode `_kwOperatorExpr` + 重生成 tmLanguage.json（4 处，缺一 gate 红）。
