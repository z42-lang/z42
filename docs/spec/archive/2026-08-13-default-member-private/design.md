# Design: 默认成员可见性 = private

## Architecture

无修饰符默认可见性由**声明位置**决定（最小封闭作用域）：成员→`private`，顶层→`internal`。
`_vis` / `_visCode` 只见 mods 字符串，故默认值由调用点按位置传入。

```
SymbolCollector._vis(mods, dflt)      dflt = 成员 "private" / 自由函数 "internal"
IrGenFacts._visCode(mods, dflt)       dflt = 成员 1(private) / 自由函数 3(internal)
DeclParser._parseModifiers()          2+ 访问修饰符 → E0405
```

## Decisions

### Decision 1: 位置默认经参数传入，不猜测

**问题：** `_vis(mods)` 无法从 mods 得知是成员还是顶层。
**决定：** 加 `dflt` 参数。`_methodSymbol(table, m, containing, ...)` 已有 `containing`——`""` = 自由函数
（传 internal）、非空 = 成员（传 private）。字段/属性/索引器调用点恒为成员（传 private）。IrGen 的
member-emit 路径传 code 1、free-func 路径（`this._q(md.Name)`，注释「自由函数：无 this」）传 code 3。

### Decision 2: `_visCode` 同步对齐（反射/元数据一致）

**问题：** 只改 `_vis`（本地强制）够不够？
**决定：** 不够——`_visCode` 驱动 zbc/zpkg 序列化 + 反射 `IsPrivate=(vis==1)`。若成员仍序列化为 3(internal)，
反射会把「语义上 private」的无修饰符成员报成非-private，与规范不符。故 `_visCode` 成员默认也改 1(private)。
跨包访问结果不变（private 与 internal 成员跨包都拒），但反射/元数据现符合规范。**无格式 bump**（沿用 #180
的 u8 值域 0-3，仅默认值改变）。

### Decision 3: 组合修饰符在 parser 层拒绝

**问题：** `protected internal` 当前被 `_vis` first-wins 静默接受。
**决定：** `_parseModifiers` 统计访问修饰符个数，>1 → `E0405 InvalidModifier`「cannot combine access
modifiers」。规范明列此为 Phase-1 必须的编译错误。（C# 允许 `protected internal`，但 z42 规范显式简化为不允许组合。）

### Decision 4: 破坏面修正 = 补显式修饰符，非放宽默认

**问题：** 无修饰符成员改 private 后，代码库中「同包跨类访问无修饰符成员」处会 E0404。
**决定：** 逐处补**显式** `internal`（同包协作的正确 C# 标注）或 `public`（真 API）。实测破坏面极小
（stdlib 4 + xtask 脚本 4 + 少量测试/e2e fixture），印证代码库本就基本遵循 private-ish 纪律。**不**为了少改
而把默认退回 internal——那正是本 change 要纠正的偏差。

## Implementation Notes

- `_methodSymbol` 传 `(containing == "" ? "internal" : "private")`。
- override 规则（#180，无显式修饰符 override→public）在 `_vis`/`_visCode` 内位于 dflt 之前，优先级正确。
- record 定位字段（#180，`DeclParser` 合成用 "public"）不受影响。

## Testing Strategy

- 单元（access_control_tests）：无修饰符成员跨类→E0404；同类内→放行；显式 internal→放行；自由函数同包调用
  →放行；组合修饰符→E0405。
- 既有 typecheck fixture 补显式修饰符（继承字段 protected、跨类调用 public 等）。
- 自举 5/5 gen1==gen2（成员 vis 3→1 是元数据 delta，被 D7 一代吸收后收敛）。
- 完整 GREEN + cargo test（反射无修饰符成员 IsPrivate）。
