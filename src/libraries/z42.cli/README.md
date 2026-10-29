# z42.cli

## 职责
CLI argv 解析器 — flag / option / positional + auto `-h/--help` 文本生成，外加其上的
**声明式命令层**（命令 = 类、参数 = 字段，参照 C# System.CommandLine / Rust clap derive）。

**用途**：脚本类 z42 程序解析命令行参数。为 z42 脚本（含 `xtask`）提供 argv parser。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/ArgParser.z42`   | `Std.Cli.ArgParser` — 注册 + Parse + HelpText |
| `src/ParseResult.z42` | `Std.Cli.ParseResult` — GetFlag / GetOption / GetPositional / ShowHelp |
| `src/CliException.z42`| `Std.CliException`（未知 flag / 缺 value / 缺 positional） |
| `src/SubcommandRouter.z42` | `Std.Cli.SubcommandRouter` + `SubcommandMatch` — git/cargo 风格 `Add(name, desc, ArgParser)`（叶子）+ `AddRouter(name, desc, child)`（嵌套）+ `Match(argv)`（单层）/ `Resolve(argv)`（嵌套，逐层下钻）派发；`HelpText()` 该层帮助 |
| `src/Command.z42` / `src/Command.Params.z42` | `Std.Cli.Command` — 声明式命令：`AddCommand` 搭层级、`Invoke(argv)` 解析 + 绑定 + 执行；参数表（字段 + 挂上的对象）的声明与绑定 |
| `src/Option.z42` / `src/Argument.z42` | 参数层最基础的对象：声明 + 解析后的 `Value` / `IsSet`（`AddOption(option)` / `AddArgument(argument)`） |
| `src/CliOption.z42` | `CliOption<T>` / `CliArgument<T>` — `AddOption<T>` / `AddArgument<T>` 返回的强类型句柄 |
| `src/CommandAttributes.z42` | `OptionAttribute` / `ArgumentAttribute`（字段写法；全部状态走命名实参） |
| `src/_ParamType.z42` | 值类型规则：类型类别 / 默认值 / choices / 取值转换 / kebab-case（内部） |
| `src/CommandResolution.z42` | `Std.Cli.CommandResolution` — `Resolve` 的三态结果（`IsMatch`/`IsHelp`/`IsUnknown` + `Path`/`Result`/`HelpText`/`ErrorMessage`）|

## 入口点
公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/cli.md)（由源码 doc 注释生成）。

```z42
using Std.Cli;
using Std.IO;

void Main() {
    var p = new ArgParser("build-stdlib", "Compile z42 stdlib workspace");
    p.AddFlag("verbose", "v", "verbose output");
    p.AddOption("profile", "p", "build profile", "release");
    p.AddPositional("target", "build target");

    ParseResult r;
    try {
        r = p.Parse(Environment.GetCommandLineArgs());
    } catch (CliException e) {
        ConsoleError.WriteLine("error: " + e.Message);
        Environment.Exit(2);
    }

    if (r.ShowHelp()) {
        Console.WriteLine(p.HelpText());
        Environment.Exit(0);
    }

    bool verbose  = r.GetFlag("verbose");
    string prof   = r.GetOption("profile");
    string target = r.GetPositional(0);
    // ...
}
```

## 声明式命令（推荐写法）

一个命令一个类：参数是贴了 `[Option]` / `[Argument]` 的字段（初始值即默认值），动作是 `Run()`；
层级由父命令在构造器里 `AddCommand(child)` 搭起。根命令 `Invoke(argv)` 一次完成解析、绑定、执行：

```z42
using Std.Cli;
using Std.IO;

enum Mode { Interp, Jit, Both }

class E2eCommand : Command {
    [Option(help: "execution mode")] public Mode Mode = Mode.Both;      // --mode {interp|jit|both}
    [Option(help: "parallel jobs", shortName: "j")] public int Jobs = 1;
    [Option(help: "skip rebuild", alias: new string[]{ "no-rebuild" })] public bool NoBuild;  // --no-build
    [Argument(help: "case name")] public string File = "";
    public E2eCommand() : base("e2e", "end-to-end suite") {}
    public override int Run() { /* 用 this.Mode / this.Jobs … */ return 0; }
}

class TestCommand : Command {
    [Option(help: "skip named stages")] public string Skip = "";
    public TestCommand() : base("test", "run tests") {
        this.SetDefaultAction("the full gate");   // 裸 `test` / `test --skip x` 执行本类 Run()
        this.AddCommand(new E2eCommand());
    }
    public override int Run() { return 0; }
}

