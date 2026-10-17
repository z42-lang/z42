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

### ArgParser(string programName, string description)

用 programName 与 description 创建一个参数解析器。

### void AddFlag(string longName, string shortName, string help)

布尔开关：出现即 true。shortName 传 "" 表示无短名；`-h` / `--help` 自动注册，无需手加。

### void AddOption(string longName, string shortName, string help, string defaultValue)

注册带值 option（`--name value` 或 `--name=value`）；未出现时取 defaultValue。

### void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)

注册带环境变量后备的 option。取值优先级 argv > 环境变量 > defaultValue； 环境变量不存在或为空串时落到 defaultValue；help 自动追加 " (env: $NAME)"。

### void AddMutuallyExclusive(string[] names)

声明一组 option long-name，argv 中至多出现一个；名字必须已注册，否则当场抛 CliException。

### void AddRepeatedOption(string longName, string shortName, string help)

注册可重复 option：每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取最后一个。 默认值固定 ""，help 自动追加 " (repeatable)"。

### void AllowExtras()

一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常。

### void AddRequiredOption(string longName, string shortName, string help)

注册必填 option：argv 里没写时 Parse 抛 CliException。默认值固定 ""， help 自动追加 " (required)"。

### void AddPositional(string name, string help)

注册必填 positional，按声明顺序匹配 argv 中非 flag/option 的部分。

### void AddOptionalPositional(string name, string help)

注册可选 positional：缺省时 `GetPositional(i)` 返回 ""。必须在所有必填 positional 之后声明，否则 `AddPositional` 抛 CliException。help 中渲染为 `[name]`。

### ParseResult Parse(string[] argv)

解析 argv（不含 program name），返回 ParseResult；失败抛 CliException。

### string HelpText()

生成 USAGE / FLAGS / OPTIONS / ARGS 四段帮助文本。

## CliException

CLI 声明期、解析期、取值期错误的统一异常；ToString() 输出 "CliException: <message>"。

```z42
public class CliException : Exception {
    CliException(string message)
}
```

### CliException(string message)

用 message 创建异常。

## CommandResolution

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

### CommandResolution(int kind, string[] path, ParseResult result, string helpText, string errorMessage)

创建一个 CommandResolution（kind 0=match / 1=help / 2=unknown）。

### static CommandResolution Match(ParseResult result)

命中叶子命令且未请求 help。

### static CommandResolution Help(string helpText)

某层请求了 help；`helpText` 是该层的帮助文本。

### static CommandResolution Unknown(string errorMessage, string helpText)

某 router 层遇到不认识的 token。

### bool IsMatch()

是否命中叶子命令（未请求 help）。

### bool IsHelp()

是否在任一层遇到 `-h` / `--help`（或某层无剩余 token，或叶子 ShowHelp() 为真）。

### bool IsUnknown()

是否在某 router 层遇到不认识的 token。

### string[] Path()

已走过的命中链，如 `["build", "package"]`。

### ParseResult Result()

叶子的 ParseResult；仅在 IsMatch() 时有效。

### string HelpText()

相应层的帮助文本；仅在 IsHelp() / IsUnknown() 时有效。

### string ErrorMessage()

错误消息；仅在 IsUnknown() 时有效。

### CommandResolution Prepend(string name)

递归回卷时在 path 头部前插祖先子命令名；原地修改并返回 this 以便链式调用（Resolve 内部使用）。

## _RepeatedList


```z42
public class _RepeatedList {
    string[] _values
    int _count
    _RepeatedList()
    void Append(string v)
    string[] Snapshot()
}
```

### string[] _values


### int _count


### _RepeatedList()


### void Append(string v)


### string[] Snapshot()


## _MutexGroup


```z42
public class _MutexGroup {
    string[] _names
    int _count
    _MutexGroup(string[] names)
}
```

### string[] _names


### int _count


### _MutexGroup(string[] names)


