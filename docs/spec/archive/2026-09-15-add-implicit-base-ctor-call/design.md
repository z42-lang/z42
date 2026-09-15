# Design: 隐式基类构造器调用 + 构造器继承

## Architecture

```mermaid
flowchart TD
  subgraph 符号收集（包级，基类先于派生）
    M[MemberCollector 注册显式 ctor] --> Q{派生类写了实例 ctor?}
    Q -- 否，基类有可继承 ctor --> INH[为每个非 private 基类实例 ctor<br/>注册继承 ctor 符号：同形参（代换基类型实参）<br/>RegKey 按本类 primary/非 primary 规则]
    Q -- 否，基类无 ctor --> DEF{本类有实例初始化器?}
    DEF -- 是 --> R[注册默认 ctor 符号]
    DEF -- 否 --> N[无 ctor：new 零初始化]
    Q -- 是 --> X[不继承]
  end
  subgraph 绑定 DeclBinder
    E[显式实例 ctor] --> H{HasCtorInit?}
    H -- this --> T[call this-ctor；不注入初始化器]
    H -- base --> B1[本类初始化器 → call base-ctor → 体]
    H -- 无 --> I{基类有实例 ctor?}
    I -- 有可零实参 --> B2[本类初始化器 → call base() → 体]
    I -- 有但都要实参 --> ERR[新专用诊断]
    I -- 无 --> B3[本类初始化器 → 体]
    INH --> SY1[体 = 本类初始化器 → base(形参原样转发)]
    R --> SY2[体 = 本类初始化器]
  end
```

## Decisions

### D1: 减少样板——没写构造器的类继承基类构造器（Swift 规则）
**问题**：C# 模型下，`class MyErr : Exception { }` 这类「只想换个类型名」的派生类必须把基类每个构造器手写一遍
转发（`MyErr(string m) : base(m) { }`），绝大多数没有任何自定义逻辑。User 要求：代码尽量少写，语义正确且直观。

**其它语言怎么做**：
| 方案 | 写法 | 评价 |
|---|---|---|
| C#（含 C# 12 主构造器） | 手写转发 / `class D(string m) : B(m)` | 不继承；主构造器也只省一半 |
| Kotlin / Dart | `class D(x: Int) : B(x)` / `D(super.x)` | 转发语法更短，但每个重载仍要写 |
| C++11 | `using Base::Base;` | 显式一行继承全部，派生类还能加自己的；需要记住写这一行 |
| Swift | 派生类**没写任何指定构造器**（且新属性都有默认值）⇒ 自动继承基类全部指定构造器 | 零样板；写了任一构造器就不继承，规则可预测 |
| Python | 未定义 `__init__` 即沿用父类的 | 同 Swift 的直觉 |

**决定**：采用 Swift 规则。z42 的字段**全部有默认值**（无初始化器即类型零值），Swift 那条「新属性都有默认值」的
前置条件天然成立，所以规则可以无条件应用。
- 直观：派生类没说要怎么构造 ⇒ 就按基类的方式构造；一旦自己写了构造器，就只有自己写的那些（与 C# 一致）。
- 语义正确：继承的构造器一定调用基类构造器、一定执行本类初始化器，不存在「基类没被初始化」的对象。
- 失效方式是**响亮的**：给派生类加第一个构造器会让继承来的消失 ⇒ 调用点编译报错，不会静默改变行为。
- 不做 C++ `using` 那样「既写自己的又继承」的混合形态（Open Question 3，按需另立；可做成显式标注）。

### D2: 继承 / 默认构造器进符号层
**问题**：派生类要知道基类有哪些（含继承来的、默认的）构造器；今天合成 ctor 只在绑定后生成体、IR 期发射，
**不是 `MethodSymbol`、不进 TSIG** ⇒ 同包别的 CU 与依赖包都看不见。
**选项**：A — 收集期注册为符号并随 TSIG 导出；B — 每个类都无条件发默认 ctor；C — 运行期把「ctor 不存在」当 no-op。
**决定**：A。B 给普查中 595 个「基类无 ctor、无初始化器」的类（多为 AST / IR 节点）加空调用链，z42c 分配密集，
且字节全面漂移；C 与 #614/#629「缺构造器不再静默」冲突。

### D3: 继承构造器的形态
对基类每个**非 private、非 static** 的实例构造器 `B(P1 p1, …, Pn pn)`，派生类得到 `D(P1' p1, …, Pn' pn) : base(p1, …, pn)`：
- `Pi'` = `Pi` 按基类型实参代换（`class IntBox : Box<int>` ⇒ `T → int`，复用 `MethodTypeArgSubst.ByName`）。
- 默认值、`params`、caller 宏元数据随签名原样继承：本地基类复制 `Param`（含 `Default` 表达式）；导入基类复制
  `Z42FuncType.ParamDefaults` / `ParamCallers` / `ParamsFrom`（TSIG 已有，跨包默认值走既有 `_crossPkgDefault`）。