class App : Command {
    [Option(help: "q|m|n|d", shortName: "v", global: true)] public string Verbosity = "minimal";
    public App() : base("xtask", "dev CLI") { this.AddCommand(new TestCommand()); }
    public override void BeforeRun() { /* 全局 option 落成副作用 */ }
}

void Main() { Environment.Exit(new App().Invoke(Environment.GetCommandLineArgs())); }
```

| 规则 | 说明 |
|------|------|
| 参数名 | `name:` 显式给；否则字段名转 kebab-case（`NoBuild` → `--no-build`） |
| 字段类型 | `bool` = flag；`string` / `int` / `long` / `double` / enum = 单值；`string[]` = 可重复 option / 变长参数 |
| choices | `choices:` 显式给；enum 字段自动取成员名（kebab-case），帮助里显示 `{a|b}`，不在其中报错并给建议 |
| 别名 | `[Option(alias: …)]` 给 option 额外 long 名；`AddCommand(child, aliases)` 给子命令别名 |
| 必填 | `required: true`（option / argument 默认都可选；bool 不能必填） |
| 全局 | `[Option(global: true)]`：所在命令整棵子树都认，子命令前后皆可写 |
| 透传 | `[Argument(passthrough: true)] string[]`：固定参数之后的 token 原样收集（`run <app> …`） |
| 运行期才知道的 | attribute 实参只能是常量；choices / 帮助要现算时用 `AddOption<T>` / `AddArgument<T>`（见下） |
| 退出码 | 帮助 → 0；未知命令 / 参数错误（含声明错误）→ stderr + 2；否则 `Run()` 的返回值 |

参数的三种写法共用同一份 `Option` / `Argument` 规格（同样的类型规则、校验与帮助），可在一个命令里混用：

```z42
class SetupCommand : Command {
    [Option(help: "force reinstall")] public bool Force;          // ① 字段：静态参数
    private CliOption<string> _os;                                // ② 强类型句柄：运行期 choices
    private Option _level;                                        // ③ 基础对象
    public SetupCommand() : base("setup", "prepare this machine") {
        this._os = this.AddOption<string>("os", "", help: "platform", choices: Platforms());
        this._level = new Option("level", typeof(int), 1, help: "detail level");
        this.AddOption(this._level);
    }
    public override int Run() { return Install(this._os.Value, (int)this._level.Value, this.Force); }
}
```

`AddOption<T>` / `AddArgument<T>` 须写明类型实参（`T` 在运行期用到）。位置参数先按字段声明序落槽，再按
`AddArgument` 的调用序。

执行顺序：解析 → 绑定路径上各命令的参数（字段来源的回写字段）→ 沿根 → 目标调 `BeforeRun()` → 目标 `Run()`。
子命令经 `ParentCommand()` 取祖先（读全局 option 字段）。只要解析不执行用 `Resolve(argv)`，
再 `Bind(res)` 得到绑定好的目标命令。

## 嵌套子命令（底层 API：`prog build package …`）

多层命令树用 `AddRouter` 挂子 router，`Resolve` 逐层下钻，每层各自拦 `-h`：

```z42
var package = new ArgParser("xtask build package", "assemble SDK package");
package.AddOption("rid", "", "target RID", "");

var build = new SubcommandRouter("xtask build", "build components");
build.Add("package", "assemble SDK package", package);   // leaf
// build.AddRouter("sub", ...)                            // 或再挂一层

var root = new SubcommandRouter("xtask", "z42 repo dev CLI");
root.AddRouter("build", "build components", build);       // 嵌套
root.Add("audit", "scan usings", auditParser);            // 叶子与嵌套并存

CommandResolution res = root.Resolve(Environment.GetCommandLineArgs());
if (res.IsHelp())    { Console.WriteLine(res.HelpText()); Environment.Exit(0); }
if (res.IsUnknown()) { ConsoleError.WriteLine(res.ErrorMessage());
                       Console.WriteLine(res.HelpText()); Environment.Exit(2); }
