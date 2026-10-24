<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.cli/ -->

# z42.cli —— 命令行参数解析。

> 包路径 `src/libraries/z42.cli/`；命名空间 `Std.Cli`

把 `string[] argv` 解析成 flag / option / positional 三类参数，并自动生成
`-h/--help` 文本。`SubcommandRouter` 在其上叠一层 `git` / `cargo` 风格的
（可嵌套的）子命令路由。

参数值一律先当 `string` 拿；需要 `int` / `long` / `double` / `bool` 时在
**取值处**转换（`ParseResult.GetIntOption` 等），声明期不区分类型。

argv 从 `Std.IO.Environment.GetCommandLineArgs()` 取，**不含 program name**，
直接传给 `Parse`。

## 不支持

- **`--` 结束选项**：`--` 不是分隔符，会当成未知 option 报 `unknown option '--'`。
- **负数 / 以 `-` 开头的值作 positional**：`-1` 会走 short 解析路径报
  `unknown option '-1'`；单个 `-` 长度不足 2，反而被当作 positional。
- **紧贴形式 `-pvalue`**：不支持，会落进 short flag 簇路径，报
  `contains option '-p' that requires a value`。
- **变长 positional（`args...`）**：不支持；用可重复 option（`-D k=v`）或调用方
  自己切 `Extras()` 替代。
- **声明期类型**：没有 `AddIntOption` 之类；一律 `AddOption`，在 `GetIntOption`
  等取值方法里转换。
- **重名检测**：重复注册同一个 long name 不报错，查找取**先声明**的那条。
- **子命令别名 / 跨子命令全局 flag**：没有；共享 flag 要在每个叶子 `ArgParser`
  里各注册一遍。
- **拼写纠正**：未知子命令只给 `unknown command '<token>'`，没有 "did you mean" 建议。
- **`Match` 不认嵌套**：`AddRouter` 注册的名字用 `Match` 去命中会撞上空的叶子 parser
  而崩溃；嵌套树只能用 `Resolve`。
- **`AllowExtras` 与 short flag 簇互斥**：开启 extras 后不再走簇解析，`-abc`
  整个 token 进 `Extras()`。

## ArgParser

[`src/libraries/z42.cli/src/ArgParser.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L37)

声明参数并解析 argv。所有 Add* 方法按调用顺序记录；Parse 可对同一 parser 重复调用。

```z42
public class ArgParser {
    ArgParser(string programName, string description)
    void AddFlag(string longName, string shortName, string help)
    void AddOption(string longName, string shortName, string help, string defaultValue)
    void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)
    void AddMutuallyExclusive(string[] names)
    void AddRepeatedOption(string longName, string shortName, string help)
    void AllowExtras()
    void AddRequiredOption(string longName, string shortName, string help)
    void AddPositional(string name, string help)
    void AddOptionalPositional(string name, string help)
    ParseResult Parse(string[] argv)
    string HelpText()
}
```

成员：[`ArgParser`](#Std.Cli.ArgParser.ctor(string,string)) · [`AddFlag`](#Std.Cli.ArgParser.AddFlag(string,string,string)) · [`AddOption`](#Std.Cli.ArgParser.AddOption(string,string,string,string)) · [`AddOptionWithEnv`](#Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)) · [`AddMutuallyExclusive`](#Std.Cli.ArgParser.AddMutuallyExclusive(string[])) · [`AddRepeatedOption`](#Std.Cli.ArgParser.AddRepeatedOption(string,string,string)) · [`AllowExtras`](#Std.Cli.ArgParser.AllowExtras()) · [`AddRequiredOption`](#Std.Cli.ArgParser.AddRequiredOption(string,string,string)) · [`AddPositional`](#Std.Cli.ArgParser.AddPositional(string,string)) · [`AddOptionalPositional`](#Std.Cli.ArgParser.AddOptionalPositional(string,string)) · [`Parse`](#Std.Cli.ArgParser.Parse(string[])) · [`HelpText`](#Std.Cli.ArgParser.HelpText())

<a id="Std.Cli.ArgParser.ctor(string,string)"></a>
### ArgParser(string programName, string description)

[`src/libraries/z42.cli/src/ArgParser.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L78)

用 programName 与 description 创建一个参数解析器。

