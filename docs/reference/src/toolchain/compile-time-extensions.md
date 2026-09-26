# 编译期扩展：Analyzer 与 Generator

z42 允许把**你自己的代码加载进编译器**，在编译期跑：

- **Analyzer** —— 遍历语法树、报自定义诊断（可选附带 `--fix` 的自动修复）。
- **Generator** —— 在 bind 之后生成源码：追加新编译单元、往已有类型注入成员、替换被标注的声明。

两类都打包成 **`kind = "analyzer"`** 的 zpkg，由消费方工程的 **`[analyzers]`** 段声明。它们
**只在编译器进程里运行，不链入目标产物**。

`kind = "analyzer"` 做两件事：让这个工程的 `[dependencies]` 够得着编译器的契约包（见
[解析域](#解析域编译期扩展才看得见-compiler-libs)），并声明「我是编译期扩展」——`[analyzers]` 的
path 条目据此校验，`[dependencies]` 据此拒收。只用 analyzer 契约（`z42c.syntax` 在普通 `libs/` 里）
的工程写 `kind = "lib"` 也仍然编得过，但只能按名引用。

> 本页所有代码片段都来自实跑通过的最小工程（2026-09-23；path 条目 2026-09-25）。

## `[analyzers]` 段

```toml
[analyzers]
"demo.noemptycatch" = "0.1.0"
```

值的写法与 `[dependencies]` 相同，但语义不同：

| | `[dependencies]` | `[analyzers]` |
|---|---|---|
| 何时加载 | 编译目标代码时解析符号 | **加载进编译器、编译期执行** |
| 进不进产物 | 进 | **不进** |
| `path = "..."` | 支持，z42c 代为构建整个闭包 | 支持，z42c 代建**那一个工程** |

**一个段覆盖两类**：z42c 从 `[analyzers]` 列出的每个 zpkg 里同时寻找 `: Analyzer`、
`: Generator`、`: ModuleGenerator` 的类型。只含 analyzer 的包发现 0 个 generator，反之亦然。

### 两种条目写法

```toml
[analyzers]
"demo.noemptycatch" = "0.1.0"              # 按名：在依赖目录找 demo.noemptycatch.zpkg
"demo.gen"          = { path = "../gen" }  # 按路径：z42c 代建该工程，取其 dist
```

**按名**：在依赖目录（`Z42_LIBS` 指向的目录或 SDK 的 `libs/`）里找 `<name>.zpkg`。要求你先单独
`z42c build` 那个工程、再把产物拷过去。

> ⚠️ **开发态构建产出的是 indexed zpkg**（主文件 + 旁边散装的 `.zbc`）。只拷主文件过去，
> 加载时会报 **E0493**。要么把散装 `.zbc` 一起拷，要么用 `--release` 构建 handler 工程，
> 得到单文件 packed zpkg。

**按路径**：指向目录，其中须恰有一份**工程清单**（裸 `z42.toml` 优先，否则唯一一份
`*.z42.toml` —— 判据与 `z42c build <dir>`、与 `[dependencies]` 的 path 完全一致），
`[project].name` 与这里写的名字一致，且 `kind = "analyzer"`。z42c 会代为构建它（用消费方的 `--release` / 优化档），再把产物挂上去。
改了扩展的源码，消费方下次构建会重编——handler 指纹含该 zpkg 的内容。**这是自己写扩展时的
推荐写法**：不用把 zpkg 拷来拷去，也不会踩上面那条 indexed/packed 的坑。

代建出来的 zpkg **不进消费方的解析域**（与 `[dependencies]` 的 path 闭包刻意不同）：handler 只活
在编译器进程里，把它的 dist 并进依赖目录就等于让编译期扩展对运行期代码可见。

### 两条校验

| 写法 | 结果 |
|---|---|
| `kind = "analyzer"` 的工程出现在 `[dependencies]` | 报错——它永不链入产物，运行期不会到场 |
| 非 `analyzer` 的工程出现在 `[analyzers]` 的 path 条目 | 报错——否则是「加载成功、发现 0 个 handler、什么都不做」的静默空转 |

两条都只在 **path 条目**上判得出来：按名引用时手上只有 zpkg，而 zpkg 不记 `kind`。

### 声明了却没生效？那会报错

挂上去的 zpkg 若加载成功、里面**一个 handler 都没有**（没有实现 `Analyzer` / `Generator` /
`ModuleGenerator` 的类型），报 **E0496**。成因两个：挂错了包（那是个普通库），或这个 handler 由
**另一代编译器**编出、契约接口对不上——后者用当前工具链重建该 handler 工程即可。

> 此前这种情形**零诊断、退出码 0**：扩展干脆不跑，而编译照常成功。「声明了、也没报错、就是
> 不生效」是最难查的一类，因为没有任何东西提示你去查。

zpkg **格式**代差（handler 由另一代工具链编出、wire 格式已经 bump 过）是另一条路：加载时就被
拦下，报 **E0493**，消息里直接说出两边的格式版本号。

## 写一个 Analyzer

契约在 **`z42c.syntax`**（命名空间 `Z42.Syntax`），依赖 `z42c.core` + `z42c.syntax` 即可：

```toml
[project]
name    = "demo.noemptycatch"
version = "0.1.0"
kind    = "analyzer"

[dependencies]
"z42c.core"   = "0.1.0"
"z42c.syntax" = "0.1.0"
```

> 这两个契约包就在普通 `libs/` 里，所以纯 analyzer 写 `kind = "lib"` 也编得过。但要被
> `[analyzers]` 的 **path 条目**引用，就必须是 `kind = "analyzer"`——那个字段同时是「我是编译期
> 扩展」的声明。

类名**必须以 `Analyzer` 结尾**（E0445 强制）：

```z42
namespace Demo.Lint;

using Z42.Core;
using Z42.Syntax;

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

消费方工程加上 `[analyzers] "demo.noemptycatch" = "0.1.0"` 后，`z42c build` 就会报：

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
warnings-as-errors = true   # 特殊布尔键
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
**「谁报诊断谁产修复」**——包括第三方 `[analyzers]` zpkg。

## 写一个 Generator

Generator 契约（`Generator` / `ModuleGenerator` / `GenTarget` / `GenSink` / `GenContext`）住在
**`z42c.semantics`**，命名空间 `Z42.Semantics`。它比 analyzer 契约深一层，因为
generator 跑在 bind **之后**，拿得到解析后的符号（`Z42ClassType` / `SymbolTable`）。

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

### 解析域：编译期扩展才看得见 `compiler-libs/`

Generator 的契约包 `z42c.semantics.zpkg` **不在 SDK 的 `libs/` 里**——普通工程的依赖解析只看
`libs/`，编译器域的包另落一个平级目录 `compiler-libs/`：

| 工程 | 解析域 | 能引用 `z42c.semantics` 吗 |
|---|---|---|
| `kind = "lib"` / `"exe"` | `libs/`（+ path 依赖闭包）| 否 —— `z42c.semantics 未找到` |
| `kind = "analyzer"` | `libs/` **+ `compiler-libs/`** | 是 |

所以一个 generator 工程的清单长这样：

```toml
[project]
name    = "demo.gen"
version = "0.1.0"
kind    = "analyzer"

[dependencies]
"z42c.semantics" = "0.1.0"
```

消费方用 path 挂上它，全程不需要手工拷 zpkg：

```toml
[analyzers]
"demo.gen" = { path = "../gen" }
```

> 在 2026-09-24 之前这条路是断的：契约 zpkg 只作为 z42c 的 payload 落在 `programs/z42c/`，
> **在 SDK 目录里、却不在解析器会去看的地方**，插件作者拿到的是一句位置在别处的
> `E0443: undefined type: ModuleGenerator`，且没有任何东西会诊断它。引擎、loader、多轮调度
> 当时全是通的——只差包模型里的一个**角色**维度。来龙去脉见
> `docs/spec/changes/add-package-roles/`。
