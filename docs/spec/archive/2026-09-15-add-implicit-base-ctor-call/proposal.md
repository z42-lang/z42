# Proposal: 实例构造器隐式调用基类构造器（对齐 C#）

> 变更类型：`lang`（完整流程）｜ 创建：2026-09-15 ｜ 来源：「推进 ctor 静默 bug」④（#650 实测发现）｜ User 裁决：参考 C# 行为 ｜ 叠在 #650 之上

## Why

z42 现行模型「**不自动调用基类构造器**」（`language-overview.md` §6.3），外加「无显式 ctor 的类由编译器合成
ctor，内联**本编译单元内**祖先链的字段初始化器」。实测（#650 构建，interp）这让基类的初始化**大面积静默丢失**：

| 源 | 实际 | C# |
|---|---|---|
| `class B { int b = 7; }` `class D : B { D(int x) { } }` → `new D(3).b` | `0` | `7` |
| `class W { int w; W() { w = 5; } }` `class D3 : W { }` → `new D3().w` | `0` | `5` |
| 同上 `class D5 : W { D5() { } }` → `new D5().w` | `0` | `5` |
| 基类 `B { int b = 7; }` 在**依赖包**，`class D1 : B { }` → `new D1().b` | `0` | `7` |
| 同上，派生类另有 `int d = 1;`（有合成 ctor）| `0` | `7` |

最后两行说明「合成 ctor 内联祖先初始化器」只在基类与派生类处于**同一编译单元**时成立（祖先链用的是当前 CU 的
类表），同包跨文件、跨包都会丢。而基类没有 ctor 可写 `: base()` 时，用户**无法补救**。

## What Changes

对齐 C# 的「基类构造器一定被调用」，并按 User 要求**减少派生类的样板代码**——采用 Swift 的构造器继承规则：

1. 写了实例构造器但没写初始化子句 ⇒ 隐式 `: base()`。`: this(..)` 委托的构造器不隐式调用（由被委托者调用）。
2. **没写任何实例构造器的类，继承基类全部可访问（非 private）的实例构造器**：每个继承来的构造器形参与基类那个
   相同（含默认值、`params`），体 = 本类字段 / auto 属性初始化器 + `base(同样的实参)`。
   - 基类本身没有构造器 ⇒ 退化为默认构造器（本类初始化器；本类也没有初始化器则不生成）。
   - 继承是传递的：基类自己继承来的构造器也会被继续继承。
   - 派生类只要写了**任何一个**实例构造器，就不再继承（此时按第 1 条）。
   - 例：`class MyErr : Exception { }` 直接可用 `new MyErr("msg")`、`new MyErr("msg", inner)`。
3. 隐式或显式 `base()` 找不到可零实参调用的基类实例构造器 ⇒ **编译错误**（新专用码）。由于第 2 条，只会发生在
   「派生类写了构造器却没写 `: base(args)`」这一种情况。
4. 继承来的 / 默认的构造器是**符号层事实**：注册为普通 `MethodSymbol` 并随 TSIG 导出，跨包派生类看得见、调得到。
   不再内联祖先链的字段初始化器（基类构造器自己会跑）。
5. 执行顺序不变（#650 已订正文档）：本类初始化器 → 基类构造器 → 本构造器体。

### 影响面普查（2026-09-15，临时编译器补丁 + 全量 `xtask test` + xtask 脚本，去重 601 处「无初始化子句的实例 ctor / 无 ctor 派生类」）

| 类别 | 处数 | 改后 |
|---|---|---|
| 基类有可零实参调用的实例 ctor | **0** | 基类 ctor 体开始执行 |
| 基类只有带实参的 ctor | **2**（均 `src/tests`：`basic/inherited_fields`、`inheritance/inheritance`，`Dog : Animal` 写了 ctor 手工赋继承字段） | 编译错误 ⇒ 测试补 `: base(..)` |
| 基类（链）无 ctor | 595（其中派生类无 ctor 的 87 处——**继承规则下这些派生类不会多出任何构造器**，因为基类无 ctor 可继承） | 仅当基类链有实例初始化器时行为变化——生产代码（stdlib / 编译器 / xtask）**0 处**有；测试里 3 个文件有，但均与派生类同 CU，结果不变 |
| 基类解析失败（编译器单测里故意的坏源） | 4 | 无变化 |

⇒ stdlib 与编译器**零行为变化**，字节漂移预计仅限 TSIG 新增合成 ctor 条目（无格式变更）。

## Scope（允许改动的文件）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/compiler/z42c.semantics/src/MemberCollector.z42` / `InheritanceResolver.z42`（或新 pass 文件） | MODIFY/NEW | 合成默认构造器符号（基类先于派生类） |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 隐式 `base()` 注入；合成 ctor 体只含本类初始化器 + `base()`；删祖先链内联 |
| `src/compiler/z42c.semantics/src/IrGenTypeEmitter.z42` | MODIFY | 合成 ctor 发射判据与符号层一致 |
| `src/compiler/z42c.semantics/src/ExportedTypeExtractor.z42` / `ClassExtractor.z42` | MODIFY | 合成 ctor 进 TSIG（若现有提取不自动覆盖） |
| `src/libraries/z42c.core/src/DiagnosticCodes.z42` | MODIFY（若新增码） | 见 design D4 |
| `src/compiler/z42c.semantics/tests/**` | MODIFY | 单测：隐式 base / 报错 / 合成 ctor 导出 |
| `src/tests/classes/**`、`src/tests/cross-zpkg/implicit_base_ctor_cross_pkg/` | NEW | 回归 |
| `src/tests/basic/inherited_fields.z42`、`src/tests/inheritance/inheritance.z42` | MODIFY | 补 `: base(..)` |
| `docs/book/src/language/constructors.md` | MODIFY | 规则改写，删「已知缺口」 |
| `docs/design/language/language-overview.md` | MODIFY | §6.3 旧模型两条改为指向 book |

## Out of Scope

- 主构造器转发基类实参 `class D(m) : B(m)`（C#-ism 清单另项）。
- 访问性（`private` 基类 ctor 不可被派生类调用）——z42 ctor 可见性检查另议，本变更只看「存在且可零实参调用」。
- 依赖包版本 skew（编译时基类无 ctor、运行时有）——与所有编译期决议同类，不在本变更处理。

## Open Questions

1. ~~`Std.Exception` 无参构造器~~ ——已裁决（2026-09-15）：**不补**。`class MyErr : Exception { }` 可 `new MyErr("msg")`，
   `new MyErr()` 报 E0426（Exception 没有无参 ctor 可继承），保持现有 API。
2. ~~报错码~~ ——已裁决：新增专用码。
3. 派生类**既想加自己的构造器、又想保留继承来的**（C++ `using Base::Base;`）——本变更不做，需要时另立（可做成
   显式标注，如 `[InheritConstructors]`）。