string[] path = res.Path();    // 例 ["build", "package"]
ParseResult r = res.Result();  // 叶子解析结果
```

- `xtask build package -h` / `xtask build -h` / `xtask -h` 均归 `IsHelp()`，打印**对应层**帮助（叶子层走该 `ArgParser.HelpText()`）。
- 任一 router 层未知 token → `IsUnknown()`，带该层帮助。
- 叶子 option 级错误（未知 flag / 缺 value）仍抛 `CliException`，`Resolve` 不吞 —— 消费端 try/catch。
- 单层场景用 `Match` / `SubcommandMatch`。

## 支持的 argv 形式

| 形式 | 含义 |
|------|------|
| `--flag` / `-f` | boolean flag → true |
| `--name value` | option with value |
| `--name=value` | option with value (inline) |
| `-n value`     | short-name option |
| `-nvalue` / `-n=4` | 紧贴短参（首字符是 option 短名，余下整段为值） |
| `--`           | 结束选项：其后一切 token（含 `-h` / `--x` / 负数）作 positional |
| `-1` / `-2.5`  | 负数字面量：非已注册 option 且有 positional 容量时按 positional 处理 |
| `positional1 positional2 ...` | positional 按声明顺序匹配，尾部可挂变长槽收集剩余 |
| `-h` / `--help` | 自动 register；触发 `ShowHelp() == true`，跳过 positional 校验 |

### 必填 vs 可选 positional

- `AddPositional(name, help)` —— **必填**，缺则抛 `CliException`，help 渲染 `<name>`。
- `AddOptionalPositional(name, help)` —— **可选**，未提供时 `GetPositional(i)` 返回 `""`、不报错，help 渲染 `[name]`。
- **排序约束**：可选 positional 必须在所有必填之后声明（否则声明期抛）。典型用法 `prog [release|debug]`、`test vm [interp|jit]`。

## 错误情况（抛 CliException）

- 未知 long flag (`--bogus`)——编辑距离足够近时附 `(did you mean '--xxx'?)`
- 未知 short flag (`-x`)
- option 缺 value (`--profile` 末尾无 value)
- 给 flag 传 value (`--verbose=yes`)
- positional 数量不足（除非 `--help`）
- positional 数量超出 declared（声明变长槽后多余 token 进变长槽，不报错）
- **重复注册**（long / short / positional 名 / 子命令名 / 别名互撞；`help` / `h` 保留）——声明期报错，不再静默先注册者胜
- `GetFlag("undeclared")` / `GetOption("undeclared")` / `GetPositional(超出范围)`

## 依赖关系
依赖 `z42.core`（基础类型 + Exception）+ `z42.text`（StringBuilder 用于
HelpText 拼接）+ `z42.io`（`AddOptionWithEnv` 读环境变量）。

## 进阶特性（完整 API 见 `docs/reference/src/stdlib/cli.md`）

- subcommand 单层（`SubcommandRouter`）+ **嵌套**（`AddRouter`/`Resolve`）
- **子命令别名**（`Add(name, desc, parser, aliases)`，命中归一为正名，撞名声明期报错）
- **option long 名别名**（`AddAlias(longName, alias)` / router `AddPersistentAlias`，帮助里与正名同列）
- **原样透传尾参**（`AddPassthrough`：固定槽填满后其余 token 不做解析，供「转交给另一个程序」；各层 persistent 旗标只在透传叶子命令名之前剥离）
- **跨子命令全局旗标**（`AddPersistentFlag` / `AddPersistentOption`：路由前从 argv 任意位置剥出；`CommandResolution.PersistentFlag` / `PersistentOption` 取值，多层各管各的子树、根层优先）
- **节点默认动作**（`SetDefault(parser, desc)`：argv 为空或首 token 是 flag 且不命中任何子命令时整段交给默认 parser——`xtask test --no-build` 这类「无子命令 = 默认全量」形态；本层 `-h` 附 DEFAULT ACTION OPTIONS 段）
- **变长尾参**（`AddVariadicPositional` + `ParseResult.VariadicValues()`，收集剩余全部 positional 含 `--` 之后）
- **`--` 分隔符**（其后 token 一律 positional）
- **紧贴短参** `-nvalue` / `-n=4` + **负数字面量回退**（`-1` 作 positional）
- **拼写建议**（未知 long option / 未知子命令，编辑距离 ≤2 且 ≤ len/2）
- **重复注册声明期报错**（含 flag 与 option 之间同 long）
- required option（`AddRequiredOption` + `WasOptionSet`）
- 类型转换 getter（`GetIntOption`/`GetLongOption`/`GetDoubleOption`/`GetBoolOption`）
- env-var fallback（`AddOptionWithEnv`，优先级 argv > env > default）
- mutually-exclusive group（`AddMutuallyExclusive`）
- 可重复 option（`AddRepeatedOption` + `GetRepeatedOption`）
- short flag 串联（`-vxf`，仅 flag；簇内嵌 option 短名仍报错）
- strict-vs-extras 透传（`AllowExtras` + `ParseResult.Extras()`）

## 待办

- 科学计数负数（`-1e5`）不回退（`e` 更像短名）——刻意行为，重新评估时先看误吞风险

## 如何测试验证

```bash
xtask test stdlib z42.cli    # 本库全部 [Test]
```

## 关联文档

- 用法与规则（参考手册）：[z42.cli](../../../docs/reference/src/stdlib/cli.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)
