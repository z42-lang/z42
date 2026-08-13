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

**默认可见性 = 最小封闭作用域**（default-member-private；[语言规范](../../../design/language/access-control.md)）：
无修饰符声明只对**直接封闭的那层结构**可见。

| 声明位置 | 默认可见性 | 封闭层 |
|---------|-----------|-------|
| 类的字段 / 方法 / 构造器 / 属性 / 索引器 | `private` | 类 |
| 顶层类 / 接口 / **自由函数** | `internal` | 模块（包） |

故 `_vis` / `_visCode` 的无修饰符默认**按位置**传入（成员 `private` / 顶层 `internal`；`_methodSymbol`
以 `containing==""` 区分自由函数）。

三条 C# 一致性规则消除「无强制期」遗留的欠标注：

- **override 继承基类可见性**：无显式修饰符的 `override` 视为 `public`（只能覆写 virtual/abstract 契约，
  通常 public）——否则 `override ToString()` 等被判 private，跨类调用全断。
- **record 定位字段公有**：`record R(string A, …)` 的定位字段合成为 `public`（镜像 C# record 定位参→公有属性）。
- **不允许组合修饰符**：2+ 访问修饰符（`protected internal` / `private protected`）→ `E0405`（`_parseModifiers` 拦截）。

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

## 类级访问强制（enforce-class-access，2026-08-13）

成员访问强制之外，**类型本身被引用**时也受可见性约束（镜像 C#：类型只要被「命名」就检查可访问性）。
本 change 落地**同包**类级强制：`private`/`protected` 嵌套类只能在其外层类家族内引用。违规同样 emit
`E0404`（C# 亦用单一 CS0122 覆盖成员与类型不可访问）。**跨包 `internal` 类引用强制**需类可见性进 zbc/zpkg
元数据（格式 bump），拆为独立 follow-up（见 Deferred `access-future-crosspkg-internal-class`）。

### 语义

| 类可见性 | 可引用范围 | 判据（`AccessChecker.CheckTypeRef`） |
|---------|-----------|-----------------------------------|
| `public` | 任意 | 不校验 |
| `private` 嵌套类 `Outer+Inner` | `Outer` 文本内（含更深嵌套） | `currentClass == Outer` 或以 `Outer+` 为前缀 |
| `protected` 嵌套类 | `Outer` + 派生自 `Outer` 的类 | `currentClass` 沿基链上溯能到 `Outer` |
| `internal` 类（含无修饰符顶层默认） | 同包（本 change 恒放行；跨包强制见 follow-up） | 被引类 `IsImported==false`（本包）→ OK |

嵌套类经 `NestedFlatten` 命名为扁平键 `Outer+Inner`；外层名由 `_nestedOuter`（剥 `+` 末段）得出，
无需结构标记。类可见性**位置默认**（`IrGenFacts.classVisCode/classVis`）：嵌套类→`private`、顶层类→
`internal`，显式修饰符优先（与成员级 `_vis` 同款「最小封闭作用域」）。可见性存内存态 `Z42ClassType.Visibility`
（本地类由 `SymbolCollector` 从 `Mods` 设），**不序列化**——故跨包 `internal` 类判定尚不可用（imported 类
可见性默认 `public`，internal 分支对 imported 暂不触发）。

### 强制点（两相位，静态 `CheckTypeRef` 共用）

`AccessChecker.CheckTypeRef(resolved, currentClass, symbols, diags, span)` 是**静态**方法、显式收 diag
bag，供绑定期与收集期共用（emit 到各自的 bag）。泛型实例化递归校验定义类 + 每个类型实参。

| 相位 | 位置 | 覆盖引用点 | diag bag |
|------|------|-----------|---------|
| 绑定期（体引用） | `TypeChecker._chkTypeRef` ← `StmtBinder`/`ExprTyper` | `new T` / 局部 `T x` / `(T)e` / `e is T` / `e as T` / `typeof(T)` / `default(T)` / `catch(T)` / 泛型实参 | `TypeChecker._diags` |
| 收集期（声明签名） | `SymbolCollector._chkTypeRef` ← `_fillClass`/`_methodSymbol` | 字段 / 属性 / 索引器类型 / 方法参·返回 / 基类·接口列表 | `SymbolCollector.Diags` |

