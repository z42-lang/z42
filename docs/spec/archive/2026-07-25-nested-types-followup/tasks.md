# Tasks: 嵌套类型补完——base/接口为兄弟或祖先嵌套类型

> 状态：🟢 GREEN（本地）| 创建：2026-07-25 | 分支：`nested-types-followup`（隔离 worktree z42-xfix）
> 类型：fix/feat（compiler，续 add-nested-types）| 占用：`compiler`

**目标**：嵌套类型的 base 或实现的接口若为**兄弟 / 祖先嵌套类型**（源码写裸名 `Base` 或限定 `Outer.Base`），
正确限定到 `+` 全名——否则继承字段 / 上转型断链、嵌套接口被误判为基类（add-nested-types v1 的 Deferred）。

**根因**：stub 的 iface-vs-base 判别（`_passClassStubs` 查 `HasInterface(裸名)`）与 IrGen 的 base_name
发射（`ClassDescBuilder` 读 AST `c.Bases`）**都读 AST 裸名**——嵌套 base/接口注册为 `Outer+Base`/`Outer+IShow`，
裸名 `Base`/`IShow` 查不到 → base 被误当顶层（`_qClass` 加 ns → `NB.Base` 不存在，运行期继承断裂、字段读 Null）
或嵌套接口被误判为基类。

**修复（单点，NestedFlatten AST 改写）**：`NestedFlatten` 在展平后、收集完全部嵌套名的前提下，把每个嵌套类型
的 base/接口 TypeExpr 名**改写为 `+` 全名**——裸 `Base` 沿 enclosing `+` 链上溯试 `<p>+Base`；限定 `Outer.Base`
转 `Outer+Base`（仅当命中某已展平嵌套类型时）。stub 判别与 IrGen 发射遂都读到限定名，一处根治。支持前向引用
（先收集全部嵌套名再改写）。z42c/stdlib 源无嵌套类型 → `_ec==0` 不改写 → 自举零扰动。

- [x] 1.1 `NestedFlatten._rewriteNestedBases` + `_qualifyBaseName` + `_isNested`/`_enclPrefix`；`_push` 带 enclosing 前缀
- [x] 1.2 验证：base 兄弟裸名（继承字段+虚派发+上转型）、限定名基类 `Inh.Base`、兄弟嵌套接口 impl（上转型+GetInterfaces）
- [x] 1.3 e2e `nested_types.z42` 追加 Inh.{Base/Derived/QualDerived/Pinger} 用例（interp+jit）
- [x] 1.4 自举不动点 gen1==gen2 byte-identical
- [x] 1.5 文档：reflection.md / nested-types.md 的 base-as-nested Deferred → 已落地
- [x] 归档 add-nested-types（release compiler+runtime 锁）

## 剩余 Deferred（未变）
- 泛型外层 `Outer<T>.Inner`（parser `Generic<Args>.Nested` 语法 + generic-instantiation，0.5.x）
- 跨包**限定名**引用嵌套（`geo.Shape.Corner`）——当前解析包内 `Outer.Inner`
- 嵌套 partial（E0435 保留）