<a id="Std.Cli.ArgParser.AddFlag(string,string,string)"></a>
### void AddFlag(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L105)

布尔开关：出现即 true。shortName 传 "" 表示无短名；`-h` / `--help` 自动注册，无需手加。

<a id="Std.Cli.ArgParser.AddOption(string,string,string,string)"></a>
### void AddOption(string longName, string shortName, string help, string defaultValue)

[`src/libraries/z42.cli/src/ArgParser.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L114)

注册带值 option（`--name value` 或 `--name=value`）；未出现时取 defaultValue。

<a id="Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)"></a>
### void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)

[`src/libraries/z42.cli/src/ArgParser.z42:129`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L129)

注册带环境变量后备的 option。取值优先级 argv > 环境变量 > defaultValue。

环境变量不存在或为空串时落到 defaultValue；help 自动追加 " (env: $NAME)"。

<a id="Std.Cli.ArgParser.AddMutuallyExclusive(string[])"></a>
### void AddMutuallyExclusive(string[] names)

[`src/libraries/z42.cli/src/ArgParser.z42:143`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L143)

声明一组 option long-name，argv 中至多出现一个；名字必须已注册，否则当场抛 CliException。

<a id="Std.Cli.ArgParser.AddRepeatedOption(string,string,string)"></a>
### void AddRepeatedOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:178`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L178)

注册可重复 option：每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取最后一个。

默认值固定 ""，help 自动追加 " (repeatable)"。

<a id="Std.Cli.ArgParser.AllowExtras()"></a>
### void AllowExtras()

[`src/libraries/z42.cli/src/ArgParser.z42:191`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L191)

一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常。

<a id="Std.Cli.ArgParser.AddRequiredOption(string,string,string)"></a>
### void AddRequiredOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:197`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L197)

注册必填 option：argv 里没写时 Parse 抛 CliException。默认值固定 ""， help 自动追加 " (required)"。

<a id="Std.Cli.ArgParser.AddPositional(string,string)"></a>
### void AddPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:210`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L210)

注册必填 positional，按声明顺序匹配 argv 中非 flag/option 的部分。

<a id="Std.Cli.ArgParser.AddOptionalPositional(string,string)"></a>
### void AddOptionalPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:227`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L227)

注册可选 positional：缺省时 `GetPositional(i)` 返回 ""。

必须在所有必填 positional 之后声明，否则 `AddPositional` 抛 CliException。
help 中渲染为 `[name]`。

<a id="Std.Cli.ArgParser.Parse(string[])"></a>
### ParseResult Parse(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.z42:238`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L238)

解析 argv（不含 program name），返回 ParseResult；失败抛 CliException。

<a id="Std.Cli.ArgParser.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/ArgParser.z42:503`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L503)

生成 USAGE / FLAGS / OPTIONS / ARGS 四段帮助文本。

## CliException

