# Proposal: 泛型表达力 —— 跨包约束持久化 + `Self` + 关联类型

> 类型：lang + ir（完整流程：DRAFT → User 确认 → IMPL → GREEN → COMMIT）
> 创建：2026-09-05 ｜ 范围裁决：2026-09-06

## Why

User 要求：「参考 Rust 和 C# 的泛型约束，截长补短，吸取优秀的设计」「开始推进关联类型，并对目前需要的地方进行应用」。

前置已完成：change `complete-where-constraints`（#475 / #478 / #482）把**校验**补齐——编译期与
运行期 `validate_type_arg_constraint` 的七项对齐，27 条负例单测守着。那轮**完全没动表达力**，
所以现在是谈「加什么」的干净基线。

## 先摆证据：全仓真实约束只有 4 条，且形态高度集中

不靠印象，实测（`grep` 全仓 `src/libraries` + `src/compiler`，去掉测试与注释噪声）：

| 约束 | 出现次数 | 位置 |
|---|---|---|
| `where TKey : IEquatable<TKey>` | 2 | `Dictionary` / `DictionaryEnumerator` |
| `where T : IEquatable<T>` | 2 | 泛型集合辅助 |
| `interface INumber<T> where T : INumber<T>` | 1 | `Protocols/INumber.z42`（声明处） |

**4/4 条真实约束全是同一形态：`X<T> where T : Something<T>`（F-bounded 自引用）。**
关联类型的直接受益形态「双型参、其一可由另一推出」全仓只有 `Dictionary<TKey, TValue>`，
而它的 `TValue` 真正独立、推不出来 ⇒ **关联类型今天零个真实受益点，`Self` 命中 100%**。

这个证据与 User 的初始预期不同，已在裁决前完整呈报。**User 裁决：A（`Self`）与 B（关联类型）
都做，并同时偿还跨包持久化的债**（见下）。

## What Changes（三条链路，拆三个 PR）

### PR-1 —— 跨包 where 约束持久化 + ZbcWriter 置全 flag 位（纯欠债）

今天**跨包泛型实例化的 where 约束 100% 不校验**——不只是新补的几项，连 base-class / `class` /
`struct` / 型参引用也一样不查。实测确认这不是两件事而是**同一条链路的上下游**：zpkg 早已删掉
TSIG 段（`drop-tsig-expt`），`ExportedClassZ` **没有 wire 表示**，是由 `TsigReconcile.Rebuild`
从 zbc `TYPE` + `SIGS` 重建的。约束的真实通道是 zbc TYPE 段，而**那里七个 bit 位早已规约完毕、
三方 reader（Rust `type_reader.rs` / `ZbcReader` / `ZpkgReader`）都已按完整布局消费** ⇒ **零格式 bump**。

断链在六个上游环节，本 PR 逐一接通：

| 环节 | 现状 | 本 PR |
|---|---|---|
| `ClassDescBuilder.z42:278-317` | special 约束（class/struct/enum/new()）**整个丢弃**；base 与 interface 混塞进同一 `Interfaces` 数组 | 分开承载，special 不再丢 |
| `IrConstraintDesc`（`IrModule.z42:76-82`）| 只有 2 个承载位，语义侧有 8 类 | 加 5 个承载位 |
| `ZbcWriter.z42:280-298` | **只写 bit3**，bit0/1/2/4/5 不写 → 运行期五个校验分支是死代码 | 写全位，接活运行期 |
| `ZbcReader.z42:459-476` | bit2 base **读而不存**（注释已留「做 PR-3 时从这里接」） | 存进新承载位 |
| `TsigReconcile.z42:508-523` | `cd.TypeParamConstraints` **一次都没读过** | 读出并搬进 `ExportedClassZ` |
| `ExportedClassZ` / `ImportedSymbolLoader` | 无约束字段 / 无约束容器 | 加字段 + seed 进 `SymbolTable.ClassConstraints` |

**同 PR 内先落一个认知修正 commit**（详见 design D1）：三处规范文档与构建机制已脱节 13 天以上，
且 `ImportedSymbolLoader.z42:92-94` 的过时禁忌**直接阻碍本 PR 实施**——必须先改对再动代码。
连带把 `xtask test bootstrap` 的 A 路径接上破环预建，消掉它对 z42.ir API 面的「当天红次日自愈」假阳性。

### PR-2 —— `Self` 类型（仅接口）

接口内可写 `Self` 指代「实现该接口的具体类型」；`class Int32 : INumber` 时 `Self` 绑到 `Int32`。
约束侧 `where T : IEquatable` 等价于今天的 `where T : IEquatable<T>`。

