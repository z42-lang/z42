# z42.cli —— 命令行参数解析

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.cli/`；命名空间 `Std.Cli`（`CliException` 在 `Std`）

把 `string[] argv` 解析成 flag / option / positional 三类参数，并自动生成 `-h/--help`
文本。`SubcommandRouter` 在其上叠一层 `git` / `cargo` 风格的（可嵌套的）子命令路由。

参数值一律先当 `string` 拿；需要 `int` / `long` / `double` / `bool` 时在**取值处**转换
（`ParseResult.GetIntOption` 等），声明期不区分类型。

argv 从 `Std.IO.Environment.GetCommandLineArgs()` 取，**不含 program name**，直接传给
`Parse`。用 `z42 run` 跑单文件脚本时，脚本自己的参数写在 `--` 之后：

```bash
./.z42/z42 run tool.z42 -- --profile debug build
# 脚本内 GetCommandLineArgs() → ["--profile", "debug", "build"]
```

## ArgParser

声明参数并解析。所有 `Add*` 方法按调用顺序记录；`Parse` 可对同一个 parser 重复调用。

```z42
public class ArgParser {
    public ArgParser(string programName, string description)

    // 声明
    public void AddFlag(string longName, string shortName, string help)
    public void AddOption(string longName, string shortName, string help, string defaultValue)
    public void AddOptionWithEnv(string longName, string shortName, string help,
                                 string defaultValue, string envVarName)
    public void AddRequiredOption(string longName, string shortName, string help)
    public void AddRepeatedOption(string longName, string shortName, string help)
    public void AddMutuallyExclusive(string[] names)
    public void AddPositional(string name, string help)
    public void AddOptionalPositional(string name, string help)
    public void AllowExtras()

    // 解析与帮助
    public ParseResult Parse(string[] argv)
    public string HelpText()
}
```

| 成员 | 说明 |
|---|---|
| `AddFlag` | 布尔开关，出现即 `true`。`shortName` 传 `""` 表示无短名 |
| `AddOption` | 带值 option；未出现时取 `defaultValue` |
| `AddOptionWithEnv` | 同上，另加环境变量后备。取值优先级 **argv > 环境变量 > `defaultValue`**；环境变量不存在或为空串时落到 `defaultValue`。help 文本自动追加 ` (env: $NAME)` |
| `AddRequiredOption` | 必填 option；argv 里没写 → `Parse` 抛 `CliException`。默认值固定为 `""`，help 自动追加 ` (required)` |
| `AddRepeatedOption` | 可重复 option，每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取**最后一个**。默认值固定 `""`，help 自动追加 ` (repeatable)` |
| `AddMutuallyExclusive` | 声明一组 option long-name，argv 中至多能出现一个。名字必须**已注册**，否则当场抛 `CliException` |
| `AddPositional` | 必填位置参数，按声明顺序匹配 |
| `AddOptionalPositional` | 可选位置参数，缺省时 `GetPositional(i)` 返回 `""`。**必须在所有必填 positional 之后声明**，否则 `AddPositional` 抛 `CliException` |
| `AllowExtras` | 一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常 |
| `Parse` | 解析 argv，返回 `ParseResult`；失败抛 `CliException` |
| `HelpText` | 生成 `USAGE` / `FLAGS` / `OPTIONS` / `ARGS` 四段文本 |

### 识别的 token 形式

| 形式 | 含义 |
|---|---|
| `--name value` | long option 取下一个 token 作值 |
| `--name=value` | long option 就地取值（`--name=` 得到空串） |
| `-n value` | short option 取下一个 token 作值 |
| `--flag` / `-f` | flag 置 `true` |
| `-abc` | short flag 簇，等价 `-a -b -c`；**只允许布尔 flag**，次序无关 |
| `-h` / `--help` | 任意位置都识别，置 `ShowHelp()` 为 `true` |
| 其它 | 按声明顺序填 positional |

`-h` / `--help` 出现时，`Parse` **跳过**「缺必填 positional」「缺必填 option」「互斥冲突」
三项校验，让调用方能在没有其它参数时打印帮助。

### 解析期错误

全部以 `CliException` 抛出（消息原文）：

| 情形 | 消息 |
|---|---|
| 未知 long | `unknown option '--x'` |
| 未知 short / 簇内未知字符 | `unknown option '-x'` |
| flag 带了值 | `flag '--x' does not take a value` |
| option 缺值（在 argv 末尾） | `option '--x' requires a value` |
| 簇里混进了 option 短名 | `short-flag cluster '-vp' contains option '-p' that requires a value (clusters are flags only)` |
| positional 少了 | `missing positional argument: 'target'` |
| positional 多了 | `unexpected positional argument 'c' (expected 2)` |
| 必填 option 缺失 | `required option '--name' missing` |
| 互斥冲突 | `options '--json' and '--yaml' are mutually exclusive` |

## ParseResult

`Parse` 的返回值。取值方法一律按 **long name** 查找；名字未注册抛 `CliException`。

```z42
public class ParseResult {
    public bool     GetFlag(string longName)
    public string   GetOption(string longName)
    public bool     WasOptionSet(string longName)
    public string[] GetRepeatedOption(string longName)
    public string[] Extras()
    public int      ExtrasCount()

