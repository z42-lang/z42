# Tasks: add-property-getter

> 状态：🟢 已完成 | 创建：2026-08-18 | 完成：2026-08-18

## 进度概览
- [x] 阶段 1: AST + Parser
- [x] 阶段 2: 语义 + Codegen（5 处）
- [x] 阶段 3: 测试 + 验证 + 单-PR 可行性实测
- [x] 阶段 4: 文档 + 归档

## 阶段 1: AST + Parser
- [x] 1.1 `Decl.z42` PropertyDecl 加 `HasGetBody` + `GetBody`（post-construction 默认 false/null）
- [x] 1.2 `MemberParser.z42` `_parseProperty`：`get` 后遇 `{` → `_parseBlock()` 捕获，置 PropertyDecl

## 阶段 2: 语义 + Codegen
- [x] 2.1 `DeclBinder.z42` 新增 PropertyDecl 分支：`HasGetBody` → 绑 getter body 到 `<Class>.get_<Name>`
- [x] 2.2 `IrGen.z42` PropertyDecl 分支加 `HasGetBody`：`FunctionEmitter.EmitFunction` 编译真实 get_X
- [x] 2.3 `SymbolCollector.z42`：`HasGetBody` 时不合成 `__prop_X` own-field
- [x] 2.4 `ClassDescBuilder.z42`：`HasGetBody` 时不合成 `__prop_X` 运行时 field

## 阶段 3: 测试 + 验证
- [x] 3.1 正向验证：fresh z42c 编计算属性 → 运行返回 getter 计算值（非 null）——computed_property golden OK
- [x] 3.2 e2e golden `src/tests/types/computed_property.z42`
- [x] 3.3 parser/AST golden `decl_tests.z42` `test_computed_property_getter`（+ `PropertyDecl.Dump` 更新）
- [x] 3.4 `xtask test` 全绿 + 自举 5/5 gen1==gen2
- [x] 3.5 单-PR 可行性实测：**定论 = 2-PR**（种子 z42c 硬 parse 报错 stdlib 计算属性 →
      `_ensureBootstrapZ42Ir` build z42.core failed；support 必须先行发 nightly，use 晚一 nightly）

## 阶段 4: 文档 + 归档
- [x] 4.1 `docs/book/src/language/member-accessors.md`（计算 getter 节；lowering；无 backing field）
- [x] 4.2 归档 + PR

## 备注
- 复用 indexer body-getter 流水线（Decision 1）；get-only（Decision 4）。
- bootstrap：本 PR 不在 z42c 源 / stdlib 用新语法（support only）。
