# z42.cli

## 职责
CLI argv 解析器 — flag / option / positional + auto `-h/--help` 文本生成。
最小子集，参照 Python `argparse` / Rust `clap` / Go `flag`。

**用途**：脚本类 z42 程序解析命令行参数。为 z42 脚本（含 `xtask`）提供 argv parser。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/ArgParser.z42`   | `Std.Cli.ArgParser` — 注册 + Parse + HelpText |
| `src/ParseResult.z42` | `Std.Cli.ParseResult` — GetFlag / GetOption / GetPositional / ShowHelp |
| `src/CliException.z42`| `Std.CliException`（未知 flag / 缺 value / 缺 positional） |
| `src/SubcommandRouter.z42` | `Std.Cli.SubcommandRouter` + `SubcommandMatch` — git/cargo 风格 `Add(name, desc, ArgParser)`（叶子）+ `AddRouter(name, desc, child)`（嵌套）+ `Match(argv)`（单层）/ `Resolve(argv)`（嵌套，逐层下钻）派发；`HelpText()` 该层帮助 |
| `src/CommandResolution.z42` | `Std.Cli.CommandResolution` — `Resolve` 的三态结果（`IsMatch`/`IsHelp`/`IsUnknown` + `Path`/`Result`/`HelpText`/`ErrorMessage`）|

## 入口点

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

## 嵌套子命令（`prog build package …`）

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
| `positional1 positional2 ...` | positional 按声明顺序匹配 |
| `-h` / `--help` | 自动 register；触发 `ShowHelp() == true`，跳过 positional 校验 |

### 必填 vs 可选 positional

- `AddPositional(name, help)` —— **必填**，缺则抛 `CliException`，help 渲染 `<name>`。
- `AddOptionalPositional(name, help)` —— **可选**，未提供时 `GetPositional(i)` 返回 `""`、不报错，help 渲染 `[name]`。
- **排序约束**：可选 positional 必须在所有必填之后声明（否则声明期抛）。典型用法 `prog [release|debug]`、`test vm [interp|jit]`、`build stdlib [lib]`。

## 错误情况（抛 CliException）

- 未知 long flag (`--bogus`)
- 未知 short flag (`-x`)
- option 缺 value (`--profile` 末尾无 value)
- 给 flag 传 value (`--verbose=yes`)
- positional 数量不足（除非 `--help`）
- positional 数量超出 declared
- `GetFlag("undeclared")` / `GetOption("undeclared")` / `GetPositional(超出范围)`

## 依赖关系
依赖 `z42.core`（基础类型 + Exception）+ `z42.text`（StringBuilder 用于
HelpText 拼接）+ `z42.io`（`AddOptionWithEnv` 读环境变量）。

## 进阶特性（完整 API 见 `docs/reference/src/stdlib/cli.md`）

- subcommand 单层（`SubcommandRouter`）+ **嵌套**（`AddRouter`/`Resolve`）
- required option（`AddRequiredOption` + `WasOptionSet`）
- 类型转换 getter（`GetIntOption`/`GetLongOption`/`GetDoubleOption`/`GetBoolOption`）
- env-var fallback（`AddOptionWithEnv`，优先级 argv > env > default）
- mutually-exclusive group（`AddMutuallyExclusive`）
- 可重复 option（`AddRepeatedOption` + `GetRepeatedOption`）
- short flag 串联（`-vxf`）
- strict-vs-extras 透传（`AllowExtras` + `ParseResult.Extras()`）

## 待办

- subcommand 别名（`co` = `commit`）
- 跨子命令全局 flag
- `-nvalue` 紧贴形式（易歧义）

## 如何测试验证

```bash
xtask test stdlib z42.cli    # 本库全部 [Test]
```
