# Tasks: 模式匹配 B（解构声明 `Point(x, y) = p`）

## 0. DRAFT + 6.5
- [x] proposal / design / spec 落地
- [x] User 6.5 确认（① 不可失败=静态限制 irrefutable；② 仅位置形态；③ 类型测试不入白名单）

## 1. syntax
- [x] `Stmt.z42`：新增 `DeconstructDeclStmt { Pattern Pat; Expr Init }`
- [x] `StmtParser.z42`：`_isDeconstructDeclStart()`（lookahead `T(...)=`）+ `_parseDeconstructDecl()`
      （复用 `_patP._parsePrimaryPattern`）
- [x] `Parser.z42`：`ParseStatement` 在 `_isVarDeclStart` 后分派

## 2. semantics 绑定
- [x] `BoundStmt.z42`：`BoundDeconstructDeclStmt { BoundPattern Pat; BoundExpr Init }`
- [x] `StmtBinder.z42`：`_bindStmt` 分派 + `_bindDeconstructDecl`（绑进当前 env）
- [x] `PatternBinder.z42`：`CheckIrrefutable`（结构仅 wildcard/binding/nested-positional + 类型精确匹配）

## 3. semantics 发射
- [x] `PatternEmitter.z42`：`EmitIrrefutable`（无 IsInstance / 无失败分支，field_get 直读 + 递归）
- [x] `StmtEmitter.z42`：`_emitStmt` 分派

## 4. 测试
- [x] `src/tests/pattern-matching/pattern_destructure.z42`：单层 / 通配 / 嵌套 / 绑定可再解构 / 后续可见
- [x] interp + jit 双绿（`Total: 2 passed`）
- [x] 负例诊断独立验证：`Point(0,y)=p`（常量子模式→irrefutable 报错）/ `Point(x,y,z)=p`（arity 报错）；
      正例 `Point(x,y)=p` 零错
- [x] 回归：`pattern_core` / `pattern_a2` / `pattern_a3` / `pattern_is` interp+jit 双绿

## 5. GREEN + 文档 + 落地
- [x] `xtask build compiler`（fresh nightly seed；z42c self-build 绿）
- [x] `xtask build stdlib`
- [x] `xtask test compiler`（self-build exit 0；字节不动点权威验证交 CI verify-selfhost）
- [x] `docs/book/src/language/pattern-matching.md` 补 B 节 + 更新 Deferred
- [ ] PR → 盯 CI（gen1==gen2 不动点 + test-vm/stdlib-jit + bootstrap-no-csharp = 权威 GREEN）→ 合并 → 删 worktree/分支

## 备注
- **stale-zbc 假失败教训**：`xtask test e2e --file X --no-build` 若在 stdlib/compiler 尚未完全重建时跑，
  会用陈旧 zbc 报假 Null 失败。修=先 `build compiler` + `build stdlib` 完成，再 `rm` 目标 zbc 重跑。
- 本机 seed 须 post-#291（含 analyzer 类型 TextEdit/CodeFix/FixSink）→ 用今日 fresh nightly SDK。
