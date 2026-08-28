# Proposal: 基于 internal 可见性的封闭类层次穷尽性诊断 W0700

## Why

穷尽性诊断 W0700（模式匹配迭代 C，#312）目前**只覆盖 bool + enum**——它们的值集编译期完全封闭。类
层次当年被判「不可行」，根因是 z42 的 `sealed` = 叶子 final（不可继承）而非根封闭，且**开放世界**下
基于「本包可见子类集」的穷尽判定**不健全**：下游包给基类新增子类时本包编译期不可见。

**关键洞察（本变更据此设计）：z42 已有一个现成的封闭机制——`internal` 可见性。** 一个 `internal`
（或任何**非 public**：`private` / `protected`）基类，别的包连它的名字都引用不了
（`AccessChecker.CheckTypeRef` 对 internal 跨包引用报 E0404），于是：

1. 跨包**无法继承**它（派生要先能命名基类）；
2. 跨包**无法 switch** 它（switch 也要引用基类）。

⟹ **基类 + 它的全部子类 + 对它的全部 switch，必然同处一个包内**，编译该包时全部可见。所以「本包可见
的子类集 = 真实的完整子类集」，穷尽判定**健全**——上一版方案里「兄弟包偷偷加子类」的漏网场景在这里
物理上不可能发生（兄弟包命名不了 internal 基类）。

于是判据非常干净：

> **subject 的静态类型是「非 public 类」（internal/private/protected）且有子类 → 封闭世界 → 检查穷尽性；
> public 基类 → 开放世界 → 不检查。**

这**不需要引入任何新的 `[Closed]` / `permits` / `sealed hierarchy` 语言机制**——纯复用既有 `internal`
语义 + 既有跨包访问强制。**零新语法、零新关键字、零 zbc/zpkg 格式 bump。**

### 与其他语言的对应

| 语言 | 封闭机制 | 本变更等价物 |
|------|---------|-------------|
| Kotlin `sealed class` | 子类须同 module/package | **非 public 基类 = 同包** ✓ |
| Java `sealed ... permits` | 显式列表，编进 .class | （本变更不需显式列表：同包即封闭）|
| Rust `enum` / Scala 3 `sealed` | 变体内联 / 同文件 | （z42 用「基类 + record 子类」形态）|

**唯一放弃的能力**：做不了**公开的封闭 sum type**（public 基类导出给下游做穷尽匹配，如 Rust `pub enum`）。
这留作将来独立的 `[Closed]` follow-up——它与本变更**正交、可叠加**：将来给 public 轴加 `[Closed]`
标记，internal 类型的检测一行不改照样有。故现在走 internal 路线**不堵死**任何后路，是干净的子集。

## What Changes

### Part A —— 极小的语义地基（非语言特性）

- **`Z42ClassType.IsAbstract` 字段**：现无（abstract 仅在 codegen 期从 `ClassDecl.Mods` 现推
  `ClassDescBuilder.z42:229` bit0）。加一个 `bool IsAbstract`，由 `StubCollector` 从 `Mods` 回填
  （镜像 `IsSealed` 采集，`StubCollector.z42:194` 旁）。**仅本地类回填即可**——非 public 基类的子类必然
  全在本包（跨包继承不了），不经 imported 路径。用途：从「必须覆盖的具体类集」中排除不可实例化的 abstract 类。

### Part B —— W0700 扩到封闭（非 public）类层次

- **收集类型模式覆盖**：`ExhaustChecker._collect`(:68) 新增三分支，把类型模式命中的类名以 `"t:"+名` 入
  `covered`：`BoundTypePattern`（`case Circle c:`）、`BoundPositionalPattern`（`case Circle(r):`）、
  `BoundPropertyPattern`（`HasType` 时，`case Circle{R:r}:`）。用各自 resolved `Z42Type.Name()` 作 key，
  与 `Classes` 键一致。（常量/or 分支不变；bool/enum 忽略 `"t:"` 键，前缀天然隔离。）
- **上报**：`_report`(:90) 在 enum 分支后、开放域 return 前，插入封闭层次分支：
  - subject 静态类型 `tn` 经 `GetClass(tn)` 拿到 `Z42ClassType baseCt`；
  - **触发条件**：`baseCt != null && !baseCt.IsImported && baseCt.Visibility != "public" && !baseCt.IsStruct`
    且非泛型（`GenericParamCount==0 && !HasArityMangle`，泛型封闭层次 defer）；
  - 遍历 `env.Symbols.Classes`，求 subtree（`cn==tn || IsSubclassOf(cn, tn)`）里的**具体（非 abstract）
    本地类**集合 = 必须覆盖集；同时统计严格子类数 `subCount`；
  - **仅当 `subCount >= 1`**（确是层次、非孤立类）才判：必须覆盖集里每个类 `C`，若无任一 `"t:"+T`
    覆盖它（`C==T || IsSubclassOf(C, T)`——`case Base b:`/`case AbstractMid:` 吞整棵子树），则缺失；
  - 缺失非空 → `_tc._diags.Warning("W0700", …)` 列出缺失类名。