[`src/libraries/z42.cli/src/CliException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CliException.z42#L4)

CLI 声明期、解析期、取值期错误的统一异常；ToString() 输出 "CliException: <message>"。

```z42
public class CliException : Exception {
    CliException(string message)
}
```

成员：[`CliException`](#Std.Cli.CliException.ctor(string))

<a id="Std.Cli.CliException.ctor(string)"></a>
### CliException(string message)

[`src/libraries/z42.cli/src/CliException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CliException.z42#L6)

用 message 创建异常。

## CommandResolution

[`src/libraries/z42.cli/src/CommandResolution.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L4)

SubcommandRouter.Resolve 的结果：IsMatch / IsHelp / IsUnknown 恰好一个为真。

```z42
public sealed class CommandResolution {
    CommandResolution(int kind, string[] path, ParseResult result, string helpText, string errorMessage)
    static CommandResolution Match(ParseResult result)
    static CommandResolution Help(string helpText)
    static CommandResolution Unknown(string errorMessage, string helpText)
    bool IsMatch()
    bool IsHelp()
    bool IsUnknown()
    string[] Path()
    ParseResult Result()
    string HelpText()
    string ErrorMessage()
    CommandResolution Prepend(string name)
}
```

成员：[`CommandResolution`](#Std.Cli.CommandResolution.ctor(int,string[],ParseResult,string,string)) · [`Match`](#Std.Cli.CommandResolution.Match(ParseResult)) · [`Help`](#Std.Cli.CommandResolution.Help(string)) · [`Unknown`](#Std.Cli.CommandResolution.Unknown(string,string)) · [`IsMatch`](#Std.Cli.CommandResolution.IsMatch()) · [`IsHelp`](#Std.Cli.CommandResolution.IsHelp()) · [`IsUnknown`](#Std.Cli.CommandResolution.IsUnknown()) · [`Path`](#Std.Cli.CommandResolution.Path()) · [`Result`](#Std.Cli.CommandResolution.Result()) · [`HelpText`](#Std.Cli.CommandResolution.HelpText()) · [`ErrorMessage`](#Std.Cli.CommandResolution.ErrorMessage()) · [`Prepend`](#Std.Cli.CommandResolution.Prepend(string))

<a id="Std.Cli.CommandResolution.ctor(int,string[],ParseResult,string,string)"></a>
### CommandResolution(int kind, string[] path, ParseResult result, string helpText, string errorMessage)

[`src/libraries/z42.cli/src/CommandResolution.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L13)

创建一个 CommandResolution（kind 0=match / 1=help / 2=unknown）。

<a id="Std.Cli.CommandResolution.Match(ParseResult)"></a>
### static CommandResolution Match(ParseResult result)

[`src/libraries/z42.cli/src/CommandResolution.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L23)

命中叶子命令且未请求 help。

<a id="Std.Cli.CommandResolution.Help(string)"></a>
### static CommandResolution Help(string helpText)

[`src/libraries/z42.cli/src/CommandResolution.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L28)

某层请求了 help；`helpText` 是该层的帮助文本。

<a id="Std.Cli.CommandResolution.Unknown(string,string)"></a>
### static CommandResolution Unknown(string errorMessage, string helpText)

[`src/libraries/z42.cli/src/CommandResolution.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L33)

某 router 层遇到不认识的 token。

<a id="Std.Cli.CommandResolution.IsMatch()"></a>
### bool IsMatch()

[`src/libraries/z42.cli/src/CommandResolution.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L38)

是否命中叶子命令（未请求 help）。

<a id="Std.Cli.CommandResolution.IsHelp()"></a>
### bool IsHelp()

[`src/libraries/z42.cli/src/CommandResolution.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L41)

是否在任一层遇到 `-h` / `--help`（或某层无剩余 token，或叶子 ShowHelp() 为真）。

<a id="Std.Cli.CommandResolution.IsUnknown()"></a>
### bool IsUnknown()

[`src/libraries/z42.cli/src/CommandResolution.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L44)

是否在某 router 层遇到不认识的 token。

<a id="Std.Cli.CommandResolution.Path()"></a>
### string[] Path()

[`src/libraries/z42.cli/src/CommandResolution.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L47)

已走过的命中链，如 `["build", "package"]`。

<a id="Std.Cli.CommandResolution.Result()"></a>
### ParseResult Result()

[`src/libraries/z42.cli/src/CommandResolution.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L50)

叶子的 ParseResult；仅在 IsMatch() 时有效。

<a id="Std.Cli.CommandResolution.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/CommandResolution.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L53)

相应层的帮助文本；仅在 IsHelp() / IsUnknown() 时有效。

<a id="Std.Cli.CommandResolution.ErrorMessage()"></a>
### string ErrorMessage()

[`src/libraries/z42.cli/src/CommandResolution.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L56)

错误消息；仅在 IsUnknown() 时有效。

<a id="Std.Cli.CommandResolution.Prepend(string)"></a>
### CommandResolution Prepend(string name)

[`src/libraries/z42.cli/src/CommandResolution.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L59)

递归回卷时在 path 头部前插祖先子命令名；原地修改并返回 this 以便链式调用（Resolve 内部使用）。

## _RepeatedList

[`src/libraries/z42.cli/src/ParseResult.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L4)

可重复 option 存储的内部包装：z42 没有原生 `string[][]`，每个 option 用它存追加值。

```z42
public class _RepeatedList {
    string[] _values
    int _count
    _RepeatedList()
    void Append(string v)
    string[] Snapshot()
}
```

成员：[`_values`](#Std.Cli._RepeatedList._values) · [`_count`](#Std.Cli._RepeatedList._count) · [`_RepeatedList`](#Std.Cli._RepeatedList.ctor()) · [`Append`](#Std.Cli._RepeatedList.Append(string)) · [`Snapshot`](#Std.Cli._RepeatedList.Snapshot())

<a id="Std.Cli._RepeatedList._values"></a>
### string[] _values

[`src/libraries/z42.cli/src/ParseResult.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L6)

已追加的值；容量不够时翻倍。

<a id="Std.Cli._RepeatedList._count"></a>
### int _count

[`src/libraries/z42.cli/src/ParseResult.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L8)

已追加的数量。

<a id="Std.Cli._RepeatedList.ctor()"></a>
### _RepeatedList()

[`src/libraries/z42.cli/src/ParseResult.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L11)

起始容量 4 的空列表。

<a id="Std.Cli._RepeatedList.Append(string)"></a>
### void Append(string v)

[`src/libraries/z42.cli/src/ParseResult.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L17)

追加一个值（容量不够翻倍）。

<a id="Std.Cli._RepeatedList.Snapshot()"></a>
### string[] Snapshot()

[`src/libraries/z42.cli/src/ParseResult.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L30)

按当前长度拷出一份。

## _MutexGroup

[`src/libraries/z42.cli/src/ParseResult.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L39)

互斥 option 组声明的内部包装：这些 long-name 至多能在命令行出现一个。

```z42
public class _MutexGroup {
    string[] _names
    int _count
    _MutexGroup(string[] names)
}
```

成员：[`_names`](#Std.Cli._MutexGroup._names) · [`_count`](#Std.Cli._MutexGroup._count) · [`_MutexGroup`](#Std.Cli._MutexGroup.ctor(string[]))

<a id="Std.Cli._MutexGroup._names"></a>
### string[] _names

[`src/libraries/z42.cli/src/ParseResult.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L41)

组内的 long-name 列表。

<a id="Std.Cli._MutexGroup._count"></a>
### int _count

[`src/libraries/z42.cli/src/ParseResult.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L43)

组内名字数。

<a id="Std.Cli._MutexGroup.ctor(string[])"></a>
### _MutexGroup(string[] names)

[`src/libraries/z42.cli/src/ParseResult.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L46)

按声明的名字建组。

## ParseResult

[`src/libraries/z42.cli/src/ParseResult.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L53)

Parse 的返回值。取值方法一律按 long name 查找；名字未注册抛 CliException。

```z42
public class ParseResult {
    string[] _flagLongs
    bool[] _flagValues
    int _flagCount
    string[] _optionLongs
    string[] _optionValues
    bool[] _optionWasSet
    bool[] _optionRepeated
    _RepeatedList[] _optionLists
    int _optionCount
    string[] _extras
    int _extrasCount
    bool _allowExtras
    string[] _positionalNames
    string[] _positionalValues
    int _positionalCount
    bool _showHelp
    ParseResult()
    bool GetFlag(string longName)
    string GetOption(string longName)
    bool WasOptionSet(string longName)
    string[] GetRepeatedOption(string longName)
    string[] Extras()
    int ExtrasCount()
    int GetIntOption(string longName)
    long GetLongOption(string longName)
    double GetDoubleOption(string longName)
    bool GetBoolOption(string longName)
    string GetPositional(int index)
    int PositionalCount()
    bool ShowHelp()
}
```

成员：[`_flagLongs`](#Std.Cli.ParseResult._flagLongs) · [`_flagValues`](#Std.Cli.ParseResult._flagValues) · [`_flagCount`](#Std.Cli.ParseResult._flagCount) · [`_optionLongs`](#Std.Cli.ParseResult._optionLongs) · [`_optionValues`](#Std.Cli.ParseResult._optionValues) · [`_optionWasSet`](#Std.Cli.ParseResult._optionWasSet) · [`_optionRepeated`](#Std.Cli.ParseResult._optionRepeated) · [`_optionLists`](#Std.Cli.ParseResult._optionLists) · [`_optionCount`](#Std.Cli.ParseResult._optionCount) · [`_extras`](#Std.Cli.ParseResult._extras) · [`_extrasCount`](#Std.Cli.ParseResult._extrasCount) · [`_allowExtras`](#Std.Cli.ParseResult._allowExtras) · [`_positionalNames`](#Std.Cli.ParseResult._positionalNames) · [`_positionalValues`](#Std.Cli.ParseResult._positionalValues) · [`_positionalCount`](#Std.Cli.ParseResult._positionalCount) · [`_showHelp`](#Std.Cli.ParseResult._showHelp) · [`ParseResult`](#Std.Cli.ParseResult.ctor()) · [`GetFlag`](#Std.Cli.ParseResult.GetFlag(string)) · [`GetOption`](#Std.Cli.ParseResult.GetOption(string)) · [`WasOptionSet`](#Std.Cli.ParseResult.WasOptionSet(string)) · [`GetRepeatedOption`](#Std.Cli.ParseResult.GetRepeatedOption(string)) · [`Extras`](#Std.Cli.ParseResult.Extras()) · [`ExtrasCount`](#Std.Cli.ParseResult.ExtrasCount()) · [`GetIntOption`](#Std.Cli.ParseResult.GetIntOption(string)) · [`GetLongOption`](#Std.Cli.ParseResult.GetLongOption(string)) · [`GetDoubleOption`](#Std.Cli.ParseResult.GetDoubleOption(string)) · [`GetBoolOption`](#Std.Cli.ParseResult.GetBoolOption(string)) · [`GetPositional`](#Std.Cli.ParseResult.GetPositional(int)) · [`PositionalCount`](#Std.Cli.ParseResult.PositionalCount()) · [`ShowHelp`](#Std.Cli.ParseResult.ShowHelp())

<a id="Std.Cli.ParseResult._flagLongs"></a>
### string[] _flagLongs

[`src/libraries/z42.cli/src/ParseResult.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L55)

flag 的 long-name（与 ArgParser 的注册表平行）。

<a id="Std.Cli.ParseResult._flagValues"></a>
### bool[] _flagValues

[`src/libraries/z42.cli/src/ParseResult.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L57)

各 flag 是否出现。

<a id="Std.Cli.ParseResult._flagCount"></a>
### int _flagCount

[`src/libraries/z42.cli/src/ParseResult.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L59)

flag 数量。

<a id="Std.Cli.ParseResult._optionLongs"></a>
### string[] _optionLongs

[`src/libraries/z42.cli/src/ParseResult.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L62)

option 的 long-name。

<a id="Std.Cli.ParseResult._optionValues"></a>
### string[] _optionValues

[`src/libraries/z42.cli/src/ParseResult.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L64)

各 option 的最终值（argv → env → default 三级解析后）。

<a id="Std.Cli.ParseResult._optionWasSet"></a>
### bool[] _optionWasSet

[`src/libraries/z42.cli/src/ParseResult.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L67)

值是否真的写自 argv（区别于默认值兜底）；用于必填校验，也让调用方能区分 「用户显式传了默认值」和「默认值隐式生效」。

<a id="Std.Cli.ParseResult._optionRepeated"></a>
### bool[] _optionRepeated

[`src/libraries/z42.cli/src/ParseResult.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L69)

声明侧 repeated 标志的镜像；非 repeated 时条目为 null。

<a id="Std.Cli.ParseResult._optionLists"></a>
### _RepeatedList[] _optionLists

[`src/libraries/z42.cli/src/ParseResult.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L71)

repeated option 追加的值列表；非 repeated 时条目为 null。

<a id="Std.Cli.ParseResult._optionCount"></a>
### int _optionCount

[`src/libraries/z42.cli/src/ParseResult.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L73)

option 数量。

<a id="Std.Cli.ParseResult._extras"></a>
### string[] _extras

[`src/libraries/z42.cli/src/ParseResult.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L76)

未知 flag / option 的透传列表；仅当调用过 ArgParser.AllowExtras 时才填充。

<a id="Std.Cli.ParseResult._extrasCount"></a>
### int _extrasCount

[`src/libraries/z42.cli/src/ParseResult.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L78)

透传条目数。

<a id="Std.Cli.ParseResult._allowExtras"></a>
### bool _allowExtras

[`src/libraries/z42.cli/src/ParseResult.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L80)

是否允许过透传。

<a id="Std.Cli.ParseResult._positionalNames"></a>
### string[] _positionalNames

[`src/libraries/z42.cli/src/ParseResult.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L83)

位置参数的名字（注册序）。

<a id="Std.Cli.ParseResult._positionalValues"></a>
### string[] _positionalValues

[`src/libraries/z42.cli/src/ParseResult.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L85)

各位置参数的值。

<a id="Std.Cli.ParseResult._positionalCount"></a>
### int _positionalCount

[`src/libraries/z42.cli/src/ParseResult.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L87)

位置参数数量。

<a id="Std.Cli.ParseResult._showHelp"></a>
### bool _showHelp

[`src/libraries/z42.cli/src/ParseResult.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L90)

是否命中了 `-h` / `--help`。

<a id="Std.Cli.ParseResult.ctor()"></a>
### ParseResult()

[`src/libraries/z42.cli/src/ParseResult.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L93)

全空的初始结果；由 ArgParser.Parse 填充。

<a id="Std.Cli.ParseResult.GetFlag(string)"></a>
### bool GetFlag(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L113)

返回指定 long-name flag 是否出现过。

<a id="Std.Cli.ParseResult.GetOption(string)"></a>
### string GetOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L123)

返回 option 的最终值（argv → env → default 三级解析后的结果）。

<a id="Std.Cli.ParseResult.WasOptionSet(string)"></a>
### bool WasOptionSet(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L134)

该 option 是否真的写在 argv 里（区分「用户显式传了默认值」和「默认值兜底」）。 env 后备命中时仍是 false。

<a id="Std.Cli.ParseResult.GetRepeatedOption(string)"></a>
### string[] GetRepeatedOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:146`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L146)

按 argv 顺序返回重复 option 的全部值。

未传过返回空数组；对非 `AddRepeatedOption` 声明的 option 抛 `CliException`。

<a id="Std.Cli.ParseResult.Extras()"></a>
### string[] Extras()

[`src/libraries/z42.cli/src/ParseResult.z42:161`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L161)

返回 AllowExtras 收集到的未知 argv token；未开启时返回空数组。

<a id="Std.Cli.ParseResult.ExtrasCount()"></a>
### int ExtrasCount()

[`src/libraries/z42.cli/src/ParseResult.z42:172`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L172)

AllowExtras 收集到的未知 token 个数；未开启时为 0。

<a id="Std.Cli.ParseResult.GetIntOption(string)"></a>
### int GetIntOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:180`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L180)

取值后走 int.Parse；失败抛 CliException。

<a id="Std.Cli.ParseResult.GetLongOption(string)"></a>
### long GetLongOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:189`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L189)

取值后走 long.Parse；失败抛 CliException。

<a id="Std.Cli.ParseResult.GetDoubleOption(string)"></a>
### double GetDoubleOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:198`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L198)

取值后走 double.Parse；失败抛 CliException。

<a id="Std.Cli.ParseResult.GetBoolOption(string)"></a>
### bool GetBoolOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:210`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L210)

只认 `true` / `false`（ASCII 大小写不敏感）。

其它值抛 CliException；用于 `--enabled=true` 这种显式布尔 option，
与 `GetFlag` 的「出现即真」不同。

<a id="Std.Cli.ParseResult.GetPositional(int)"></a>
### string GetPositional(int index)

[`src/libraries/z42.cli/src/ParseResult.z42:232`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L232)

返回第 index 个 positional（0-based）。

边界为**声明总数**（非已提供数）：已声明但未提供的可选 positional 返回 ""；
index 超出声明总数抛 CliException。

<a id="Std.Cli.ParseResult.PositionalCount()"></a>
### int PositionalCount()

[`src/libraries/z42.cli/src/ParseResult.z42:241`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L241)

argv 里实际提供的 positional 个数。

<a id="Std.Cli.ParseResult.ShowHelp()"></a>
### bool ShowHelp()

[`src/libraries/z42.cli/src/ParseResult.z42:244`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L244)

argv 里出现过 `-h` / `--help`。

## SubcommandRouter

[`src/libraries/z42.cli/src/SubcommandRouter.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L5)

子命令路由：每个子命令要么是叶子 ArgParser，要么是嵌套 SubcommandRouter。 同名重复注册会覆盖先前那条（叶子与嵌套可互相覆盖）。

```z42
public sealed class SubcommandRouter {
    SubcommandRouter(string programName, string description)
    void Add(string name, string description, ArgParser parser)
    void AddRouter(string name, string description, SubcommandRouter child)
    SubcommandMatch? Match(string[] argv)
    CommandResolution Resolve(string[] argv)
    string HelpText()
    int Count()
    bool Has(string name)
}
```

成员：[`SubcommandRouter`](#Std.Cli.SubcommandRouter.ctor(string,string)) · [`Add`](#Std.Cli.SubcommandRouter.Add(string,string,ArgParser)) · [`AddRouter`](#Std.Cli.SubcommandRouter.AddRouter(string,string,SubcommandRouter)) · [`Match`](#Std.Cli.SubcommandRouter.Match(string[])) · [`Resolve`](#Std.Cli.SubcommandRouter.Resolve(string[])) · [`HelpText`](#Std.Cli.SubcommandRouter.HelpText()) · [`Count`](#Std.Cli.SubcommandRouter.Count()) · [`Has`](#Std.Cli.SubcommandRouter.Has(string))

<a id="Std.Cli.SubcommandRouter.ctor(string,string)"></a>
### SubcommandRouter(string programName, string description)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L19)

用 programName 与 description 创建一个子命令路由。

<a id="Std.Cli.SubcommandRouter.Add(string,string,ArgParser)"></a>
### void Add(string name, string description, ArgParser parser)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L32)

注册叶子子命令（其参数由 `parser` 解析）；同名重复注册会覆盖。

<a id="Std.Cli.SubcommandRouter.AddRouter(string,string,SubcommandRouter)"></a>
### void AddRouter(string name, string description, SubcommandRouter child)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L57)

注册嵌套子命令路由（`prog build package …`）。

`child` 处理该名字之后的 token；同名重复注册会覆盖（叶子 → 路由或路由 → 路由）。

<a id="Std.Cli.SubcommandRouter.Match(string[])"></a>
### SubcommandMatch? Match(string[] argv)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L82)

单层 sugar：只看 argv[0]，命中叶子返回 `SubcommandMatch`。

其余情形（argv 为空 / argv[0] 是 `--help` / `-h` / 名字不认识）一律返回
null。不处理嵌套子命令。

<a id="Std.Cli.SubcommandRouter.Resolve(string[])"></a>
### CommandResolution Resolve(string[] argv)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L106)

推荐入口：递归下钻整棵命令树，返回三态互斥的 `CommandResolution`。

IsMatch / IsHelp / IsUnknown 恰好一个为真。叶子 `ArgParser.Parse` 的 option
级错误（未知 flag / 缺值 / 缺 positional）以 `CliException` 向上透传。

<a id="Std.Cli.SubcommandRouter.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:140`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L140)

渲染顶层帮助：programName - description + 一段 SUBCOMMANDS: 列表。

<a id="Std.Cli.SubcommandRouter.Count()"></a>
### int Count()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L155)

已注册的子命令个数。

<a id="Std.Cli.SubcommandRouter.Has(string)"></a>
### bool Has(string name)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:158`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L158)

指定名字是否已注册为子命令。

## SubcommandMatch

[`src/libraries/z42.cli/src/SubcommandRouter.z42:204`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L204)

单层子命令命中的结果：名字 + 叶子 ParseResult。

```z42
public sealed class SubcommandMatch {
    SubcommandMatch(string name, ParseResult result)
    string Name()
    ParseResult Result()
}
```

成员：[`SubcommandMatch`](#Std.Cli.SubcommandMatch.ctor(string,ParseResult)) · [`Name`](#Std.Cli.SubcommandMatch.Name()) · [`Result`](#Std.Cli.SubcommandMatch.Result())

<a id="Std.Cli.SubcommandMatch.ctor(string,ParseResult)"></a>
### SubcommandMatch(string name, ParseResult result)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:209`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L209)

用命中的子命令名与解析结果创建匹配对象。

<a id="Std.Cli.SubcommandMatch.Name()"></a>
### string Name()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:215`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L215)

命中的子命令名。

<a id="Std.Cli.SubcommandMatch.Result()"></a>
### ParseResult Result()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:218`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L218)

命中的叶子 ParseResult。