## ParseResult

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

### string[] _flagLongs


### bool[] _flagValues


### int _flagCount


### string[] _optionLongs


### string[] _optionValues


### bool[] _optionWasSet


### bool[] _optionRepeated


### _RepeatedList[] _optionLists


### int _optionCount


### string[] _extras


### int _extrasCount


### bool _allowExtras


### string[] _positionalNames


### string[] _positionalValues


### int _positionalCount


### bool _showHelp


### ParseResult()


### bool GetFlag(string longName)

返回指定 long-name flag 是否出现过。

### string GetOption(string longName)

返回 option 的最终值（argv → env → default 三级解析后的结果）。

### bool WasOptionSet(string longName)

该 option 是否真的写在 argv 里（区分「用户显式传了默认值」和「默认值兜底」）。 env 后备命中时仍是 false。

### string[] GetRepeatedOption(string longName)

按 argv 顺序返回重复 option 的全部值；未传过返回空数组。 对非 `AddRepeatedOption` 声明的 option 抛 `CliException`。

### string[] Extras()

返回 AllowExtras 收集到的未知 argv token；未开启时返回空数组。

### int ExtrasCount()

AllowExtras 收集到的未知 token 个数；未开启时为 0。

### int GetIntOption(string longName)

取值后走 int.Parse；失败抛 CliException。

### long GetLongOption(string longName)

取值后走 long.Parse；失败抛 CliException。

### double GetDoubleOption(string longName)

取值后走 double.Parse；失败抛 CliException。

### bool GetBoolOption(string longName)

只认 `true` / `false`（ASCII 大小写不敏感），其它值抛 CliException。 用于 `--enabled=true` 这种显式布尔 option，与 `GetFlag` 的「出现即真」不同。

### string GetPositional(int index)

返回第 index 个 positional（0-based）。边界为**声明总数**（非已提供数）： 已声明但未提供的可选 positional 返回 ""；index 超出声明总数抛 CliException。

### int PositionalCount()

argv 里实际提供的 positional 个数。

### bool ShowHelp()

argv 里出现过 `-h` / `--help`。

## SubcommandRouter

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

### SubcommandRouter(string programName, string description)

用 programName 与 description 创建一个子命令路由。

### void Add(string name, string description, ArgParser parser)

注册叶子子命令（其参数由 `parser` 解析）；同名重复注册会覆盖。

### void AddRouter(string name, string description, SubcommandRouter child)

注册嵌套子命令路由（`prog build package …`）；`child` 处理该名字之后的 token。 同名重复注册会覆盖（叶子 → 路由或路由 → 路由）。

### SubcommandMatch? Match(string[] argv)

单层 sugar：只看 argv[0]。命中叶子返回 `SubcommandMatch`；其余情形（argv 为空 / argv[0] 是 `--help` / `-h` / 名字不认识）一律返回 null。不处理嵌套子命令。

### CommandResolution Resolve(string[] argv)

推荐入口：递归下钻整棵命令树，返回三态互斥的 `CommandResolution` （IsMatch / IsHelp / IsUnknown 恰好一个为真）。叶子 `ArgParser.Parse` 的 option 级错误（未知 flag / 缺值 / 缺 positional）以 `CliException` 向上透传。

### string HelpText()

渲染顶层帮助：programName - description + 一段 SUBCOMMANDS: 列表。

### int Count()

已注册的子命令个数。

### bool Has(string name)

指定名字是否已注册为子命令。

## SubcommandMatch

单层子命令命中的结果：名字 + 叶子 ParseResult。

```z42
public sealed class SubcommandMatch {
    SubcommandMatch(string name, ParseResult result)
    string Name()
    ParseResult Result()
}
```

### SubcommandMatch(string name, ParseResult result)

用命中的子命令名与解析结果创建匹配对象。

### string Name()

命中的子命令名。

### ParseResult Result()

命中的叶子 ParseResult。