```z42
// 今天（C# 11 的 TSelf 模式）              // PR-2 之后
interface INumber<T> where T : INumber<T> {   interface INumber {
    static abstract T op_Add(T a, T b);           static abstract Self op_Add(Self a, Self b);
}                                             }
class Int32 : INumber<Int32> { ... }          class Int32 : INumber { ... }
class Dictionary<K,V> where K : IEquatable<K> class Dictionary<K,V> where K : IEquatable
```

顺带**在语法层关掉 Deferred `where-constraint-future-type-arg-matching`**：主流用法不写类型实参，
`IEquatable<string>` 误满足 `where T : IEquatable<T>` 这个 bug 类别消失——比实现「类型实参精确
匹配算法」（要处理 F-bounded 递归）便宜一个数量级。

`Self` 在语法层几乎免费：全仓零占用，`TypeParser._parseType:96-97` 无条件按 `.Text` 取类型名，
今天已能解析成 `NamedType("Self")`。工作全在语义层。

### PR-3 —— 关联类型 `type Item;` + `where T : IEnumerable<Item = U>`

**结构性前置（实测发现，proposal 初稿低估）**：`Z42InstantiatedType.Def` 的静态类型是
`Z42ClassType`（`Z42Type.z42:324`），**只能承载泛型 class/struct 的实例化，不能表示实例化的
泛型接口**；`Z42InterfaceType` 连型参名字段都没有。⇒ `where T : IEnumerable<Item = U>` 在
今天的类型模型里无处落脚，本 PR 必须先补「泛型接口实例化」这块地基。

绑定的跨包持久化**复用 PR-1 建好的通道**——这正是把 PR-1 排在最前的原因。

## Scope（允许改动的文件）

### PR-1：跨包约束持久化 + flag 位 + 认知修正

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 删过时禁忌论证；seed 导入类型的约束进符号表 |
| `docs/design/compiler/self-hosting.md` | MODIFY | 删 :219 已失效的 warm-skip 描述（与 :235-245 自相矛盾） |
| `.claude/rules/bootstrap-seed.md` | MODIFY | 旧函数名订正；轴④判据补「已有包的新 API 由预建自动破环」 |
| `scripts/build/xtask_compiler.z42` | MODIFY | `_ensureBootstrapSelfDepLibs` 参数化目标 libs 目录 |
| `scripts/build/xtask_bootstrap_check.z42` | MODIFY | A 路径接上破环预建，消假阳性 |
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | `IrConstraintDesc` 加 5 个承载位 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 约束 bundle 写全 bit0/1/2/4/5 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | bit2 base 由「读而不存」改为存入承载位 |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | `ExportedClassZ` 加约束字段（构造后赋值，ctor 元数不变） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `_rebuildClass` 读 `cd.TypeParamConstraints` → `ExportedClassZ` |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | special 约束不再丢弃；base 与 interface 分开承载 |
| `src/compiler/z42c.semantics/src/GenericConstraint.z42` | MODIFY | 从导入元数据构造 `ConstraintSet` 的入口 |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | 键规则统一；导入约束接入 `Check` |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | `ClassConstraints` 键规则与 `Classes` 对齐 |
| `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42` | MODIFY | 新增本包正/负例 |
| `src/tests/cross-zpkg/generic_constraint_cross_pkg/` | NEW | 跨包约束校验端到端用例 |
| `docs/book/src/language/generic-constraints.md` | MODIFY | 已知限制 §1 由「不校验」改为「已校验」；补链路机制 |
| `docs/roadmap.md` | MODIFY | 关掉 `where-constraint-future-crosspkg` / `-runtime-flags` |

### PR-2：`Self`（仅接口）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42InterfaceType` 加型参名与 `Self` 绑定槽 |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | `ResolveTypeP` 加 `Self` 分支 |
| `src/compiler/z42c.semantics/src/TypeEnv.z42` | MODIFY | 透传「当前所属接口」上下文 |
| `src/compiler/z42c.semantics/src/MemberCollector.z42` | MODIFY | `_fillInterface` 建立 `Self` 绑定 |
| `src/compiler/z42c.semantics/src/StubCollector.z42` | MODIFY | 类实现接口时把 `Self` 绑到实现类型 |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | `where T : IEquatable` 的 `Self` 等价展开 |
| `src/compiler/z42c.semantics/src/ClassExtractor.z42` | MODIFY | 跨包导出时 `Self` 的编码 |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | 导入侧 `Self` 解码 |
| `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42` | MODIFY | `Self` 正/负例 |
| `src/tests/cross-zpkg/self_type_cross_pkg/` | NEW | 跨包 `Self` 端到端用例 |
| `docs/book/src/language/generic-constraints.md` | MODIFY | `Self` 语义与作用域（仅接口） |
| `docs/roadmap.md` | MODIFY | 关掉 `where-constraint-future-type-arg-matching` |

