# Proposal: `[Forward]` 成员转发 —— 用组合替代多继承的语法糖

> 状态：🟡 **DRAFT，待 User 确认** | 2026-09-12
> 派生自 [[add-method-reference]]（`methodof`，已合并 #568），两者已解耦；本提案**使用** `methodof`。

## Why

z42 不支持多继承（`docs/design/philosophy.md` 明文 Single inheritance），组合是唯一替代。
但组合出来的链式访问累赘：

```z42
class Service {
    private Logger _log;
}
svc._log.Log("x");        // 或者手写 N 个 public void Log(string m) => this._log.Log(m);
```

手写转发是**零成本对照**——它能工作、能被 IDE 跳转、签名完全可控。本提案要赢它，只能赢在
**「上游签名面变化时，转发面自动跟上，且跟不上时编译报错」**，而不是「少打几个字」。

### 这一条是本提案的立身之本

> 手写转发的真实失效模式不是「写起来累」，是**上游改了签名、转发面悄悄过时**：
> 上游 `Log(string)` 改成 `Log(string, Level)`，手写的那份要么编译错（好），要么因为重载
> 仍在而**继续转发到旧重载**（坏，静默）。

所以白名单**必须是编译期可检查的**——这正是 `methodof` 的用武之地（见下）。

## What Changes

### 载体：Generator 生成源码（不是名字解析重写）

`[Forward]` 贴在**字段**上，由一个 generator 在编译期生成真方法的**源码**，走 `GenSink.Augment`
脱糖成同命名空间的 `partial` 碎片，重新 parse 成真实 CU。

**生成真方法**（而非绑定期把 `a.c` 改写成 `a.b.c`）是刻意的：只有真方法才能满足接口、被反射看见、
被 IDE 跳转。Lombok `@Delegate` / Kotlin `by` / Rust RFC 2393 全是生成真方法，没有一门语言用纯名字解析。

> ❌ **已否决：纯名字解析重写。** 要 `$Forward` 哨兵 + 下游重写 + 绑定期可见性豁免钩子（无先例）+
> 立一条「名字级」新公理 + 碰重载决议器，且**做不到接口满足与反射可见**。

### 三档粒度

```z42
class Service {
    [Forward<ILogger>]                                   // ① 接口面：签名由接口钉死
    private Logger _log;

    [Forward(methodof(ILogger.Log(string)))]             // ② 名单：逐个点名，编译期可检查
    private Logger _log2;

    public partial void Log(string m);                   // ③ partial 声明：generator 填 body
}
```

| 档 | 转发面由谁钉死 | 上游加重载会不会自动进来 | 参数名/默认值保真 |
|---|---|---|---|
| ① `[Forward<I>]` | **接口** | 否（接口没变就没变） | 由接口签名给出 |
| ② `[Forward(methodof(...))]` | **逐个点名** | 否 | ⚠️ 见「已知损耗」 |
| ③ `partial` 声明 | **用户手写的声明** | 否 | ✅ 完全保真（用户自己写的） |

**没有「public 全量省略」档**——它承担绝大部分持续复杂度换最薄收益，且「钉死名字却没钉死签名」
本身是内在矛盾（上游加重载仍会自动进转发面）。

### ⭐ 白名单用 `methodof`，不用字符串

这是本提案相对 09-06 设计的**唯一实质新增**，也是 `methodof` 落地后第一个真实使用点：

```z42
[Forward("Log", "Log(string)")]              // ❌ 改名 → 静默不再转发；拼错 → 静默
[Forward(methodof(ILogger.Log(string)))]     // ✅ 改名/签名漂 → 编译错误 E0459/E0460
```

**已核实它真的会被检查到**（不是想当然）：`AttributeSynth:75-76` 对**字段级** attribute 按
`fld$<Class>$<Field>` 合成工厂函数，把实参 AST 原样塞进去编译；而 `GeneratorDriver` 的
ordering fixup 目前只剥**类级**触发点的工厂。

> 🔴 **因此有一条硬约束进 tasks**：扩展 GeneratorDriver 支持字段级触发时，**不得**顺手剥掉字段级
> 触发 attr 的合成工厂——剥了这层编译检查就没了，白名单退回字符串时代。

generator 自己读的是 `Attr.Args` 的**原始 AST**（`MethodOfExpr` 节点，直接拿 `OwnerName`/`Member`/
`ParamTypes`），**不需要**运行期的 `MethodInfo`。即：methodof 在这里同时充当「编译期可检查的书写形式」
与「generator 的结构化输入」，运行期零参与。

### 五条语义规则