codegen（`FunctionEmitter` 直呼 `symbols.ResolveTypeP`）绕过校验入口 → 不重复报、不碰字节不动点（纯诊断，
z42c/stdlib 自身无嵌套类越界引用 → `gen1==gen2` 保持）。非类类型（prim/接口 `Z42InterfaceType`/泛型形参/
未知/func）一律放行，绝不误报。

## Deferred / Future Work

### access-future-crosspkg-internal-class: 跨包 internal 类引用强制（含格式 bump）

- **来源**：enforce-class-access 拆分（2026-08-13，本地 macOS 两代自举墙阻挡格式-bump 本地 GREEN）
- **需要什么**：类可见性序列化进 zbc/zpkg 元数据（TYPE 记录紧随 `class_flags` 加一个可见性字节，`class_flags`
  u8 已占满故须独立字节）——链路 `ClassDecl.Mods → IrClassDesc.Visibility → zbc TYPE 字节 → ZbcReader →
  TsigReconcile._visStr → ExportedClassZ.Visibility → ImportedSymbolLoader → Z42ClassType.Visibility`，
  之后 `CheckTypeRef` 的 internal 分支（`IsImported && Visibility=="internal"` → E0404）即生效。**真格式
  bump**（zbc 1.32→1.33 / zpkg 0.37→0.38，非成员 internal=3 的零 bump）。
- **触发条件**：格式-bump 可在 CI（Linux）或有 0.38 nightly 后本地两代自举验证时。破坏面尽调≈0（z42c/stdlib
  导出类全 `public`）。完整设计与代码见 change `enforce-crosspkg-internal-class`（承接本 change 的 design）。

### access-future-class-inconsistent-accessibility: 不一致可访问性

- **来源**：enforce-class-access 实施期（Out of Scope）
- **触发原因**：C# CS0050–53「public 签名暴露较低可见性类型」是独立于「引用点能否命名类型」的判定；本 change
  只做后者。
- **触发条件**：需要拦「public 方法返回一个 internal 类型」这类泄漏面时。

### access-future-class-toplevel-modifier: 顶层类标 private/protected 的声明期拒绝

- C# 顶层类型只能 public/internal。当前不拒绝声明（`CheckTypeRef` 对顶层 private/protected 做了合理兜底但
  破坏面为 0）；声明合法性检查与引用强制正交，独立后续。

### access-future-interface-visibility: 接口类型可见性

- `Z42InterfaceType` 未建模 `Visibility`，故 `CheckTypeRef` 对解析为接口的引用放行。private/internal 嵌套/顶层
  接口的引用强制需给接口类型加可见性字段 + 序列化，平行于类的做法。

### access-future-class-visibility-reflection: 类可见性反射面

- VM 读 TYPE 可见性字节但 read-and-discard；`Type.IsPublic`（类级）等反射 API 未接入。

### access-future-inherited-internal-fidelity: 跨包**继承**成员的 internal 保真

- **来源**：enforce-access-control 实施期
- **触发原因**：`TsigReconcile` 对**祖先继承**成员经 `_visStr(int)` 重建；其自有成员 int 已带 3=internal
  故保真，但若某跨包基类的 internal 成员经继承链 reconcile 时上游 int 已塌缩，则可能漏判。当前主路径
  （自有成员）已正确，此为长尾边界。
- **触发条件**：出现「跨包访问经**多层继承**得到的 internal 成员未被拦」的实际用例时。
- **当前 workaround**：无（正确代码不跨包访问他包 internal，漏判仅「未报本不该写的违规」）。

### access-future-as-is-boxing: `as` / `is` 与装箱路径的访问检查

- 反射 `FieldInfo.GetValue` 对 private 字段等**运行时**绕过不在编译期强制范围。
