# 访问权限强制（Access control）

> 对齐：2026-08-12（enforce-access-control）｜ 代码：`src/compiler/z42c.semantics/src/AccessChecker.z42` + `MemberResolver.z42` / `ExprTyper.z42`

z42 的访问修饰符（`public` / `private` / `protected` / `internal`）遵循 **C# 语义**，并在编译期
**强制**：违规成员访问 emit `E0404 AccessViolation`。此前修饰符只被解析、存进符号表与 zpkg 元数据，
却**从不校验**（parsed-but-not-enforced）——本机制补齐强制层。

## 可见性语义（镜像 C#）

| 修饰符 | 可访问范围 | 判据 |
|--------|-----------|------|
| `public` | 任意位置 | 不校验 |
| `private` | **声明类文本内**（含同类其它实例；派生类**不可**访问基类 private） | `env.CurrentClass() == 声明类` |
| `protected` | 声明类 + 派生类（跨包派生同样允许） | `CurrentClass()` 沿基链上溯能到声明类 |
| `internal` | **同包**（同一编译单元 / zpkg）；跨包不可 | 声明类 `IsImported == false` |

**默认可见性**：无修饰符成员 = `internal`（`SymbolCollector._vis`）。

两条 C# 一致性规则消除「无强制期」遗留的欠标注：

- **override 继承基类可见性**：无显式修饰符的 `override` 视为 `public`（只能覆写 virtual/abstract 契约，
  通常 public）——否则 `override ToString()` 等被判 internal，跨包调用全断。
- **record 定位字段公有**：`record R(string A, …)` 的定位字段合成为 `public`（镜像 C# record 定位参→公有属性）。

## 机制 / 实现

### 强制点

访问在绑定期（TypeChecker/成员解析）解析出成员符号后，调 `AccessChecker.CheckAccess(vis, 声明类短名,
env, symbols, kind, name, span)`：

| 位置 | 形态 |
|------|------|
| `MemberResolver._bindClassMemberAccess` | 实例字段读 / 属性 getter / 方法组 |
| `MemberResolver._bindInstanceMemberCall` | 实例方法调用 |
| `MemberResolver._bindMember` | 静态字段读 |
| `MemberResolver._bindMemberCall` | 静态方法调用（`Class.m()`） |
| `ExprTyper._bindAssign` | 属性 setter（`obj.P = v`）；字段写经 `_bindClassMemberAccess` 已覆盖 |

`CheckAccess` 决策（`AccessChecker.z42`）：

```
public       → OK
private      → CurrentClass()==declClass ? OK : E0404
protected    → CurrentClass()==declClass 或派生自 declClass（沿 HasBase/BaseName 上溯，含隐式 Object）? OK : E0404
internal     → symbols.GetClass(declClass).IsImported==false ? OK : E0404
```

违规 emit `E0404` 但**不阻断绑定**（返回原 Bound 节点），使单次编译能收集多条诊断。

### 名字形式对齐

`env.CurrentClass()`（`DeclBinder` 建方法 TypeEnv 时传 `ct.Name()`）与 `FieldSymbol/MethodSymbol.
ContainingTypeName`（`SymbolCollector` 从 `c.Name` 设）**均为短名**，直接可比，无 FQN 归一。

### local vs imported：`Z42ClassType.IsImported`

`internal` 需判「声明类是否本包」。TypeChecker 期无现成 local/imported API（`SymbolTable.Classes` 混存
两者）。根因解法：`Z42ClassType` 带 `bool IsImported`，`ImportedSymbolLoader` 构造 imported 类型时置
`true`，本地类型默认 `false`。声明类自己携带来源，检查点一行 `dc.IsImported` 即判。

### 跨包 internal 无需格式 bump（关键实现事实）

成员可见性早已是 zbc/zpkg 格式里的 `u8`（`0=public/1=private/2=protected`）。给它加值 **`3=internal`**
**不改字节布局**——Rust reader 只原样携带 `u8`（无穷举 match 拒 3），反射 `IsPublic=(vis==0)` /
`IsPrivate=(vis==1)` 对 3 仍正确。故只改两个编码函数即让跨包 internal 生效，**零格式常量改动**：

- `IrGenFacts._visCode`：无修饰符 / 显式 `internal` → `3`；`override`（无显式修饰符）→ `0`。
- `TsigReconcile._visStr`：`3` → `"internal"`（跨包 TSIG 恢复 internal，供 `ImportedSymbolLoader` 还原
  成员 `Visibility`，AccessChecker 据此对 imported 声明类判 internal）。

> **反射副作用（正确化）**：无修饰符成员现 emit `vis=3` → 反射 `IsPublic` 对其返回 `false`（C# 语义下
> internal 非 public；此前 vis=0 误报 true）。

### 自举：codegen 不变、字节不动点保持

强制层纯诊断——合法程序的 Bound 树 / IR 字节不变，z42c 自举 `gen1==gen2` 逐字节保持。落地时把编译器
split 辅助类（Parser/DeclParser/MemberParser/TypeParser 等）45 处**同包互访的** `private` 改为
`internal`（无强制期的欠标注；internal 是同包协作的正确修饰符），stdlib 私有零违规。

## Deferred / Future Work

### access-future-inherited-internal-fidelity: 跨包**继承**成员的 internal 保真

- **来源**：enforce-access-control 实施期
- **触发原因**：`TsigReconcile` 对**祖先继承**成员经 `_visStr(int)` 重建；其自有成员 int 已带 3=internal
  故保真，但若某跨包基类的 internal 成员经继承链 reconcile 时上游 int 已塌缩，则可能漏判。当前主路径
  （自有成员）已正确，此为长尾边界。
- **触发条件**：出现「跨包访问经**多层继承**得到的 internal 成员未被拦」的实际用例时。
- **当前 workaround**：无（正确代码不跨包访问他包 internal，漏判仅「未报本不该写的违规」）。

### access-future-as-is-boxing: `as` / `is` 与装箱路径的访问检查

- 反射 `FieldInfo.GetValue` 对 private 字段等**运行时**绕过不在编译期强制范围。