| # | 规则 | 依据 |
|---|---|---|
| S1 | **不产生子类型、不产生隐式转换** | D 的 `alias this` 灾难全源于此（Walter Bright 原话 *"has turned out to be a mistake"*；DIP66 多 alias this 被 **Rejected**，理由「就是多继承」） |
| S2 | `Object` 协议方法（`ToString`/`Equals`/`GetHashCode`/`GetType`）**永不转发** | 对齐 Lombok `@Delegate` 的 *"All public non-`Object` methods … are copied"* |
| S3 | **不传递、不递归**，只提升一层 | D 的 `alias this is not transitive` 是 15 年未修的 open bug 群 |
| S4 | 多个 `[Forward]` 提供同名成员 → **声明处报错** | Go 推迟到用时才报，且动态接口断言处**静默返回 false** |
| S5 | 外层已声明同名成员 → **generator 跳过不生成** + info 诊断 | 不是「静默 shadow」；技术上也别无选择，见下 🔴 |

### 🔴 S5 不是风格选择，是被一个已知缺陷逼出来的

**跨碎片同名重载会让先声明的那个从方法表里静默消失。** 这是 `partial-types.md` 记录在案的已知
限制，本提案**实测复现**（`2026-09-12`）：

```z42
// 碎片 1（用户）        public string Log(string m) { … }
// 碎片 2（generator）   public string Log(int n)    { … }
```
→ 编译**不报重复**，但 `s.Log("hello")` 得到 `E0402: cannot assign string to Int32` ——
用户自己那个 `Log(string)` 已经被覆盖掉了，报出来的却是一条看不懂的假类型错。

机制：实例方法注册键是「声明序首个同名 → 裸名 primary／其余 → 全签名 mangle」，而做这个判定的
`emittedInst` tracker 是 `MemberCollector._fillClass` 的**局部变量**、`_fillClass` **按碎片逐个调用**
⇒ 两个碎片各自认为「我这个是首个」⇒ 双双注册裸键 ⇒ 后写覆盖先写。E0433 只在**完整签名相同**时才报，
重载签名恰恰不同 ⇒ 不报错。

⇒ **S5 必须按「名字」跳过，不能按「签名」跳过**；arity 不同也要跳。这也意味着本提案
**不得**生成任何与用户碎片同名的方法，哪怕签名不冲突。

## 前置验证（已做完，2026-09-12）

| # | 验什么 | 结果 |
|---|---|---|
| A | ③ 档的 partial 配对：声明在用户碎片、实现在 generator 碎片能否配上 | ✅ **通过**——实测跨碎片 `partial string Greet(string m);` + 另一碎片填 body，编译干净、运行正确 |
| B | 生成源码是否落盘 | ❌ **不落盘**——`GeneratorDriver` 全文件无 `File.WriteAllText`，`_parseGen` 直接在内存 parse。**这顶在「选 Generator 路线是为了以后能调试」这条理由上**，见下决策点 |
| C | 跨碎片同名重载的真实行为 | 🔴 **静默覆盖**（非硬错误）——见上 S5，已实测复现 |

> C 的结论与 09-06 记忆里写的「跨碎片同 RegKey 是硬错误 `PartialDuplicateMember`」**不符**。
> 以本次实测 + `partial-types.md` 为准：**不同签名不报错、静默丢失**，比硬错误更危险。

## 生成源码落盘：`[build] generated_dir`（User 2026-09-12 裁决）

**走 `z42.toml` 配置，不加 CLI flag；没配置就用默认值。**

```toml
[build]
generated_dir = "…"        # 没配置 → <output_dir>/generated
```

落在既有的 `[build]` 段（已有 `output_dir` / `cache_dir` / `dist_dir` / `incremental` / `hooks`），
沿用 `cache_dir` / `dist_dir` 那套「**目录型配置 + 从 `output_dir` 派生的默认值**」形状，
`ManifestLoader._parseBuild` 工程级(:216) 与工作区级(:167) 两处同步。

生成文件名沿用 GeneratorDriver 现有的 CU 命名：`<generated_dir>/<pkg>/__gen$<Name>$augment.z42`。

### 这意味着**恒落盘**，不是 opt-in

配置的是「写到哪儿」而不是「写不写」⇒ 只要有 generator 产出，每次构建都会写。这与选
Generator 路线的初衷（「以后可以支持调试」）一致：**栈帧里出现 `__gen$…z42` 时，背后永远有一个
真文件可以打开**，不需要先想起来去开个开关、再重编一次才能看到。

代价可控：只有真的用了 `[Forward]`（或其它 generator）的包才会产出文件，没用就一个字节都不写。

**关闭方式**：`generated_dir = ""` 显式表示不落盘。留这个口子是因为某些场景（只读文件系统、
CI 里不想要额外产物）需要它，但**默认是写**。

