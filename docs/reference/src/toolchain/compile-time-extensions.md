# 编译期扩展：Analyzer 与 Generator

z42 允许把**你自己的代码加载进编译器**，在编译期跑：

- **Analyzer** —— 遍历语法树、报自定义诊断（可选附带 `--fix` 的自动修复）。
- **Generator** —— 在 bind 之后生成源码：追加新编译单元、往已有类型注入成员、替换被标注的声明。

两类都打包成普通的 `kind = "lib"` zpkg，由消费方工程的 **`[analyzers]`** 段声明。它们
**只在编译器进程里运行，不链入目标产物**。

> 本页所有代码片段都来自实跑通过的最小工程（2026-09-23）。

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
| `path = "..."` | 支持，z42c 代为构建整个闭包 | **不支持**（见下） |

**一个段覆盖两类**：z42c 从 `[analyzers]` 列出的每个 zpkg 里同时寻找 `: Analyzer`、
`: Generator`、`: ModuleGenerator` 的类型。只含 analyzer 的包发现 0 个 generator，反之亦然。

### 限制：不支持 `path`

`[analyzers]` **不建依赖闭包、也不代为构建**。写了 `path` 会得到一条明确的拒绝：

```
z42c build: [analyzers] `demo.x` 指定了 `path`，但 [analyzers] 段尚不支持 path 依赖……
```

正确做法：先单独 `z42c build` 那个工程，把产出的 `<name>.zpkg` 放进依赖目录
（`Z42_LIBS` 指向的目录或 SDK 的 `libs/`），清单里只写名字与版本。

> ⚠️ **开发态构建产出的是 indexed zpkg**（主文件 + 旁边散装的 `.zbc`）。只拷主文件过去，
> 加载时会报 **E0493**。要么把散装 `.zbc` 一起拷，要么用 `--release` 构建 handler 工程，
> 得到单文件 packed zpkg。

## 写一个 Analyzer

契约在 **`z42c.syntax`**（命名空间 `Z42.Syntax`），依赖 `z42c.core` + `z42c.syntax` 即可：

```toml
[project]
name    = "demo.noemptycatch"
version = "0.1.0"
kind    = "lib"

[dependencies]
"z42c.core"   = "0.1.0"
"z42c.syntax" = "0.1.0"
```

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

### ⚠️ 现状：外部 generator 还写不了

**`z42c.semantics.zpkg` 目前不在 SDK 的 `libs/` 里**（它住在 `programs/z42c/`，那不是依赖解析
的地方）。因此消费方工程写 `[dependencies] "z42c.semantics"` 会得到：

```
E0443: undefined type: ModuleGenerator
```

引擎、loader、多轮调度都是通的 —— 只有「契约包没被 ship 到用户能解析到的目录」这一件事挡着。
这属于包模型缺一个**角色**维度（运行时库 / 编译期契约），修法见
`docs/spec/changes/add-package-roles/`。在那之前，generator 只能在编译器自身的构建里用
（如内建的 `[Forward]`）。