- 可见性与基类构造器相同（`protected` 仍 `protected`）。
- RegKey 按本类 primary / 非 primary 规则计算（与显式声明同路径 `SymbolCollector.RegisterMethod`）。
- 体 = 本类实例初始化器 → `base(形参)` 原样转发（`params` 形参以数组原样传递，不再展开）。

### D4: 处理顺序
本包类按**基类先于派生类**的拓扑序处理（本包基类可能在别的 CU），基类的继承 / 默认构造器先定案，派生类再据此继承；
导入基类的构造器来自 TSIG（已包含它自己继承来的）。无环（继承本身无环，环已由既有诊断拒绝）。

### D5: 合成体不再内联祖先链
基类初始化器由基类构造器执行；旧的「按 CU 收集祖先链并内联」删除——它正是跨文件 / 跨包丢失的来源。

### D6: 诊断
新增专用码（User 裁决；编号实施时按 `DiagnosticCodes` 取下一个空位）。只在「派生类写了实例构造器、没写
`: base(args)` / `: this(..)`、基类有实例构造器但无一可零实参调用」时报，位置指向该构造器：
`` constructor of `D` must call a base constructor: `B` has no constructor accepting 0 arguments (add `: base(...)`) ``。
（派生类没写构造器的情形不会报——它直接继承带参的那些。）

### D7: 自举与字节
普查（proposal）：stdlib / 编译器 / xtask 中**不存在**「基类有构造器、派生类没写 `: base`」或「基类链有初始化器」的
生产类；87 个无 ctor 的派生类其基类都没有构造器 ⇒ 不会继承出任何构造器，也不会新增基类构造器调用。
「有初始化器、无显式 ctor」的类，其默认 ctor 从 IR 期发射改为收集期符号：IR 函数字节不变（同名、`public` ⇒
Visibility 0、无 flag），但它现在会作为方法进入 TSIG ⇒ 这类包的 TSIG 字节会变（未逐包实测：跨 worktree 比较 zpkg
字节受 build_id 路径影响不成立）。无 zbc / zpkg 格式变更；不涉及新语法，无两 nightly 纪律问题。

## Implementation Notes

- 继承 / 默认构造器的 `MethodDecl` 由编译器合成（形参复制、`HasBody=true`、体为空块，真实体在绑定期生成），
  标记为合成（新字段，构造后置位，守种子 ABI）：`_hasExplicitInstanceCtor` 与「写了任何构造器就不继承」的判据须排除它。
- 隐式 `base()` 复用 #650 在 `DeclBinder` 的注入路径；继承构造器的 `base(形参)` 走同一路径（`BaseArgs` = 形参标识符）。
- `ForwardGenerator`（`[Forward]`）已排除 ctor（`ForwardGenerator.z42:272`），不受影响。
- Record 主构造器（`RecordSynth`）是显式构造器 ⇒ 不继承，走隐式 `base()` 规则。
- 与 ③（`fix-crosspkg-static-call-cctor`）无交集。

### D9: 与构造器可见性（#660）的衔接
- 「只继承非 private」在 #660 后成立（此前构造器可见性从不检查，无修饰符构造器名义 private 实际处处可调，导致规则失效——
  这正是本变更暂停、先做 #660 的原因）。
- 可见性检查位于 `_bindCtorArgs`：显式 / 隐式 `base()`、`: this()`、继承构造器的转发都经它，调用基类 private 构造器报 E0404。
- **基类构造器全是 private、派生类没写构造器**：合成默认构造器（而不是什么都不合成），由隐式 `base()` 报错——有 private 无参
  构造器 ⇒ E0404，只有 private 带参构造器 ⇒ E0469。否则派生类会被默认构造、基类构造器静默不执行（对标 C# 此时报错）。

### D8: 实施中的调整
- **E0426 / 重载决议不考虑默认值形参是既有缺陷**（方法与构造器同样中招，nightly 复现：`M.F("a")` 对
  `F()` + `F(string, int = 2)` 报「no static method」；`new B("a")` 同形编译通过、运行期选中无参 ctor）。与本变更
  无关，不在此修；回归用例避开「默认值 + 重载」组合，默认值只在单构造器基类上验证。已登记待立项。
- 初始化子句改走 `_bindCtorArgs` 后，`: base(args)` 与 `new` 在上述缺陷上表现一致（不更好也不更坏）。

## Testing Strategy

- 单测：诊断（显式 ctor 无 `: base` + 基类只有带参 ctor）；不报（派生无 ctor 继承带参 / 基类默认值 / `params` / `: this()` / 基类无 ctor）；
  继承签名（重载集、默认值、可见性、泛型代换）；E0426 对继承集生效（`new D()` 在无无参可继承时报）。
- 运行期 `src/tests/classes/implicit_base_ctor.z42` + `inherited_ctors.z42`：spec 全部场景，修前逐条确认红（修前多为 E0426 / 字段为 0）。
- 同包跨文件：单包多文件工程（z42b 或 cross-zpkg 单包形态）。
- 跨包：cross-zpkg `inherited_ctor_cross_pkg`（基类在依赖包；派生在中间包、主包 `new`；默认值跨包）。
- 两后端手验；全量 `xtask test` GREEN；`xtask test bootstrap`。