- **保守边界**：仅在能证明封闭（非 public 基类）时报，否则沿用开放域不检查。守卫臂不计覆盖
  （`CheckStmt`/`CheckExpr` 已排除）；裸绑定/通配仍是无条件兜底（`_isUncond` 不变）→ 有兜底即穷尽。
- **无 zbc/zpkg 格式 bump**：`IsAbstract` 是 semantics 内存标志；封闭性判定纯用既有 `Visibility`（已持久化、
  已跨包强制）+ 本包 `IsSubclassOf`，不新增任何持久化位。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42ClassType` 加 `bool IsAbstract`（构造器默认 false）|
| `src/compiler/z42c.semantics/src/StubCollector.z42` | MODIFY | `ct.IsAbstract = _sc._hasWord(c.Mods, "abstract")`（镜像 :194 IsSealed）|
| `src/compiler/z42c.semantics/src/ExhaustCheck.z42` | MODIFY | `_collect` 加类型模式覆盖；`_report` 加封闭层次穷尽分支 + `_isTypeCovered` 助手 |
| `src/compiler/z42c.semantics/tests/analyzer/analyzer_tests.z42` | MODIFY | `SemanticDump.FirstErrorCode`/warning 单测：封闭层次穷尽（无 W0700）/漏子类（W0700）；public 基类不报（不用 Std、switch 不写 break 避噪声）|
| `src/tests/pattern-matching/pattern_exhaust_sealed.z42` | NEW | e2e：internal 封闭层次 switch 穷尽（无 warning）/漏子类；jit 双验 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 穷尽性从 bool/enum 扩到「非 public 封闭类层次」；记录 internal=封闭 的机制、健全性论证、public 轴 defer |

**只读引用**（理解上下文必须读，不修改）：
- `src/compiler/z42c.semantics/src/ExhaustCheck.z42` — 全文（bool/enum 现算法、`_isUncond`/`_collect`/`_report`）
- `src/compiler/z42c.semantics/src/SymbolTable.z42` — `IsSubclassOf`(:73)、`Classes`(:10)、`GetClass`(:66)
- `src/compiler/z42c.semantics/src/BoundPattern.z42` — `BoundTypePattern.BoundType`(:30)/`BoundPositionalPattern.Type`(:52)/`BoundPropertyPattern`(:70)
- `src/compiler/z42c.semantics/src/Z42Type.z42` — `IsSealed`(:61)/`Visibility`(:70)/`IsImported`(:65) 采集范式
- `src/compiler/z42c.semantics/src/StubCollector.z42` — `IsSealed` 回填(:194)

## Out of Scope

- **公开（public）封闭 sum type**：留作将来独立 `[Closed]` follow-up（正交、可叠加）。
- **接口层次穷尽性**：internal 接口的实现者集也封闭，但枚举实现者 + 覆盖判定更复杂 → v1 只做 class 基类，defer。
- **泛型封闭层次**：泛型基类（`Base<T>`）defer（对齐 #318 泛型解构逐步放开的节奏）。
- 运行期封闭性强制（本变更纯编译期诊断，`internal` 强制早已由访问控制落地）。
- ADT/enum-with-payload：用「非 public 基类 + record 子类」已达等价 sum type，不引入新 ADT 语法。

## Open Questions

- [x] 封闭标记形态 → **复用 `internal` 可见性**（User 裁决：不引入新机制）。
- [x] 跨包健全性 → internal 基类的 switch/子类必同包，**天然健全、无需持久化**（不同于 public 需存 zpkg）。
- [ ] **abstract 具体类判定**：required 集只数「非 abstract 本地类」，需 `IsAbstract` 可靠回填。本地
  StubCollector 回填覆盖所有本地类；imported 类不回填（非 public 基类无 imported 子类，不影响）。design 已坐实。
- [ ] **触发的误报风险**：对「非 public 基类 + 有子类 + switch 无兜底 + 漏子类」报 W0700。z42c 源零 switch
  → 无自噪声；stdlib/tests 需 IMPL 后 grep 现有 switch 确认无非预期触发（有则加 `_` 兜底或属预期 warning）。
