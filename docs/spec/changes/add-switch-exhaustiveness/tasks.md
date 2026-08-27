# Tasks: 模式匹配 C（switch 穷尽性诊断 bool/enum）

## 0. DRAFT + 6.5
- [x] proposal / design / spec 落地
- [x] User 6.5 确认（① warning 默认开 ② 仅 bool+enum，sealed out-of-scope ③ enum 按整数值比对 ④ stmt+expr 都查）
- [x] 事实校正：不用 analyzer 框架（AST 层无类型）→ 落 binder；sealed 不可行（final + 无反向子类索引）

## 1. semantics
- [x] `ExhaustCheck.z42`：`ExhaustChecker`（CheckStmt/CheckExpr + _isUncond + _collect + _constKey + _report）
- [x] `TypeChecker.z42`：`_exhaust` 字段 + ctor 实例化
- [x] `StmtBinder._bindSwitchStmt`：尾部 `_exhaust.CheckStmt(bsw, env)`
- [x] `ExprTyper._bindSwitchExpr`：尾部 `_exhaust.CheckExpr(bse, env)`
- [x] W0700 走字面量发码（不改 DiagnosticCodes.z42）

## 2. 测试
- [x] `tests/exhaust/exhaust_tests.z42`（10 例）+ `z42c.semantics.test.exhaust.z42.toml`
- [x] 10/10 PASS：enum 缺成员(stmt+expr) / enum 全覆盖·default·or-覆盖 / bool 缺 false·全覆盖 /
      守卫不计 / 开放域 int 不查
- [x] switch-stmt 测试源无 break（避最小 Infer 路径 E0410 遮蔽）

## 3. GREEN + 文档 + 落地
- [x] clean `xtask build compiler`（rm -rf artifacts/build/compiler 后自建；避增量 staleness）
- [x] clean `xtask test compiler`（self-build + units 含 exhaust + 自举不动点 gen1==gen2 + 回归 全绿 exit 0）
- [x] `docs/book/src/language/pattern-matching.md` 补 C 节（含 analyzer/sealed 两事实校正）
- [ ] PR → 盯 CI（gen1==gen2 + test-vm/stdlib-jit + bootstrap-no-csharp = 权威 GREEN）→ 合并 → 删 worktree/分支

## 备注（本次踩坑教训）
- **增量构建 staleness**：编辑 z42c.semantics 源后 `xtask build compiler` 增量缓存不可靠、可能不重编改动文件
  → 必 `rm -rf artifacts/build/compiler` 再建才可靠（本次 C 多次踩，含 SemanticDump 改动未生效）。
- **测试助手别改 SemanticDump/编译器内部**：给 SemanticDump 加 helper 会踩 bootstrap shadowing（driver 运行时
  用自身 bundled 的 Z42.Semantics 遮蔽 Z42_LIBS 的 fresh dist → E0401 找不到新方法）。改用既有 `FirstErrorCode`。
- **FirstErrorCode 最小 Infer 路径**：不链 stdlib（Std.* 报 undefined）、switch break 报 E0410——warning 用例
  须避开这些噪声（不用 Std、switch-stmt 不写 break），使 W0700 成为唯一/首条诊断。
