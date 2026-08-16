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
（本地类由 `SymbolCollector` 从 `Mods` 设，imported 类由 `ImportedSymbolLoader` 从 zbc 可见性字节还原）。
**跨包 internal 类强制已落地**（#184）：可见性经 zbc 1.33/zpkg 0.38 TYPE 记录可见性字节序列化（链路
`IrClassDesc.Visibility → ZbcWriter → ZbcReader → TsigReconcile._visStr → ExportedClassZ.Visibility →
ImportedSymbolLoader → Z42ClassType.Visibility`），故 imported internal 类的 `CheckTypeRef` internal 分支
（`IsImported` → E0404 `from another package`）现生效。

### 强制点（两相位，静态 `CheckTypeRef` 共用）

`AccessChecker.CheckTypeRef(resolved, currentClass, symbols, diags, span)` 是**静态**方法、显式收 diag
bag，供绑定期与收集期共用（emit 到各自的 bag）。泛型实例化递归校验定义类 + 每个类型实参。

| 相位 | 位置 | 覆盖引用点 | diag bag |
|------|------|-----------|---------|
| 绑定期（体引用） | `TypeChecker._chkTypeRef` ← `StmtBinder`/`ExprTyper` | `new T` / 局部 `T x` / `(T)e` / `e is T` / `e as T` / `typeof(T)` / `default(T)` / `catch(T)` / 泛型实参 | `TypeChecker._diags` |
| 收集期（声明签名） | `SymbolCollector._chkTypeRef` ← `_fillClass`/`_methodSymbol` | 字段 / 属性 / 索引器类型 / 方法参·返回 / 基类·接口列表 | `SymbolCollector.Diags` |

codegen（`FunctionEmitter` 直呼 `symbols.ResolveTypeP`）绕过校验入口 → 不重复报、不碰字节不动点（纯诊断，
z42c/stdlib 自身无嵌套类越界引用 → `gen1==gen2` 保持）。`CheckTypeRef` 现同时处理**类与接口**（各自
`Visibility`，共用 `_checkVisRef` 逻辑），其余类型（prim/泛型形参/func/匿名未知）一律放行，绝不误报。

### 顺带：未定义类型诊断（E0443，fix-undefined-type-diagnostic，2026-08-16）

`CheckTypeRef` 既是所有类型引用注解的唯一 choke point，未定义类型名的诊断也在此报，无需改动 ~15 处调用点：

- **携名机制**：`SymbolTable.ResolveTypeP` 对具名 `NamedType` 全解析路径（泛型形参 / 别名 / void / prim /
  本包类 / imported 类 / 嵌套类型 / 限定名回退）均未命中的 fallthrough，构造 `Z42UnknownType` 时把
  `UnresolvedName = nt.Name` 一并记下（此前丢名、`Name()` 恒 `<unknown>`）。
- **报错**：`CheckTypeRef` 对**具名** Unknown（`UnresolvedName != ""`）报 `E0443 undefined type: <name>`
  （对标 C# CS0246）；**匿名** Unknown（`var` 无 init / 表达式级联抑制哨兵，`UnresolvedName == ""`）放行，
  不误报。递归覆盖 `Z42InstantiatedType` 泛型实参与（新增）`Z42ArrayType` 元素 → `List<C>` / `C[]` 也捕获。
- **不误报的保证**：`var` 在 `StmtBinder._varType` 于 `_chkTypeRef` 前被特判过滤；泛型形参经携形参的
  `ResolveTypeP` → `Z42GenericParamType`（非 Unknown）；嵌套类型经 `TypeEnv.ResolveType` 的 `+` 链上溯重试。
  全量 GREEN + 自举 5/5 字节不动点 = 无合法类型误报的权威验证。
- **`new` 收敛**：`_bindNew`（`ExprTyper`）原对未定义类型另发 `E0401 unknown type in new`，与 E0443 双报；
  已删该特例，`new C()` 统一由 `CheckTypeRef` 报 E0443，与其它位置一致。

## complete-class-access-control（2026-08-13）——类级访问补齐四项

`enforce-crosspkg-internal-class` 之后的四项 Deferred 一次补齐，类级访问控制自此完整（**均无格式 bump**——
复用 #184 已在线的 TYPE 可见性字节）：

### ④ 接口类型可见性

`Z42InterfaceType` 与 `Z42ClassType` 对称加 `Visibility` + `IsImported`。接口经 `ClassDescBuilder._interfaceDesc`
产同一 `IrClassDesc`（Flags bit4），走同一 TYPE 记录——#184 的 `WriteU8(cd.Visibility)` 对每条记录无条件写，
故接口 TYPE 早已携带该字节（此前恒 0=public）。补 6 处接口分支写入/还原真实可见性：`SymbolCollector._passInterfaces`
（从 `Mods`）、`_interfaceDesc`（emit）、`ExportedInterfaceZ.Visibility` + `TsigReconcile._rebuildInterface`
（跨包 round-trip）、`ImportedSymbolLoader`（传播）、`CheckTypeRef` 接口分支。跨包引用 internal 接口 → E0404。

### ③ 顶层声明拒绝 private/protected（E0442）

