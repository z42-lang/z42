# Tasks: 嵌套类型（nested types）作语言特性

> 状态：🟢 GREEN（本地）| 创建：2026-07-25 | 分支：`add-nested-types`（隔离 worktree z42-xfix）
> 类型：lang（规范先行已过——见 [proposal.md](proposal.md)，User 2026-07-25 裁决）
> 占用：`compiler`+`runtime`（ACTIVE.md 已登记；split-irgen-class 关闭、make-vm-loading-lazy 阶段1 已合并）

**目标**：让 `class Outer { class Inner {} }` 的 `Inner` 成为可用类型——`Outer.Inner ni = new Outer.Inner()`
可编、`typeof(Outer.Inner)` 有真句柄、反射 `GetNestedTypes`/`GetDeclaringType`/`IsNested` + `GetMembers` 纳入。

**核心约定**：源码 `.`、元数据 FQ 名 `+`（`T.Outer+Inner`）→ 反射关系纯字符串派生 → **无 format bump、无自举分阶段**。

## 阶段 1 — 编译器（compiler 锁）
- [x] 1.1 **符号注册**：`SymbolCollector` 递归注册嵌套 `ClassDecl`（class/struct/interface/enum）为独立类型，
      FQ 名 `Ns.Outer+Inner`，任意深度；内层挂 declaring 关系（或纯靠 `+` 名派生）。
- [x] 1.2 **名解析**：类型位置的 `Outer.Inner`（`MemberAccess`/限定名）解析为嵌套 FQ 句柄；
      词法作用域上溯解析内层非限定 `Inner`（当前类嵌套 → 外层嵌套 → 本 ns 顶层 → imported）。
      注意与 namespace-qualified 名（`Std.Console`）解析隔离——先试类型链。
- [x] 1.3 **IR/TYPE 发射**：`IrGen` 把嵌套类型当独立类发 TYPE record（FQ 带 `+`，字段/方法/接口块同构，
      复用 `ClassDescBuilder`）；`new Outer.Inner()`（`ObjNew`）用 FQ 名。
- [x] 1.4 **codegen 名产出**：`Z42TypeName` 对嵌套类型产带 `+` 的 FQ 名（新分支，类似接口 `QualifyClassName`）。
- [x] 1.5 **泛型外层**（`Outer<T>.Inner`）：试做 → **卡两处，如实标 Deferred**（reflection.md）：① parser 类型
      位置不接受 `Generic<Args>.Nested`（`Box<int>` 后不消费 `.Tag`）；② 内层引用 `T` 需 generic-instantiation
      做替换（0.5.x L3-R，同 method-invoke 前置）。非静默延后——已验证到边界。

## 阶段 2 — 运行期（runtime 锁）+ stdlib
- [x] 2.1 `__type_nested_types`（`GetNestedTypes`）：扫 type registry 返 FullName 形如 `<thisFQ>+<simple>`
      且 `<simple>` 不含 `+`（直接子嵌套）的真句柄 `Std.Type[]`。
- [x] 2.2 `GetMembers` 追加嵌套类型切片（与 methods/fields/props 并列，C# `MemberTypes.NestedType`）。
- [x] 2.3 `Std.Type` 门面（z42.core/Type.z42）：`GetNestedTypes()`（→2.1 builtin）+ `GetDeclaringType()`
      （纯 stdlib：`FullName` 切最后 `+` → `Type.GetType(prefix)`）+ `IsNested`（`FullName.Contains("+")`）
      + `IsNestedPublic`/`IsNestedPrivate`（visibility 反射已有）。

## 阶段 3 — 测试 + 文档
- [x] 3.1 `src/tests/types/nested_types.z42`（e2e interp+jit）：注册/实例化/字段方法/typeof/多层嵌套/
      class·struct·interface·enum 各一/跨包 public 嵌套可见 + private 不可见/`GetNestedTypes`/
      `GetDeclaringType`/`IsNested`/`GetMembers` 含嵌套。
- [x] 3.2 z42.core 反射 [Test] 补 `GetNestedTypes` 等。
- [x] 3.3 自举不动点 `xtask test compiler` gen1==gen2 byte-identical（z42c 源不含嵌套类型 → 应零漂移）。
- [x] 3.4 文档：reflection.md（nested-types Deferred → 已落地）；类型系统页新增「嵌套类型」小节
      （语法/作用域/`.` vs `+`/scope + 真实 Deferred）；复杂流程（名解析上溯 + FQ `+` 派生）按 doc-system §5.1 补 book。

## GREEN 门禁
xtask test 全绿（e2e + cross-zpkg + stdlib + compiler 单测 + 不动点 7/7）+ cargo lib；完整 gate 以 CI 为权威。

## 决策记录（proposal D1–D5，User 照推荐定案）
- D1 `+` 分隔元数据名（无 format bump）｜D2 自举零影响｜D3 v1 scope（含 interface/enum，泛型外层试做）
- D4 C# 同款词法作用域上溯｜D5 `Name`=简单名 / `FullName`=`T.Outer+Inner`