    public int    GetIntOption(string longName)
    public long   GetLongOption(string longName)
    public double GetDoubleOption(string longName)
    public bool   GetBoolOption(string longName)

    public string GetPositional(int index)
    public int    PositionalCount()
    public bool   ShowHelp()
}
```

| 成员 | 说明 |
|---|---|
| `GetFlag` | flag 是否出现过 |
| `GetOption` | option 的最终值（argv → env → default 三级解析后的结果） |
| `WasOptionSet` | 该 option 是否**真的写在 argv 里**（区分「用户显式传了默认值」和「默认值兜底」）。env 后备命中时仍是 `false` |
| `GetRepeatedOption` | 按 argv 顺序返回全部值；没传过返回空数组。对非 `AddRepeatedOption` 声明的 option 抛 `CliException` |
| `Extras()` / `ExtrasCount()` | `AllowExtras` 收集到的未知 token（原样保留，含 `--` 前缀）。未开启时为空数组 / `0` |
| `GetIntOption` / `GetLongOption` / `GetDoubleOption` | 取值后分别走 `int.Parse` / `long.Parse` / `double.Parse`；失败抛 `CliException`（`option '--port' value 'abc' is not a valid int`） |
| `GetBoolOption` | 只认 `true` / `false`（ASCII 大小写不敏感），其它值抛 `CliException`。用于 `--enabled=true` 这种显式布尔 option，跟 `GetFlag` 的「出现即真」不是一回事 |
| `GetPositional` | 0-based。边界是**声明总数**而非已提供数：已声明未提供的可选 positional 返回 `""`，越过声明总数抛 `CliException` |
| `PositionalCount` | argv 里**实际提供**的 positional 个数 |
| `ShowHelp` | argv 里出现过 `-h` / `--help` |

## SubcommandRouter

子命令路由。每个子命令要么是一个叶子 `ArgParser`，要么是一个嵌套 `SubcommandRouter`。
同名重复注册**覆盖**先前那条（叶子与嵌套可互相覆盖）。

```z42
public sealed class SubcommandRouter {
    public SubcommandRouter(string programName, string description)

    public void Add(string name, string description, ArgParser parser)
    public void AddRouter(string name, string description, SubcommandRouter child)

    public CommandResolution Resolve(string[] argv)
    public SubcommandMatch   Match(string[] argv)

    public string HelpText()
    public int    Count()
    public bool   Has(string name)
}

public sealed class SubcommandMatch {
    public SubcommandMatch(string name, ParseResult result)
    public string      Name()
    public ParseResult Result()
}
```

`Resolve` 是推荐入口：递归下钻整棵命令树，返回三态互斥的 `CommandResolution`。
`Match` 是单层 sugar，只看 `argv[0]`，命中叶子返回 `SubcommandMatch`，其余
（argv 为空 / `argv[0]` 是 `-h`、`--help` / 名字不认识）一律返回 `null`；
**它不处理嵌套子命令**（见「不支持」）。

`HelpText()` 渲染 `programName - description` 加一段 `SUBCOMMANDS:` 列表。

### CommandResolution

```z42
public sealed class CommandResolution {
    public CommandResolution(int kind, string[] path, ParseResult result,
                             string helpText, string errorMessage)

    public static CommandResolution Match(ParseResult result)
    public static CommandResolution Help(string helpText)
    public static CommandResolution Unknown(string errorMessage, string helpText)

    public bool IsMatch()
    public bool IsHelp()
    public bool IsUnknown()

    public string[]    Path()
    public ParseResult Result()
    public string      HelpText()
    public string      ErrorMessage()
    public CommandResolution Prepend(string name)
}
```

`IsMatch` / `IsHelp` / `IsUnknown` 恰好一个为真：

| 状态 | 何时 | 有效取值 |
|---|---|---|
| `IsMatch()` | 命中叶子命令且没请求 help | `Result()` = 叶子的 `ParseResult`；`Path()` = 命中链，如 `["build", "package"]` |
| `IsHelp()` | 任一层遇到 `-h` / `--help`，或某 router 层没有剩余 token，或叶子的 `ShowHelp()` 为真 | `HelpText()` = **该层**的帮助（router 层列子命令，叶子层走 `ArgParser.HelpText()`）；`Path()` = 已走过的链 |
| `IsUnknown()` | 某 router 层遇到不认识的 token | `ErrorMessage()`（`git build: unknown command 'zzz'`）+ 该层 `HelpText()`；`Path()` = 已走过的链 |

叶子 `ArgParser.Parse` 的错误（未知 flag、缺值、缺 positional）以 `CliException`
**向上透传**，`Resolve` 不吞。

## CliException

```z42
namespace Std;

