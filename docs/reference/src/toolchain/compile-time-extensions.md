# 插件：Analyzer / Generator / Hook

z42 允许把**你自己的代码加载进工具链**（编译器 z42c / 构建器 z42b），在构建期跑：

- **Analyzer** —— 遍历语法树、报自定义诊断（可选附带 `--fix` 的自动修复）。
- **Generator** —— 在 bind 之后生成源码：追加新编译单元、往已有类型注入成员、替换被标注的声明。
- **Hook** —— 接进 z42b 的构建相位（编译前后、资产前后等），定制构建编排。

三类都打包成 **`kind = "plugin"`** 的 zpkg（一个插件包可同时提供几类——包内实现了哪些契约，构建时
推导出能力位写进 zpkg 头），由消费方工程的 **`[plugins]`** 段声明。它们**只在工具链进程里运行，
不链入目标产物**。旧名 `kind = "analyzer"` 与 `[analyzers]` 段已删除（写了按未知键/未知段报错）。

`kind = "plugin"` 做两件事：让工具链的契约包对这个工程**自动可见**、免声明（见
[解析域](#解析域编译期扩展自动看得见-sdk-库)），并声明「我是插件」——`[plugins]` 的
path 条目据此校验，`[dependencies]` 据此拒收。

> 契约包（`z42c.analyzers` / `z42c.generators` / `z42c.syntax` / `z42c.core` / `z42c.semantics`）是 **SDK 库**：住在 `programs/z42c/`、
> 不在 `libs/`。`kind = "plugin"` 自动看得见它们；普通工程（`kind = "lib"` / `"exe"`）要在
> `[dependencies]` 里**按名声明**才可见。

> 本页所有代码片段都来自实跑通过的最小工程。

## `[plugins]` 段

```toml
[plugins]
"demo.noemptycatch" = "0.1.0"
```

值的写法与 `[dependencies]` 相同，但语义不同：

| | `[dependencies]` | `[plugins]` |
|---|---|---|
| 何时加载 | 编译目标代码时解析符号 | **加载进工具链、构建期执行** |
| 进不进产物 | 进 | **不进** |
| `path = "..."` | 支持，z42c 代为构建整个闭包 | 支持，代建**那一个工程**；也可指 `.zpkg` 文件直接用 |

**一个段覆盖三类**：z42c 从 `[plugins]` 列出的每个 zpkg 里同时寻找 `: Analyzer`、
`: Generator`、`: ModuleGenerator` 的类型；z42b 反射出全部继承 `: BuildHooks` 的具体类。条目里
**不写类型**——包提供哪几类由 zpkg 头能力位说明，宿主按位分流。

### 三种条目写法

```toml
[plugins]
"demo.noemptycatch" = "0.1.0"                 # 按名：在插件目录找 demo.noemptycatch.zpkg
"demo.gen"          = { path = "../gen" }     # 按工程路径：代建该工程，取其 dist
"demo.local"        = { path = "../x.zpkg" }  # 按 zpkg 路径：现成产物，读头校验后直接用
```

**按名**：在**插件目录**里找 `<name>.zpkg`——`Z42_PLUGINS` 指向的目录，或各依赖目录同级的
`plugins/`（SDK 布局：`<sdk>/plugins`）。与依赖目录（libs）刻意分开：那里是运行期解析域，插件
混进去会污染它。要求你先单独 `z42c build` 那个工程、再把产物拷进插件目录。

> ⚠️ **开发态构建产出的是 indexed zpkg**（主文件 + 旁边散装的 `.zbc`）。只拷主文件过去，
> 加载时会报 **E0493**。要么把散装 `.zbc` 一起拷，要么用 `--release` 构建 handler 工程，
> 得到单文件 packed zpkg。

**按工程路径**：指向目录，其中须恰有一份**工程清单**（裸 `z42.toml` 优先，否则唯一一份
`*.z42.toml` —— 判据与 `z42c build <dir>`、与 `[dependencies]` 的 path 完全一致），
`[project].name` 与这里写的名字一致，且 `kind = "plugin"`。工具链会代为构建它（用消费方的
`--release` / 优化档），再把产物按能力位挂给对应宿主。改了插件的源码，消费方下次构建会重编——
指纹含该 zpkg 的内容。**这是自己写插件时的推荐写法**：不用把 zpkg 拷来拷去，也不会踩上面那条
indexed/packed 的坑。住在工程目录里的 path 插件目录自动排除出本工程源发现。

**按 zpkg 路径**：现成产物直接用（读头验能力位 + META 验名，不代建）。适合钉住一份已构建的插件
（CI 产物、发布件）；产物字节变了消费方照样重编。

代建出来的 zpkg **不进消费方的解析域**（与 `[dependencies]` 的 path 闭包刻意不同）：插件只活
在工具链进程里，把它的 dist 并进依赖目录就等于让构建期代码对运行期代码可见。

### 三条校验

| 写法 | 结果 |
|---|---|
| `kind = "plugin"` 的工程出现在 `[dependencies]` | 报错——它永不链入产物，运行期不会到场（按名也拦得住：读头能力位） |
| 位全 0 的包出现在 `[plugins]`（按名 / zpkg） | 定位阶段即报「不是插件包」——不留到加载后才发现「零 handler」 |
| `kind = "plugin"` 的工程里一个契约都没实现 | **构建该插件时就报 E0505**，点名四个契约 |

zpkg 头记能力位（bit3–5），所以按名 / 按 zpkg 引用在拿到文件的第一刻就能判——旧世界「zpkg 不记
kind、按名没法校验」的洞补上了。

### 声明了却没生效？那会报错

挂上去的 zpkg 若加载成功、里面**一个 handler 都没有**（没有实现 `Analyzer` / `Generator` /
`ModuleGenerator` 的类型），报 **E0496**。成因两个：挂错了包（那是个普通库），或这个 handler 由
**另一代编译器**编出、契约接口对不上——后者用当前工具链重建该 handler 工程即可。

> 这里必须报错：否则扩展干脆不跑而编译照常成功，「声明了、也没报错、就是不生效」是最难查的一类。

zpkg **格式**代差（handler 由另一代工具链编出、wire 格式已经 bump 过）是另一条路：加载时就被
拦下，报 **E0493**，消息里直接说出两边的格式版本号。

## 写一个 Analyzer

契约在 **`z42c.analyzers`**（命名空间 `Z42.Analyzers`）；AST 节点类型（`CatchClause` 等）在 `z42c.syntax`（命名空间 `Z42.Syntax`）。依赖这三个包即可：

```toml
[project]
name    = "demo.noemptycatch"
version = "0.1.0"
kind    = "plugin"

[dependencies]
"z42c.core"   = "0.1.0"
"z42c.syntax" = "0.1.0"
"z42c.analyzers" = "0.1.0"
```

> 这些契约包住在 `programs/z42c/`、**不在 `libs/`**。`kind = "plugin"` 下它们自动可见，上面的
> `[dependencies]` 可以省略（写上也无妨）。这个字段同时也是「我是插件」的声明——`[plugins]` 的
> **path 条目**只接受它。

类名**必须以 `Analyzer` 结尾**（E0445 强制）：

```z42
namespace Demo.Lint;

using Z42.Core;
using Z42.Syntax;      // AST 节点
using Z42.Analyzers;   // Analyzer / DiagRule / DiagSink / SyntaxKind / AnalyzerSeverity

public sealed class NoEmptyCatchAnalyzer : Analyzer {
    public DiagRule[] SupportedRules() {
        DiagRule[] rs = new DiagRule[1];
        rs[0] = new DiagRule("DEMO001", "空 catch", "空 catch 会把异常悄悄吃掉",
                             "Correctness", AnalyzerSeverity.Warning, true);
        return rs;
    }
    public int[] ObservedKinds() {
        int[] ks = new int[1];
        ks[0] = SyntaxKind.CatchClause;
        return ks;
    }
    public void OnSyntaxNode(int kind, object node, DiagSink diags) {
        BlockStmt b = (node as CatchClause).Body as BlockStmt;
        if (b != null && b.Count == 0) {
            diags.Report(this.SupportedRules()[0], (node as CatchClause).Span);
        }
    }
}
```

消费方工程加上 `[plugins] "demo.noemptycatch" = "0.1.0"` 后，`z42c build` 就会报：

```
./src/Main.z42(7,5): DEMO001: 空 catch 会把异常悄悄吃掉
```

### 契约速查

| 成员 | 说明 |
|---|---|
| `DiagRule[] SupportedRules()` | 本 analyzer 能发的规则（供 `[lints]` 枚举与 severity 解析） |
| `int[] ObservedKinds()` | 只在这些节点上被回调（`SyntaxKind.*`） |
| `void OnSyntaxNode(int kind, object node, DiagSink diags)` | 回调。`node` 是 `object`，自己 `as` 成具体节点类型 |

**契约刻意没有 delegate**（z42c 规避 delegate；命名 delegate 跨 zpkg 会丢全限定名）。

### ⚠️ `SyntaxKind` 目前只有 6 种

```
CatchClause=1  TryCatchStmt=2  ClassDecl=3  MethodDecl=4  WhileStmt=5  ForStmt=6
```

观察面就这么宽 —— 想在表达式、字段、属性上做 lint，现在还挂不上去。

### `AnalyzerSeverity`

`Hidden=0`（不显示，只供 `--fix` 消费）/ `Info=1` / `Warning=2` / `Error=3`（编译失败）。

## 调级与抑制

### `[lints]`：按规则覆盖 severity

```toml
[lints]
DEMO001 = "error"           # warning → error，编译失败、不产产物
"webgen.*" = "none"         # 支持通配前缀
warnings_as_errors = true   # 特殊布尔键
```

`DiagRule.EnabledByDefault` 为 `false` 的规则默认不报，要在 `[lints]` 里显式打开。

### `#suppress` / `#restore`：区间抑制

```z42
void Main() {
#suppress DEMO001
    try { Console.WriteLine("hi"); }
    catch (Exception e) { }
#restore DEMO001
}
```

区间内静默，`#restore` 之后照报。另有 `[Suppress]` 特性做声明级抑制。
两者都是**纯编译期**的，不写进 zpkg。

## 让诊断自带修复（`--fix`）

analyzer 可以在报诊断的同时给出修复：把 `diags` 下转成 `FixSink`，调 `ReportFix(rule, at, fix)`
（`CodeFix` 装一组 `TextEdit`）。`z42c build --fix` 会就地重写源文件。原则是
**「谁报诊断谁产修复」**——包括第三方 `[plugins]` zpkg。

## 写一个 Generator

Generator 契约（`Generator` / `ModuleGenerator` / `GenTarget` / `GenSink` / `GenContext`）住在
**`z42c.generators`**，命名空间 `Z42.Generators`。它比 analyzer 契约深一层：generator 在一次
预备绑定**之后**运行，拿得到解析后的符号（`Z42ClassType` / `SymbolTable`，命名空间 `Z42.Semantics`，
由 `z42c.generators` 传递引入）。

两种形态：

- **applied generator** —— 被 `[X]` 标注处触发，只看见被标注的那一个声明。
  类名须为 `<Trigger>Generator`（`AddEqGenerator` → 触发 `[AddEq]`，E0447 强制）。
- **module generator** —— 不贴任何声明，注册即跑一次、扫全编译，经
  `ctx.TypesWith<T>()` / `ctx.MethodsWith<T>()` 强类型查询，聚合成表（路由表 / DI 容器 / serde 注册）。

三个 sink 操作：

| 操作 | 含义 |
|---|---|
| `AddSource(hint, src)` | 追加一个新编译单元 |
| `Augment(id, membersSrc)` | 往已有类型注入成员（不用手写 `partial`） |
| `Replace(id, src)` | 替换被标注的声明 |

多个 generator 之间用 `Consumes()` / `Produces()` 定序，引擎按拓扑分层逐层重新 bind；
成环报 **E0449**。

### 解析域：插件自动看得见 SDK 库

契约包（`z42c.generators` / `z42c.analyzers` / `z42c.semantics` / `z42c.syntax` / `z42c.core`）是 **SDK 库**——不在 SDK 的 `libs/` 里，而在
**`programs/z42c/`**，与 z42c 自己的 zpkg 同址（规则全文见 [z42-toml.md](z42-toml.md) 的「SDK 库」）：

| 工程 | SDK 库可见吗 | 部署 |
|---|---|---|
| `kind = "plugin"` | **自动**，免声明 | 不拷：加载进工具链进程（z42c / z42b），用宿主那份 |
| `kind = "lib"` / `"exe"` | **按名声明**才可见；未声明 ⇒ `E0494`，并点名提供它的 SDK 库与声明写法 | exe 拷进输出目录（连同传递依赖） |

> 📌 编译器域的库落在 `programs/z42c/`（不存在 `compiler-libs/` 目录）。普通工程（非 analyzer）
> 要引用编译器域的库，在 `[dependencies]` 按名声明即可，见 [z42-toml.md](z42-toml.md)。

所以一个 generator 工程的清单长这样：

```toml
[project]
name    = "demo.gen"
version = "0.1.0"
kind    = "plugin"

[dependencies]
"z42c.generators" = "0.1.0"
```

消费方用 path 挂上它，全程不需要手工拷 zpkg：

```toml
[plugins]
"demo.gen" = { path = "../gen" }
```

## 写一个 Hook

hook 插件接进 **z42b** 的构建编排。契约在 **`z42.build`**（`BuildHooks` 基类，八个头相位的
`Before*` / `After*` 虚方法 + `ProvideNative`）；override 需要的钩子，默认 no-op：

```z42
namespace Demo.Build;

using Std.IO;
using Z42.Build;

public class MyHooks : BuildHooks {
    public override void BeforeAssets(IPipelineContext ctx) {
        Console.WriteLine("prepare assets for " + ctx.Target.Name);
    }
}
```

包内**全部**继承 `BuildHooks` 的具体类都会被发现并实例化（按类全名序），多个插件 / 多个类按
`[plugins]` 声明序依次组合调用。（本工程的 hook 也这么写：hook 目录补一份 `kind = "plugin"` 清单、
经 `[plugins]` path 条目引用——旧 `[build] hooks` 目录写法已删除。）

> hook 在 z42b 进程里全权运行（写文件、起子进程），与 `[dependencies]` 上一个包的运行期代码同级
> 信任——声明即信任，不做沙箱。

## lib 携带插件（`[plugins.export]`，消费方零配置）

一个库可以把插件**随包带给**所有依赖它的工程——消费方一个字都不用写：

```toml
# foo（kind = "lib"）的清单
[plugins.export]
"foo.gen"  = { path = "../gen" }    # 工程路径：构建时记「名 + 精确版本」进 zpkg 的 PLGS 段
"foo.lint" = "0.1.0"                # 按名携带（须写版本）
```

```toml
# 消费方——只有普通依赖，插件自动生效
[dependencies]
"foo" = "0.1.0"
```

- **携带是引用不是内联**（serde → serde_derive 形态）：lib 产物只记 PLGS 引用，插件 zpkg 分发在
  插件目录——发布 lib 时把携带的插件一起放进插件目录（release 构建 lib 时会验证 path 条目的产物）。
- 生效规则：只走**真在依赖闭包里**的包（传递携带经各 dep 的 DEPS 传递生效）；显式 `[plugins]`
  赢携带；`[plugins] "x" = false` 关掉某个携带项；analyzer 规则粒度仍可用 `[lints]` 调。
- 失败即失败：携带件缺失 / 版本不符，构建红且**点名携带方**——「声明了、静默没生效」不给留。
- 只对 lib 有意义（插件工程不参与依赖图，携带链终止在 lib）。

## 插件怎么被找到、被加载

1. 条目定位：按名查插件目录（`Z42_PLUGINS` / 依赖目录同级 `plugins/`）；path 条目代建或直取产物。
2. 读头 16 字节验能力位与包名——位全 0 即「不是插件包」，不留到加载后。
3. z42c 取带 Analyzer / Generator 位的包，`AnalyzerLoader` / `GeneratorLoader` 按接口发现、实例化；
   z42b 取带 Hook 位的包，反射出 `BuildHooks` 具体类组合执行。
4. 一切失败（加载失败 E0493、零 handler E0496、非插件包）都是构建失败——没有静默降级。