> 🔴 **`generated_dir` 必须在 `[sources] include` 扫描范围之外**，否则下一次构建会把生成的
> `partial` 碎片当成**用户源码再编一遍** —— 同一个类出现两份带同名成员的碎片，直接踩验证 C 的
> **静默覆盖**：用户的方法被吃掉、调用点报假类型错。默认值在 `output_dir`（`artifacts/…`）之下、
> 天然在扫描范围外；但既然这是个**用户可配的路径**，就必须**校验并报错拒绝**，不能只 warn ——
> 这个配置配错的后果是「你的方法静默消失」。（tasks 4.4 / 4.5 / 5.5）

## 待 User 裁决的决策点

1. ✅ **已裁决**：生成源码落盘 → `[build] generated_dir`，默认 `<output_dir>/generated`（恒落盘，见上节）。
2. ✅ **已裁决：三档一次全做**（User 2026-09-12）。但 1.D/1.E 查出 ①② 对**跨包类型**不可保真
   且不可判定 ⇒ 适用边界收在「①② 仅本包类型」，见下节。**这是「全做」的实际含义，需 User 知悉。**

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|---|---|---|
| `src/compiler/z42c.semantics/src/GeneratorDriver.z42` | MODIFY | **唯一框架改动**：触发点扫描扩展到字段级 `AttributedDecl`（现仅顶层 `Inner is ClassDecl`，:137-140）。**不得剥字段级触发 attr 的工厂** |
| `src/libraries/z42.core/src/ForwardAttribute.z42` | NEW | `[Forward]` 的真实 attribute 类（合成工厂需要真类型） |
| `src/libraries/z42c.*/…/ForwardGenerator.z42` | NEW | 转发 generator 本体：读 `Attr.Args` AST → 收转发面 → **显式 sort** → 生成源码 |
| `src/libraries/z42.project/src/ManifestLoader.z42` + `BuildConfig` | MODIFY | `[build] generated_dir`（目录，默认 `<output_dir>/generated`）；工程级 + 工作区级两处，沿用 `cache_dir`/`dist_dir` 的形状 |
| `src/libraries/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新诊断码（S4 冲突 / S5 跳过 info / 白名单指向不存在的成员 / emit 目录落在源码扫描范围内） |
| `src/tests/forwarding/**` | NEW | e2e：三档各一条 + S1–S5 各一条 + 跨包 |
| `docs/book/src/language/member-forwarding.md` | NEW | 语义规则 / 三档 / 与多继承的界线 |

## Out of Scope

- **`public` 全量省略**（已否决，见上）
- **转发字段/属性**：v1 只转发方法。字段转发要生成属性、牵扯可写性语义，独立评估
- **递归/传递转发**（S3 明确不做）
- **调试器对生成源码的支持本体**——本提案至多产出落盘开关

## 适用边界：①② 档只对**本包类型**（2026-09-12 前置验证 1.D/1.E 定出来的）

|  | 本包类型 | 跨包类型 |
|---|---|---|
| ③ `partial` 声明档 | ✅ | ✅ |
| ①`[Forward<I>]` / ②`[Forward(methodof(…))]` | ✅ 签名**完全保真** | ❌ **报错拒绝** |

**不是保守，是跨包元数据里就没有那些信息**：

- **参数名**：`ImportedSymbolLoader` 四个读取点只取 `Params[j].TypeName`，`ExportedParamZ.Name`
  从不落进 `MethodSymbol` ⇒ 跨包只能造 `p0/p1` ⇒ 命名实参失效。
- 🔴 **`ref` / `out`：TSIG 格式里根本不记录**（`IsRef` 在 z42.ir 全层 + 导入侧零命中）。
  ⇒ 我们**连「这个方法带不带 ref」都判定不了**，没法只拒绝带 ref 的那部分。

而丢掉 `ref` 的后果是**静默的**——实测：调用点少写 `ref` 照样编译通过、零诊断、修改直接丢失
（`Bump(ref x)` 得 2，`Bump(y)` 得 1）。一个「判定不了 + 错了还不响」的组合，只能整类拒绝。

> 这条既有缺陷不由本提案修：`add-ref-borrow-system` 的下一步「typecheck ref 真 byref」正覆盖它。
> 等它落地并把 `ref` 记进 TSIG 之后，跨包 ①② 的封印可以解除——**本提案的判据用 `ms.HasDecl`，
> 不依赖那个判不出来的 ref 信息，所以边界是硬的、届时也容易放开。**

本包侧则**完全保真**：`ms.Decl.Params[i]` 带齐 `Name` / `IsRef` / `IsParams` / `Default` / `Attrs`。
⇒ proposal 初稿写的「①② 档参数名一定丢」**过于悲观，已更正**——只有跨包才丢。

## 验证

- `xtask test` 全绿；`test stdlib --mode jit` 补跑
- **自举字节不动点 gen1 == gen2** —— 生成顺序必须显式 sort，否则 `Methods`（hashed StrMap）的迭代序
  会让 zpkg 字节漂移（`common-pitfalls.md §1` 铁律）
- `xtask test bootstrap` 无越界