### PR-3：关联类型

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42c.syntax/src/MemberParser.z42` | MODIFY | `_parseType()` 之前拦截 `type Item;`（上下文关键字） |
| `src/libraries/z42c.syntax/src/TypeParser.z42` | MODIFY | 类型实参位支持 `Name = Type` 命名绑定 |
| `src/libraries/z42c.syntax/src/TypeExpr.z42` | MODIFY | `NamedType` 承载命名绑定 |
| `src/libraries/z42c.syntax/src/Decl.z42` | MODIFY | 新增关联类型声明节点 |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | 泛型接口实例化（地基）+ 关联类型槽 |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | 泛型接口解析 |
| `src/compiler/z42c.semantics/src/MemberCollector.z42` | MODIFY | 收集接口的关联类型声明 |
| `src/compiler/z42c.semantics/src/GenericConstraint.z42` | MODIFY | `ConstraintBundle` 承载关联类型绑定 |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | 绑定的解析与校验 |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | 绑定进 `IrConstraintDesc` |
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | `IrConstraintDesc` 承载绑定 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 绑定写入（可能需 bit7 或新块 → 见 design D6） |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | 绑定读出 |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | `ExportedInterfaceZ` 承载关联类型名单 |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | 关联类型的重建 |
| `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42` | MODIFY | 关联类型正/负例 |
| `src/tests/cross-zpkg/assoc_type_cross_pkg/` | NEW | 跨包关联类型端到端用例 |
| `docs/book/src/language/generic-constraints.md` | MODIFY | 关联类型语义 |
| `docs/design/language/generics.md` | MODIFY | L3-G3a 由 ❌ 未实现改为已实现 |
| `docs/roadmap.md` | MODIFY | 关掉 L3-G3a |

**只读引用**（理解上下文必须读，不修改）：

- `src/runtime/src/corelib/reflection/generics.rs` — 判定 SoT（七项校验的运行期实现）
- `src/runtime/src/metadata/zbc_reader/type_reader.rs` — 约束 bundle 的 bit 位权威定义
- `src/libraries/z42.ir/src/ZpkgReader.z42` — `_skipConstraintBundle` 的布局口径参照
- `src/libraries/z42.core/src/Protocols/INumber.z42` — F-bounded + `static abstract` 样板
- `src/compiler/z42c.semantics/src/BuiltinTypeDefs.z42` — 内建 prelude 的 INumber 复刻
- `docs/spec/archive/2026-09-05-complete-where-constraints/design.md` — 前一轮的 D1 裸名匹配决策

## Out of Scope

- **不改写真实源码使用新语法**（`INumber` / `Dictionary` / `Protocols` 仍用旧写法）——
  support 与 use 必须跨两个 nightly，见下节。测试用例不受此限（由自建 z42c 编译）。
- 不做 nullable 类型体系（C# `notnull` 的前提，User 已暂缓）
- 不做 LINQ / iterator chain
- 不做「类实现接口时的成员签名齐备性校验」（`InheritanceResolver.z42:14` 自陈「留待后续」）——
  独立立项，见 design 的 Deferred
- 不动 `xtask test bootstrap` 该守什么的整体重设（只做「接上预建」这一处，见 design D2）

## 🔴 硬约束：support 与 use 必须跨两个 nightly

新语法（`Self` / `type Item;`）一旦落地，**z42c 源 / xtask 源不得在同一 release 使用它**
（[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 轴①）——否则上一版 nightly
编不了当前 main，跨版本自举断链。⇒ User 说的「对目前需要的地方进行应用」拆成两个 release，
本轮只落 support。**User 已确认接受。**

> **注意区分**：这条铁律约束的是**新语法**（轴①）。**stdlib API 面（轴③）不再是障碍**——
> `_ensureBootstrapSelfDepLibs` 的破环预建已让「z42c 源用当前源 z42.ir 新 API」成为日常操作
> （最近一次先例 `04719bbb`，2026-09-05）。这是 PR-1 得以完整落地的关键事实，详见 design D1。

## Open Questions

全部已由 User 裁决（2026-09-06），无未决项：

- [x] 选 A、B 还是 A+B？ → **A + B 都做，拆三个 PR**
- [x] `Self` 在类里是否也可用？ → **仅接口**
- [x] 本轮范围？ → **只落 support + 测试，并同时还跨包持久化的债**
- [x] 三处过时规范文档如何处理？ → **并入 PR-1**
- [x] `xtask test bootstrap` 的假阳性？ → **顺手修：让 A 路径也走预建**
- [x] 关联类型绑定由实现方显式声明还是推断？ → 见 design D5（显式声明；理由随 design 一并确认）
- [x] zbc 格式 bump 时机？ → **PR-1 零 bump**；PR-3 见 design D6
