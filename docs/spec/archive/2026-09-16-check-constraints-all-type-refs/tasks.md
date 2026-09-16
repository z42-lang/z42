# tasks — check-constraints-all-type-refs

状态图例：⚪ 未开始 / 🟡 进行中 / 🟢 完成

## 1. 探针 / 去风险
- 🟢 1.1 链路核实（H1：`_chkTypeRef` 不调 Check；H2：`_checkBundle` 不递归）—— 亲自走链路确认
- 🟢 1.2 体内位+H2 探针：harness 能报错（阳性对照 line4+line8）+ 全仓零命中（stdlib+compiler gen2）
- 🟢 1.3 声明位探针：各位置精确报（字段/属性/参数/返回/嵌套）+ D2 不双报 + 全量零爆炸半径

## 2. 实现
- 🟢 2.1 H2：`ConstraintChecker.Check` 开头无条件递归进类型实参（在 HasConstraints 早退前）
- 🟢 2.2 体内位：`TypeChecker._chkTypeRef` 接 `ConstraintChecker.Check`
- 🟢 2.3 D2：`ConstructTyper` 的 new 显式 Check 改为仅 target-typed（`n.Type==null`）触发，避双报
- 🟢 2.4 声明位：新 pass `ConstraintChecker.CheckDeclTypeRefs`（字段/属性/索引器/形参·返回/基类
  + 自由函数），走 AST 自有声明 + 镜像 SymbolCollector 类型上下文；接进 `TypeChecker.Infer`

## 3. 测试
- 🟢 3.1 单元门 `constraint_typeref_tests.z42`（体内/cast/字段/属性/参数/返回/自由函数/H2×2 +
  型参转发不误报 + 正例对照）
- 🟢 3.2 退回对照：同时禁用三处 hook 重建 → 9 条负例全 FAIL（body/decl/H2 各位置）、5 条正例全 PASS
  ⇒ 门有判别力非空门；恢复后 test compiler 全绿
- 🟢 3.3 完整 GREEN（interp）✔ + `cross-zpkg --mode jit` 58/0 + `test bootstrap` NO violation +
  `stdlib --mode jit`（http_server_threaded 首跑 flake=并发争端口，隔离重跑全绿）
- 🟢 3.4 自举不动点 3/3 gen1==gen2

## 4. 文档
- 🟢 4.1 `generic-constraints.md`「校验发生在哪里」表：新增「类型引用位」行 + 两条 choke point + H2 机制
- 🟢 4.2 归档（changes→archive + 本 tasks 全 🟢）

## 5. 落地
- 🟡 5.1 commit（feat(compiler)）+ PR（body/验证/页脚三段）
- ⚪ 5.2 合并后删分支 + worktree