public class CliException : Exception {
    public CliException(string message)
    override string ToString()   // "CliException: <message>"
}
```

声明期（`AddMutuallyExclusive` 名字未注册、必填 positional 排在可选之后）与解析期
（上表全部情形）与取值期（名字未注册、类型转换失败、positional 越界）统一用它。

## 用法

```z42
using Std;
using Std.IO;
using Std.Cli;

void Main() {
    var p = new ArgParser("build", "compile the workspace");
    p.AddFlag("verbose", "v", "verbose output");
    p.AddOption("profile", "p", "build profile", "release");
    p.AddOptionWithEnv("token", "t", "API token", "", "API_TOKEN");
    p.AddRepeatedOption("define", "D", "extra define k=v");
    p.AddPositional("target", "build target directory");
    p.AddOptionalPositional("stage", "optional stage name");

    try {
        ParseResult r = p.Parse(Environment.GetCommandLineArgs());
        if (r.ShowHelp()) {
            Console.WriteLine(p.HelpText());
            Environment.Exit(0);
        }
        bool verbose  = r.GetFlag("verbose");
        string prof   = r.GetOption("profile");
        string[] defs = r.GetRepeatedOption("define");
        string target = r.GetPositional(0);
        string stage  = r.GetPositional(1);      // 未提供 → ""
    } catch (CliException e) {
        ConsoleError.WriteLine(e.Message);
        Console.WriteLine(p.HelpText());
        Environment.Exit(2);
    }
}
```

子命令树：

```z42
var pkg = new ArgParser("xtask build package", "package artifacts");
pkg.AddFlag("release", "r", "release mode");

var build = new SubcommandRouter("xtask build", "build things");
build.Add("package", "package artifacts", pkg);

var root = new SubcommandRouter("xtask", "repo task runner");
root.AddRouter("build", "build things", build);

CommandResolution res = root.Resolve(Environment.GetCommandLineArgs());
if (res.IsHelp())    { Console.WriteLine(res.HelpText()); Environment.Exit(0); }
if (res.IsUnknown()) { ConsoleError.WriteLine(res.ErrorMessage());
                       Console.WriteLine(res.HelpText()); Environment.Exit(2); }
string[] path = res.Path();     // ["build", "package"]
ParseResult r = res.Result();
```

`HelpText()` 输出形如：

```
build — compile the workspace

USAGE:
    build [OPTIONS] <target> [stage]

FLAGS:
    -v, --verbose    verbose output
    -h, --help    Print this help and exit

OPTIONS:
    -p, --profile <profile>    build profile  [default: release]
    -D, --define <define>    extra define k=v (repeatable)  [default: ]

ARGS:
    <target>    build target directory
    [stage]    optional stage name
```

## 不支持

- **`--` 结束选项**：`--` 不是分隔符，会当成未知 option 报
  `unknown option '--'`。（`z42 run script.z42 -- ...` 里的 `--` 由启动器消费，到不了
  `ArgParser`。）
- **负数 / 以 `-` 开头的值作 positional**：`-1` 会走 short 解析路径报
  `unknown option '-1'`。单个 `-` 长度不足 2，反而被当作 positional。
- **紧贴形式 `-pvalue`**：不支持。会落进 short flag 簇路径，报
  `contains option '-p' that requires a value`。
- **变长 positional（`args...`）**：不支持；用可重复 option（`-D k=v`）或调用方自己切
  `Extras()` 替代。
- **声明期类型**：没有 `AddIntOption` 之类；一律 `AddOption`，在 `GetIntOption` 等取值
  方法里转换。
- **重名检测**：重复注册同一个 long name 不报错，查找取**先声明**的那条。
- **子命令别名 / 跨子命令全局 flag**：没有；共享 flag 要在每个叶子 `ArgParser` 里各注册
  一遍。
- **拼写纠正**：未知子命令只给 `unknown command '<token>'`，没有 "did you mean" 建议。
- **`Match` 不认嵌套**：`AddRouter` 注册的名字用 `Match` 去命中会撞上空的叶子 parser 而
  崩溃；嵌套树只能用 `Resolve`。
- **`AllowExtras` 与 short flag 簇互斥**：开启 extras 后不再走簇解析，`-abc` 整个 token
  进 `Extras()`。

> `_RepeatedList` / `_MutexGroup` 以及 `ParseResult` 上 `_` 开头的字段虽然写着
> `public`，是库内部接线，不属于公开面。
