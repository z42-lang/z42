# Tasks: 多维索引器使用侧 `obj[a, b]`

> 状态：🟢 已完成 | 创建：2026-08-11 | 完成：2026-08-11

## 进度概览
- [x] 阶段 1: AST + Parser
- [x] 阶段 2: Typer 路由 + 合成点适配
- [x] 阶段 3: 测试 + example + 文档
- [x] 阶段 4: GREEN 验证

## 阶段 1: AST + Parser
- [x] 1.1 `Ast.z42` `IndexExpr` 改 `Expr[] Indices` + `int IndexCount`，构造器 + `Dump()`
- [x] 1.2 `ExprParser.z42` 后缀 `[` 循环解析逗号分隔下标

## 阶段 2: Typer 路由 + 合成点适配
- [x] 2.1 `ExprTyper._bindAssign` 写路由传 N 下标 + value 给 set_Item
- [x] 2.2 `ExprTyper._bindIndex` 读路由：数组取 Indices[0]（IndexCount!=1 报 E0402）；索引器传 N 下标给 get_Item
- [x] 2.3 加 helper `_ix1` 并替换 516/579(×2)/589 单下标合成点

## 阶段 3: 测试 + example + 文档
- [x] 3.1 parser_tests.z42 加 `test_index_multidim`（`m[a,b]` / `x[a,b,c]` / 复合下标 / 链式）
- [x] 3.2 codegen_tests.z42 加 `test_multidim_indexer_dispatch`（get_Item/set_Item IR）
- [x] 3.3 e2e 断言测试 `src/tests/classes/indexer_multidim.z42`（矩阵 2D + 立方 3D + 单维回归）
- [x] 3.4 `examples/indexer.z42`（单维 string 键 + 多维矩阵）
- [x] 3.5 文档同步（新写 `docs/book/src/language/member-accessors.md`（属性+索引器合并页）+ 挂 SUMMARY）

## 阶段 4: GREEN 验证
- [x] 4.1 cargo build (z42vm) 无错
- [x] 4.2 xtask test（全 stage gate）全绿 —— `✅ GREEN — all stages passed (C#-free)`
- [x] 4.3 spec 场景逐条覆盖确认（多维读/写/三维/单维回归/数组多下标 E0402）
- [x] 4.4 自举不动点 gen1==gen2 —— 5/5 packages byte-identical（纯前端无格式 bump）

## 备注
- 手工验证（fresh z42c）：多维读写 `10 15 42` 正确；IR 正确派发 `set_Item(r,c,v)`/`get_Item(r,c)`；
  数组多下标 `a[i,j]` 报 `E0402: array does not support multi-dimensional index`。
- Scope 调整：e2e 测试按实际约定改为单文件 `src/tests/classes/indexer_multidim.z42`（内联 Assert.Equal），
  非最初 proposal 写的 `src/tests/e2e/indexer_multidim/`；proposal Scope 表已同步。