顶层 class/struct/interface/record/enum/函数在模块作用域下标 `private`/`protected` 无意义（默认 internal）。
`Parser.ParseCompilationUnit` 分派点检查 `mods` → `E0442`（parser bag，`SemanticDump.ErrorCount` 可见）；
嵌套类型经 `MemberParser` 不走此路径，仍可 private/protected。

### ② 不一致可访问性（E0441，C# CS0050 族）

`DeclBinder._bindClass` 对每个类的基类/接口 + 字段/方法（含 ctor）签名调 `AccessChecker.CheckExposure`：被暴露
类型可见性 rank 须 ≥ 暴露声明的**有效可访问性** `min(成员声明, 外层类)`（`MinVisibility`——镜像 C# effective
accessibility，`internal class` 内 `public` 成员实际只 internal 可见，暴露 internal 不算泄漏 → 绝不误报）。
可见性线性 rank `public 3 > internal 2 > protected 1 > private 0`；递归穿透泛型实例化（Def+实参）与数组元素；
非类/接口类型视 public 不触发。完整 accessibility-domain 偏序（引入组合修饰符时）Deferred。

### ① 类可见性反射（`Type.IsPublic` 族，对齐 C#）—— VM support 已落、stdlib 面推迟一 nightly

VM 此前 read-and-discard 的可见性字节现存入 `ClassDesc.visibility → TypeDesc.visibility`。6 个 builtin
（`__type_is_public` / `__type_is_not_public` / `__type_is_nested_{public,private,family,assembly}`）读 `td.visibility`
+ 名内 `+` 判嵌套（顶层 `IsPublic` xor `IsNotPublic`；嵌套为 `IsNested{Public,Private,Family,Assembly}` 之一；
无 TYPE handle（基元/数组）→ 全 false）。

> **`Type.z42` 的 6 个 extern 属性推迟到 follow-up PR**（bootstrap-seed 纪律，见下 Deferred）：本 PR 同时 bump
> 格式（zbc 1.33），CI 冷启动走两代自举、用**旧 nightly VM** 加载 stdlib；旧 VM 无新 builtin，stdlib 一引用即
> load 期 panic。故 **support（VM builtin + 存储 + Rust 单测）先行**、晚一个 nightly 再在 `Type.z42` 加 extern
> 属性 **use**（届时 nightly 的 VM 已含 builtin、且无格式 bump→无两代自举）。

## Deferred / Future Work

> **已完成（勿再列 Deferred）**：跨包 internal 类（#184）、接口类型可见性 / 顶层拒绝 E0442 / 不一致可访问性
> E0441（complete-class-access-control，2026-08-13）——见上「complete-class-access-control」节。类可见性反射
> **VM 面已落、stdlib 面推迟**（见下）。

### access-future-type-visibility-reflection-surface: 类可见性反射的 stdlib 面（`Type.IsPublic` 族）

- **来源**：complete-class-access-control ①（bootstrap-seed 纪律拆分）
- **触发原因**：本 PR 同时 bump 格式（zbc 1.33），CI 冷启动两代自举用旧 nightly VM 加载 stdlib；旧 VM 无新
  builtin（`__type_is_public` 等），`Type.z42` 一引用即 load 期 panic。VM 侧 6 builtin + `TypeDesc.visibility`
  存储 + Rust 单测已随本 PR 落地（support 先行）。
- **触发条件**：本 PR 合并 + nightly 发布后（该 nightly 的 VM 已含 6 builtin）→ follow-up PR 在 `Type.z42` 加
  6 个 `[Native]` extern 属性（`IsPublic`/`IsNotPublic`/`IsNested{Public,Private,Family,Assembly}`）+
  `src/tests/types/type_visibility.z42` golden。无格式 bump、无两代自举 → 直接过。

### access-future-inconsistent-accessibility-partial-order: 不一致可访问性的完整偏序

- **来源**：complete-class-access-control ②（design D2）
- **触发原因**：E0441 用线性 rank `public>internal>protected>private` 近似；C# accessibility-domain 是**偏序**
  （protected 与 internal 不可比）。z42 无组合修饰符（`protected internal`）故偏序退化，线性 rank 覆盖全部实用
  泄漏面；唯一近似偏差（internal 成员暴露 protected 类型按 rank 报错）语义上正确。
- **触发条件**：若未来引入组合修饰符（`protected internal` 等），需换成真正的域运算。

### access-future-inherited-internal-fidelity: 跨包**继承**成员的 internal 保真

- **来源**：enforce-access-control 实施期
- **触发原因**：`TsigReconcile` 对**祖先继承**成员经 `_visStr(int)` 重建；其自有成员 int 已带 3=internal
  故保真，但若某跨包基类的 internal 成员经继承链 reconcile 时上游 int 已塌缩，则可能漏判。当前主路径
  （自有成员）已正确，此为长尾边界。
- **触发条件**：出现「跨包访问经**多层继承**得到的 internal 成员未被拦」的实际用例时。
- **当前 workaround**：无（正确代码不跨包访问他包 internal，漏判仅「未报本不该写的违规」）。

### access-future-as-is-boxing: `as` / `is` 与装箱路径的访问检查

- 反射 `FieldInfo.GetValue` 对 private 字段等**运行时**绕过不在编译期强制范围。
