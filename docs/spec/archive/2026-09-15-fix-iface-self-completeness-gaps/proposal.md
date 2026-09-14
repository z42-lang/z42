# fix-iface-self-completeness-gaps — 接口满足性覆盖 impl 块 + `Self` 下钻 Func + 接口索引器（A2+A3）

## 背景

[`add-associated-types-program`](../../../../.claude/projects/-Users-d-s-qiu-Documents-codesigner-ui-z42-test/memory/add-associated-types-program.md)
gap 扫描 Tier A 剩 A2/A3 两项，User 裁决**一起做**（含索引器子项）。两项都是**闭合潜在洞 + 立回归门**
型（同 #604/#651），实测生产受益点均为 0——不是修活跃 bug，但都是会静默错的真洞。

## A2：impl 块补的接口不做成员齐备性校验

`impl Trait for Target` 补的接口此前**从不校验**：`_checkIfaceMembersComplete`（`_passSealedEnforce`
内、走 AST `c.Bases`）看不到 impl 块给的接口——trait 由 `_passImpls` 并进 `ct.InterfaceNames`、
**不在** target 的 `c.Bases` 里 ⇒ `impl I for C {}`（空/缺成员）静默通过、等同没实现却编得过。

## A3：`Self` 完整性两个洞

- **`Self` 藏在 `Func<…>` 里漏出**：`MemberResolver._substSelf`（调用点返回位替换）与
  `InheritanceResolver._substForIface`（满足性校验期望签名）都**不下钻 `Z42FuncType`**。⇒
  `interface IMk { Func<Self,int> Make(); }` 经接口调用结果漏出裸 `Self`；满足性把
  `Apply(Func<Self,int>)` 判成与 `Apply(Func<C,int>)` 不匹配（假红 E0412）。`_containsSelf`
  （禁令扫描）早已下钻 Func，替换侧一直漏——不对称。
- **接口索引器根本不可用**：`interface IBox { int this[int i] { get; } }` + `IBox b; b[0]` 三处缺失：
  ① 解析器不认无体 `get;`（accessor-only）→ 吞掉接口闭合 `}`、后续声明错误嵌套；②
  `MemberCollector._fillInterface` 不 lower `IndexerDecl` → 接口不进 `get_Item`；③ `_bindIndex`/
  `AssignTyper` 无 `Z42InterfaceType` 收者分支 → `b[0]` 报 `index on non-array`。

## 变更

- **A2**：新 pass `InheritanceResolver._passImplIfaceComplete`（在 `_passImpls`/`_passInheritFields`
  **之后**跑，迭代 `ImplDecl` 按 trait 名解析接口）复用 `_checkOneIfaceMembers` 校验 impl 块接口，
  与声明接口**同一口径**（成员齐备 + static/返回/可见性）。为让 impl 路径（实现方 decl 可能跨 CU）
  也能复用，`_checkOneIfaceMembers`/`_checkOneIfaceMethod` 的 `ClassDecl c` 参数改成 `name`+`Span`
  （声明路径传 `c.Name`/`c.Span`，行为逐字不变）。3 处编排点接线。
- **A3-Func**：`_substSelf` + `_substForIface` 各补 `Z42FuncType` 分支（递归形参 + 返回位），与
  `_containsSelf` 的 Func 递归面对齐。
- **A3-索引器**：① `MemberParser._parseIndexer` 区分有体/无体 accessor（镜像 `_parseProperty`）；
  ② `MemberCollector._fillInterface` 补 `IndexerDecl` → `get_Item`/`set_Item` lowering；
  ③ `ExprTyper._bindIndex`（读）+ `AssignTyper._bindAssign`（写）补 `Z42InterfaceType` 收者分支，
  返回位 `Self` 经 `_substSelf` 换成接口自身（同 #527）。

## 非目标

- 不改 MangleKey / 不回灌发射（派发键，改它撼动自举字节）。
- 泛型 impl trait（`impl IFoo<int> for C`）只按裸名解析——与 `_passImpls` 自身一致，类型实参匹配
  是跨包/泛型接口约束的独立议题（Tier B4）。
- 跨包导入接口的 kind/可见性保真仍跳过（`it.IsImported` 早退，Deferred `imported-iface-static-member-fidelity`）。
- 索引器重载（按键类型/元数区分多个 `this[...]`）不做，沿用「一类一 `get_Item`」。

## 影响面

- **零字节漂移**：纯诊断（A2/A3-Func 不回灌发射）+ 全仓零 `Func<Self>` / 零接口索引器（A3
  爆炸半径实测 0）⇒ 自举不动点 3/3 gen1==gen2。
- **无格式 bump**：复用 E0412 家族 + 既有 `get_Item`/`set_Item` 机制、无新 IR/wire。
- **无 bootstrap 越界**：解析器改的是**既有构造**（accessor-only 索引器）的 bug，z42c/stdlib 源
  无一处用接口索引器 ⇒ 上一 nightly z42c 能编当前源（`test bootstrap` 绿）。
