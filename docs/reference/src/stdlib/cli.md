<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.cli/ -->

# z42.cli —— 命令行参数解析。

> 包路径 `src/libraries/z42.cli/`；命名空间 `Std.Cli`

把 `string[] argv` 解析成 flag / option / positional 三类参数，并自动生成
`-h/--help` 文本。`SubcommandRouter` 在其上叠一层 `git` / `cargo` 风格的
（可嵌套的）子命令路由，外加跨子命令 persistent option 与节点默认动作。

参数值一律先当 `string` 拿；需要 `int` / `long` / `double` / `bool` 时在
**取值处**转换（`ParseResult.GetIntOption` 等），声明期不区分类型。

argv 从 `Std.IO.Environment.GetCommandLineArgs()` 取，**不含 program name**，
直接传给 `Parse`。

## 支持的 argv 形态

- **`--` 结束选项**：其后一切 token（含 `-h` / `--foo` / 负数）都作 positional
  落进固定槽或变长槽。
- **紧贴短参 `-pvalue` / `-p=4`**：首字符是已注册 option 短名时，余下整段是值
  （`=` 剥一层）。
- **负数字面量作 positional**：`-1` / `-2.5` 在「不是已注册 option 且还有
  positional 容量（含变长槽）」时按 positional 处理；无容量仍报 unknown option。
- **变长尾参 `args...`**：`AddVariadicPositional` 收集全部剩余 positional。
- **拼写建议**：未知 long option / 未知子命令在编辑距离足够近时附
  `(did you mean '...')`。
- **重名检测**：long / short / positional 重复注册在**声明期**抛 `CliException`
  （`--help` / `-h` 保留）。

## 不支持

- **声明期类型**：没有 `AddIntOption` 之类；一律 `AddOption`，在 `GetIntOption`
  等取值方法里转换。
- **簇内嵌 option 短名**：`-vp`（v 是 flag、p 是 option）报错——簇只收 flag；
  紧贴值形式只认**首字符**是 option 短名（`-p4`），不支持「前面 flag 簇 + 尾随
  option」的混合簇（`-vp 4`）。
- **科学计数负数**：`-1e5` 不当数字回退（`e` 更像短名），走 unknown option。
- **persistent 名与叶子重名**：router 层 `AddPersistent*` 声明的名字先于路由被剥走，
  叶子里同名 option 从 argv 拿不到值——别重名。
- **`Match` 不认嵌套**：`AddRouter` 注册的名字用 `Match` 去命中返回 null
  （不进嵌套树）；嵌套树、persistent 剥离、默认动作都只在 `Resolve` 里。
- **`AllowExtras` 与 short flag 簇 / 负数回退互斥**：开启 extras 后不再走簇解析，
  `-abc` 与 `-1` 整个 token 进 `Extras()`。

ArgParser 类按关注点分为四个 partial 碎片：本文件（声明 API 与注册表）、
`ArgParser.Parse.z42`（解析主循环）、`ArgParser.Help.z42`（帮助文本）、
`ArgParser.Persistent.z42`（persistent 剥离，供 SubcommandRouter）。

## ArgParser

[`src/libraries/z42.cli/src/ArgParser.Help.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L5)

`ArgParser` 的 partial 碎片 —— 帮助文本渲染（USAGE / FLAGS / OPTIONS / ARGS）。

```z42
public partial class ArgParser {
    string HelpText()
    string OptionsHelp()
    ParseResult Parse(string[] argv)
    _StripOutcome StripDeclared(string[] argv)
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
    void AddVariadicPositional(string name, string help)
}
```

成员：[`HelpText`](#Std.Cli.ArgParser.HelpText()) · [`OptionsHelp`](#Std.Cli.ArgParser.OptionsHelp()) · [`Parse`](#Std.Cli.ArgParser.Parse(string[])) · [`StripDeclared`](#Std.Cli.ArgParser.StripDeclared(string[])) · [`ArgParser`](#Std.Cli.ArgParser.ctor(string,string)) · [`AddFlag`](#Std.Cli.ArgParser.AddFlag(string,string,string)) · [`AddOption`](#Std.Cli.ArgParser.AddOption(string,string,string,string)) · [`AddOptionWithEnv`](#Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)) · [`AddMutuallyExclusive`](#Std.Cli.ArgParser.AddMutuallyExclusive(string[])) · [`AddRepeatedOption`](#Std.Cli.ArgParser.AddRepeatedOption(string,string,string)) · [`AllowExtras`](#Std.Cli.ArgParser.AllowExtras()) · [`AddRequiredOption`](#Std.Cli.ArgParser.AddRequiredOption(string,string,string)) · [`AddPositional`](#Std.Cli.ArgParser.AddPositional(string,string)) · [`AddOptionalPositional`](#Std.Cli.ArgParser.AddOptionalPositional(string,string)) · [`AddVariadicPositional`](#Std.Cli.ArgParser.AddVariadicPositional(string,string))

<a id="Std.Cli.ArgParser.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L10)

生成 USAGE / FLAGS / OPTIONS / ARGS 四段帮助文本。

<a id="Std.Cli.ArgParser.OptionsHelp()"></a>
### string OptionsHelp()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L54)

只渲染 FLAGS / OPTIONS 两段（不含 USAGE / ARGS）；`HelpText` 复用， 也供 `SubcommandRouter` 的 GLOBAL OPTIONS 段直接展示。

<a id="Std.Cli.ArgParser.Parse(string[])"></a>
### ParseResult Parse(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Parse.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Parse.z42#L11)

解析 argv（不含 program name），返回 ParseResult；失败抛 CliException。

<a id="Std.Cli.ArgParser.StripDeclared(string[])"></a>
### _StripOutcome StripDeclared(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L17)

把本 parser 已注册的 flag / option 从 argv 的**任意位置**剥出并解析： 命中的 token（及其取值 token）进本 parser 的 Parse，其余原样保留在 `Kept`（保序）。只识别 `--long` / `--long=v` / `--long v` / `-s` / `-s v` / `-sv` 六种形态；不认识的 token、`-h/--help`、以及 `--` 之后的 token 全部留在 `Kept`（`--` 之后的剥离语义归叶子 parser 的 positional，不归全局旗标）。

取值 token 缺失时抛 CliException。叶子路由 / 簇解析 / 负数回退都不在本方法职责内。

<a id="Std.Cli.ArgParser.ctor(string,string)"></a>
### ArgParser(string programName, string description)

[`src/libraries/z42.cli/src/ArgParser.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L97)

用 programName 与 description 创建一个参数解析器。

<a id="Std.Cli.ArgParser.AddFlag(string,string,string)"></a>
### void AddFlag(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L127)

布尔开关：出现即 true。shortName 传 "" 表示无短名；`-h` / `--help` 自动注册，无需手加。

<a id="Std.Cli.ArgParser.AddOption(string,string,string,string)"></a>
### void AddOption(string longName, string shortName, string help, string defaultValue)

[`src/libraries/z42.cli/src/ArgParser.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L138)

注册带值 option（`--name value` 或 `--name=value`）；未出现时取 defaultValue。

<a id="Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)"></a>
### void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)

[`src/libraries/z42.cli/src/ArgParser.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L155)

注册带环境变量后备的 option。取值优先级 argv > 环境变量 > defaultValue。

环境变量不存在或为空串时落到 defaultValue；help 自动追加 " (env: $NAME)"。

<a id="Std.Cli.ArgParser.AddMutuallyExclusive(string[])"></a>
### void AddMutuallyExclusive(string[] names)

[`src/libraries/z42.cli/src/ArgParser.z42:171`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L171)

声明一组 option long-name，argv 中至多出现一个；名字必须已注册，否则当场抛 CliException。

<a id="Std.Cli.ArgParser.AddRepeatedOption(string,string,string)"></a>
### void AddRepeatedOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:206`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L206)

注册可重复 option：每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取最后一个。

默认值固定 ""，help 自动追加 " (repeatable)"。

<a id="Std.Cli.ArgParser.AllowExtras()"></a>
### void AllowExtras()

[`src/libraries/z42.cli/src/ArgParser.z42:221`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L221)

一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常。

<a id="Std.Cli.ArgParser.AddRequiredOption(string,string,string)"></a>
### void AddRequiredOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:227`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L227)

注册必填 option：argv 里没写时 Parse 抛 CliException。默认值固定 ""， help 自动追加 " (required)"。

<a id="Std.Cli.ArgParser.AddPositional(string,string)"></a>
### void AddPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:242`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L242)

注册必填 positional，按声明顺序匹配 argv 中非 flag/option 的部分。

<a id="Std.Cli.ArgParser.AddOptionalPositional(string,string)"></a>
### void AddOptionalPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:264`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L264)

注册可选 positional：缺省时 `GetPositional(i)` 返回 ""。

必须在所有必填 positional 之后声明，否则 `AddPositional` 抛 CliException。
help 中渲染为 `[name]`。

<a id="Std.Cli.ArgParser.AddVariadicPositional(string,string)"></a>
### void AddVariadicPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:280`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L280)

注册变长尾参（必须是最后一个 positional）：收集固定槽填满后的**全部剩余** positional（含 `--` 之后的 token），用 `ParseResult.VariadicValues()` 取回。 变长槽不参与必填校验（0 个也合法）；help 的 USAGE 渲染为 `[name...]`。

## ArgParser

[`src/libraries/z42.cli/src/ArgParser.Parse.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Parse.z42#L6)

`ArgParser` 的 partial 碎片 —— 解析主循环与 token 分派 （`--` 分隔、长/短形态、紧贴短参、簇、负数回退、positional 落槽）。

```z42
public partial class ArgParser {
    string HelpText()
    string OptionsHelp()
    ParseResult Parse(string[] argv)
    _StripOutcome StripDeclared(string[] argv)
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
    void AddVariadicPositional(string name, string help)
}
```

成员：[`HelpText`](#Std.Cli.ArgParser.HelpText()) · [`OptionsHelp`](#Std.Cli.ArgParser.OptionsHelp()) · [`Parse`](#Std.Cli.ArgParser.Parse(string[])) · [`StripDeclared`](#Std.Cli.ArgParser.StripDeclared(string[])) · [`ArgParser`](#Std.Cli.ArgParser.ctor(string,string)) · [`AddFlag`](#Std.Cli.ArgParser.AddFlag(string,string,string)) · [`AddOption`](#Std.Cli.ArgParser.AddOption(string,string,string,string)) · [`AddOptionWithEnv`](#Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)) · [`AddMutuallyExclusive`](#Std.Cli.ArgParser.AddMutuallyExclusive(string[])) · [`AddRepeatedOption`](#Std.Cli.ArgParser.AddRepeatedOption(string,string,string)) · [`AllowExtras`](#Std.Cli.ArgParser.AllowExtras()) · [`AddRequiredOption`](#Std.Cli.ArgParser.AddRequiredOption(string,string,string)) · [`AddPositional`](#Std.Cli.ArgParser.AddPositional(string,string)) · [`AddOptionalPositional`](#Std.Cli.ArgParser.AddOptionalPositional(string,string)) · [`AddVariadicPositional`](#Std.Cli.ArgParser.AddVariadicPositional(string,string))

<a id="Std.Cli.ArgParser.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L10)

生成 USAGE / FLAGS / OPTIONS / ARGS 四段帮助文本。

<a id="Std.Cli.ArgParser.OptionsHelp()"></a>
### string OptionsHelp()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L54)

只渲染 FLAGS / OPTIONS 两段（不含 USAGE / ARGS）；`HelpText` 复用， 也供 `SubcommandRouter` 的 GLOBAL OPTIONS 段直接展示。

<a id="Std.Cli.ArgParser.Parse(string[])"></a>
### ParseResult Parse(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Parse.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Parse.z42#L11)

解析 argv（不含 program name），返回 ParseResult；失败抛 CliException。

<a id="Std.Cli.ArgParser.StripDeclared(string[])"></a>
### _StripOutcome StripDeclared(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L17)

把本 parser 已注册的 flag / option 从 argv 的**任意位置**剥出并解析： 命中的 token（及其取值 token）进本 parser 的 Parse，其余原样保留在 `Kept`（保序）。只识别 `--long` / `--long=v` / `--long v` / `-s` / `-s v` / `-sv` 六种形态；不认识的 token、`-h/--help`、以及 `--` 之后的 token 全部留在 `Kept`（`--` 之后的剥离语义归叶子 parser 的 positional，不归全局旗标）。

取值 token 缺失时抛 CliException。叶子路由 / 簇解析 / 负数回退都不在本方法职责内。

<a id="Std.Cli.ArgParser.ctor(string,string)"></a>
### ArgParser(string programName, string description)

[`src/libraries/z42.cli/src/ArgParser.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L97)

用 programName 与 description 创建一个参数解析器。

<a id="Std.Cli.ArgParser.AddFlag(string,string,string)"></a>
### void AddFlag(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L127)

布尔开关：出现即 true。shortName 传 "" 表示无短名；`-h` / `--help` 自动注册，无需手加。

<a id="Std.Cli.ArgParser.AddOption(string,string,string,string)"></a>
### void AddOption(string longName, string shortName, string help, string defaultValue)

[`src/libraries/z42.cli/src/ArgParser.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L138)

注册带值 option（`--name value` 或 `--name=value`）；未出现时取 defaultValue。

<a id="Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)"></a>
### void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)

[`src/libraries/z42.cli/src/ArgParser.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L155)

注册带环境变量后备的 option。取值优先级 argv > 环境变量 > defaultValue。

环境变量不存在或为空串时落到 defaultValue；help 自动追加 " (env: $NAME)"。

<a id="Std.Cli.ArgParser.AddMutuallyExclusive(string[])"></a>
### void AddMutuallyExclusive(string[] names)

[`src/libraries/z42.cli/src/ArgParser.z42:171`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L171)

声明一组 option long-name，argv 中至多出现一个；名字必须已注册，否则当场抛 CliException。

<a id="Std.Cli.ArgParser.AddRepeatedOption(string,string,string)"></a>
### void AddRepeatedOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:206`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L206)

注册可重复 option：每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取最后一个。

默认值固定 ""，help 自动追加 " (repeatable)"。

<a id="Std.Cli.ArgParser.AllowExtras()"></a>
### void AllowExtras()

[`src/libraries/z42.cli/src/ArgParser.z42:221`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L221)

一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常。

<a id="Std.Cli.ArgParser.AddRequiredOption(string,string,string)"></a>
### void AddRequiredOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:227`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L227)

注册必填 option：argv 里没写时 Parse 抛 CliException。默认值固定 ""， help 自动追加 " (required)"。

<a id="Std.Cli.ArgParser.AddPositional(string,string)"></a>
### void AddPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:242`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L242)

注册必填 positional，按声明顺序匹配 argv 中非 flag/option 的部分。

<a id="Std.Cli.ArgParser.AddOptionalPositional(string,string)"></a>
### void AddOptionalPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:264`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L264)

注册可选 positional：缺省时 `GetPositional(i)` 返回 ""。

必须在所有必填 positional 之后声明，否则 `AddPositional` 抛 CliException。
help 中渲染为 `[name]`。

<a id="Std.Cli.ArgParser.AddVariadicPositional(string,string)"></a>
### void AddVariadicPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:280`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L280)

注册变长尾参（必须是最后一个 positional）：收集固定槽填满后的**全部剩余** positional（含 `--` 之后的 token），用 `ParseResult.VariadicValues()` 取回。 变长槽不参与必填校验（0 个也合法）；help 的 USAGE 渲染为 `[name...]`。

## ArgParser

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L6)

`ArgParser` 的 partial 碎片 —— persistent 剥离：把本 parser 已注册的 flag / option 从 argv 任意位置剥出，供 `SubcommandRouter` 的跨子命令 全局旗标使用。

```z42
public partial class ArgParser {
    string HelpText()
    string OptionsHelp()
    ParseResult Parse(string[] argv)
    _StripOutcome StripDeclared(string[] argv)
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
    void AddVariadicPositional(string name, string help)
}
```

成员：[`HelpText`](#Std.Cli.ArgParser.HelpText()) · [`OptionsHelp`](#Std.Cli.ArgParser.OptionsHelp()) · [`Parse`](#Std.Cli.ArgParser.Parse(string[])) · [`StripDeclared`](#Std.Cli.ArgParser.StripDeclared(string[])) · [`ArgParser`](#Std.Cli.ArgParser.ctor(string,string)) · [`AddFlag`](#Std.Cli.ArgParser.AddFlag(string,string,string)) · [`AddOption`](#Std.Cli.ArgParser.AddOption(string,string,string,string)) · [`AddOptionWithEnv`](#Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)) · [`AddMutuallyExclusive`](#Std.Cli.ArgParser.AddMutuallyExclusive(string[])) · [`AddRepeatedOption`](#Std.Cli.ArgParser.AddRepeatedOption(string,string,string)) · [`AllowExtras`](#Std.Cli.ArgParser.AllowExtras()) · [`AddRequiredOption`](#Std.Cli.ArgParser.AddRequiredOption(string,string,string)) · [`AddPositional`](#Std.Cli.ArgParser.AddPositional(string,string)) · [`AddOptionalPositional`](#Std.Cli.ArgParser.AddOptionalPositional(string,string)) · [`AddVariadicPositional`](#Std.Cli.ArgParser.AddVariadicPositional(string,string))

<a id="Std.Cli.ArgParser.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L10)

生成 USAGE / FLAGS / OPTIONS / ARGS 四段帮助文本。

<a id="Std.Cli.ArgParser.OptionsHelp()"></a>
### string OptionsHelp()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L54)

只渲染 FLAGS / OPTIONS 两段（不含 USAGE / ARGS）；`HelpText` 复用， 也供 `SubcommandRouter` 的 GLOBAL OPTIONS 段直接展示。

<a id="Std.Cli.ArgParser.Parse(string[])"></a>
### ParseResult Parse(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Parse.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Parse.z42#L11)

解析 argv（不含 program name），返回 ParseResult；失败抛 CliException。

<a id="Std.Cli.ArgParser.StripDeclared(string[])"></a>
### _StripOutcome StripDeclared(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L17)

把本 parser 已注册的 flag / option 从 argv 的**任意位置**剥出并解析： 命中的 token（及其取值 token）进本 parser 的 Parse，其余原样保留在 `Kept`（保序）。只识别 `--long` / `--long=v` / `--long v` / `-s` / `-s v` / `-sv` 六种形态；不认识的 token、`-h/--help`、以及 `--` 之后的 token 全部留在 `Kept`（`--` 之后的剥离语义归叶子 parser 的 positional，不归全局旗标）。

取值 token 缺失时抛 CliException。叶子路由 / 簇解析 / 负数回退都不在本方法职责内。

<a id="Std.Cli.ArgParser.ctor(string,string)"></a>
### ArgParser(string programName, string description)

[`src/libraries/z42.cli/src/ArgParser.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L97)

用 programName 与 description 创建一个参数解析器。

<a id="Std.Cli.ArgParser.AddFlag(string,string,string)"></a>
### void AddFlag(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L127)

布尔开关：出现即 true。shortName 传 "" 表示无短名；`-h` / `--help` 自动注册，无需手加。

<a id="Std.Cli.ArgParser.AddOption(string,string,string,string)"></a>
### void AddOption(string longName, string shortName, string help, string defaultValue)

[`src/libraries/z42.cli/src/ArgParser.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L138)

注册带值 option（`--name value` 或 `--name=value`）；未出现时取 defaultValue。

<a id="Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)"></a>
### void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)

[`src/libraries/z42.cli/src/ArgParser.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L155)

注册带环境变量后备的 option。取值优先级 argv > 环境变量 > defaultValue。

环境变量不存在或为空串时落到 defaultValue；help 自动追加 " (env: $NAME)"。

<a id="Std.Cli.ArgParser.AddMutuallyExclusive(string[])"></a>
### void AddMutuallyExclusive(string[] names)

[`src/libraries/z42.cli/src/ArgParser.z42:171`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L171)

声明一组 option long-name，argv 中至多出现一个；名字必须已注册，否则当场抛 CliException。

<a id="Std.Cli.ArgParser.AddRepeatedOption(string,string,string)"></a>
### void AddRepeatedOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:206`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L206)

注册可重复 option：每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取最后一个。

默认值固定 ""，help 自动追加 " (repeatable)"。

<a id="Std.Cli.ArgParser.AllowExtras()"></a>
### void AllowExtras()

[`src/libraries/z42.cli/src/ArgParser.z42:221`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L221)

一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常。

<a id="Std.Cli.ArgParser.AddRequiredOption(string,string,string)"></a>
### void AddRequiredOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:227`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L227)

注册必填 option：argv 里没写时 Parse 抛 CliException。默认值固定 ""， help 自动追加 " (required)"。

<a id="Std.Cli.ArgParser.AddPositional(string,string)"></a>
### void AddPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:242`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L242)

注册必填 positional，按声明顺序匹配 argv 中非 flag/option 的部分。

<a id="Std.Cli.ArgParser.AddOptionalPositional(string,string)"></a>
### void AddOptionalPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:264`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L264)

注册可选 positional：缺省时 `GetPositional(i)` 返回 ""。

必须在所有必填 positional 之后声明，否则 `AddPositional` 抛 CliException。
help 中渲染为 `[name]`。

<a id="Std.Cli.ArgParser.AddVariadicPositional(string,string)"></a>
### void AddVariadicPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:280`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L280)

注册变长尾参（必须是最后一个 positional）：收集固定槽填满后的**全部剩余** positional（含 `--` 之后的 token），用 `ParseResult.VariadicValues()` 取回。 变长槽不参与必填校验（0 个也合法）；help 的 USAGE 渲染为 `[name...]`。

## _StripKind

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L109)

token 分类结果的内部载体（见 ArgParser._ClassifyLong / _ClassifyShort）。

```z42
public class _StripKind {
    int Kind
    int Next
    _StripKind(int kind, int next)
}
```

成员：[`Kind`](#Std.Cli._StripKind.Kind) · [`Next`](#Std.Cli._StripKind.Next) · [`_StripKind`](#Std.Cli._StripKind.ctor(int,int))

<a id="Std.Cli._StripKind.Kind"></a>
### int Kind

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:111`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L111)

0=与本 parser 无关；1=flag；2=option 分离值；3=option 内联值。

<a id="Std.Cli._StripKind.Next"></a>
### int Next

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L113)

命中时下一个未处理下标。

<a id="Std.Cli._StripKind.ctor(int,int)"></a>
### _StripKind(int kind, int next)

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L116)

用类别与下一个下标创建。

## _StripOutcome

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L123)

`ArgParser.StripDeclared` 的结果：剥离后剩余的 argv（保序）+ 剥出 token 的解析结果。

```z42
public class _StripOutcome {
    string[] Kept
    ParseResult Result
    _StripOutcome(string[] kept, ParseResult result)
}
```

成员：[`Kept`](#Std.Cli._StripOutcome.Kept) · [`Result`](#Std.Cli._StripOutcome.Result) · [`_StripOutcome`](#Std.Cli._StripOutcome.ctor(string[],ParseResult))

<a id="Std.Cli._StripOutcome.Kept"></a>
### string[] Kept

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L125)

未命中的 token（原序）。

<a id="Std.Cli._StripOutcome.Result"></a>
### ParseResult Result

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L127)

剥出 token 经本 parser 解析的结果。

<a id="Std.Cli._StripOutcome.ctor(string[],ParseResult)"></a>
### _StripOutcome(string[] kept, ParseResult result)

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:130`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L130)

用两部分结果创建。

## ArgParser

[`src/libraries/z42.cli/src/ArgParser.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L50)

声明参数并解析 argv。所有 Add* 方法按调用顺序记录；Parse 可对同一 parser 重复调用。

```z42
public partial class ArgParser {
    string HelpText()
    string OptionsHelp()
    ParseResult Parse(string[] argv)
    _StripOutcome StripDeclared(string[] argv)
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
    void AddVariadicPositional(string name, string help)
}
```

成员：[`HelpText`](#Std.Cli.ArgParser.HelpText()) · [`OptionsHelp`](#Std.Cli.ArgParser.OptionsHelp()) · [`Parse`](#Std.Cli.ArgParser.Parse(string[])) · [`StripDeclared`](#Std.Cli.ArgParser.StripDeclared(string[])) · [`ArgParser`](#Std.Cli.ArgParser.ctor(string,string)) · [`AddFlag`](#Std.Cli.ArgParser.AddFlag(string,string,string)) · [`AddOption`](#Std.Cli.ArgParser.AddOption(string,string,string,string)) · [`AddOptionWithEnv`](#Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)) · [`AddMutuallyExclusive`](#Std.Cli.ArgParser.AddMutuallyExclusive(string[])) · [`AddRepeatedOption`](#Std.Cli.ArgParser.AddRepeatedOption(string,string,string)) · [`AllowExtras`](#Std.Cli.ArgParser.AllowExtras()) · [`AddRequiredOption`](#Std.Cli.ArgParser.AddRequiredOption(string,string,string)) · [`AddPositional`](#Std.Cli.ArgParser.AddPositional(string,string)) · [`AddOptionalPositional`](#Std.Cli.ArgParser.AddOptionalPositional(string,string)) · [`AddVariadicPositional`](#Std.Cli.ArgParser.AddVariadicPositional(string,string))

<a id="Std.Cli.ArgParser.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L10)

生成 USAGE / FLAGS / OPTIONS / ARGS 四段帮助文本。

<a id="Std.Cli.ArgParser.OptionsHelp()"></a>
### string OptionsHelp()

[`src/libraries/z42.cli/src/ArgParser.Help.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Help.z42#L54)

只渲染 FLAGS / OPTIONS 两段（不含 USAGE / ARGS）；`HelpText` 复用， 也供 `SubcommandRouter` 的 GLOBAL OPTIONS 段直接展示。

<a id="Std.Cli.ArgParser.Parse(string[])"></a>
### ParseResult Parse(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Parse.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Parse.z42#L11)

解析 argv（不含 program name），返回 ParseResult；失败抛 CliException。

<a id="Std.Cli.ArgParser.StripDeclared(string[])"></a>
### _StripOutcome StripDeclared(string[] argv)

[`src/libraries/z42.cli/src/ArgParser.Persistent.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.Persistent.z42#L17)

把本 parser 已注册的 flag / option 从 argv 的**任意位置**剥出并解析： 命中的 token（及其取值 token）进本 parser 的 Parse，其余原样保留在 `Kept`（保序）。只识别 `--long` / `--long=v` / `--long v` / `-s` / `-s v` / `-sv` 六种形态；不认识的 token、`-h/--help`、以及 `--` 之后的 token 全部留在 `Kept`（`--` 之后的剥离语义归叶子 parser 的 positional，不归全局旗标）。

取值 token 缺失时抛 CliException。叶子路由 / 簇解析 / 负数回退都不在本方法职责内。

<a id="Std.Cli.ArgParser.ctor(string,string)"></a>
### ArgParser(string programName, string description)

[`src/libraries/z42.cli/src/ArgParser.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L97)

用 programName 与 description 创建一个参数解析器。

<a id="Std.Cli.ArgParser.AddFlag(string,string,string)"></a>
### void AddFlag(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L127)

布尔开关：出现即 true。shortName 传 "" 表示无短名；`-h` / `--help` 自动注册，无需手加。

<a id="Std.Cli.ArgParser.AddOption(string,string,string,string)"></a>
### void AddOption(string longName, string shortName, string help, string defaultValue)

[`src/libraries/z42.cli/src/ArgParser.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L138)

注册带值 option（`--name value` 或 `--name=value`）；未出现时取 defaultValue。

<a id="Std.Cli.ArgParser.AddOptionWithEnv(string,string,string,string,string)"></a>
### void AddOptionWithEnv(string longName, string shortName, string help, string defaultValue, string envVarName)

[`src/libraries/z42.cli/src/ArgParser.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L155)

注册带环境变量后备的 option。取值优先级 argv > 环境变量 > defaultValue。

环境变量不存在或为空串时落到 defaultValue；help 自动追加 " (env: $NAME)"。

<a id="Std.Cli.ArgParser.AddMutuallyExclusive(string[])"></a>
### void AddMutuallyExclusive(string[] names)

[`src/libraries/z42.cli/src/ArgParser.z42:171`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L171)

声明一组 option long-name，argv 中至多出现一个；名字必须已注册，否则当场抛 CliException。

<a id="Std.Cli.ArgParser.AddRepeatedOption(string,string,string)"></a>
### void AddRepeatedOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:206`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L206)

注册可重复 option：每次出现都追加；用 `GetRepeatedOption` 取全部，`GetOption` 取最后一个。

默认值固定 ""，help 自动追加 " (repeatable)"。

<a id="Std.Cli.ArgParser.AllowExtras()"></a>
### void AllowExtras()

[`src/libraries/z42.cli/src/ArgParser.z42:221`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L221)

一次性开关：开启后未知 flag / option 进 `ParseResult.Extras()` 而不抛异常。

<a id="Std.Cli.ArgParser.AddRequiredOption(string,string,string)"></a>
### void AddRequiredOption(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:227`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L227)

注册必填 option：argv 里没写时 Parse 抛 CliException。默认值固定 ""， help 自动追加 " (required)"。

<a id="Std.Cli.ArgParser.AddPositional(string,string)"></a>
### void AddPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:242`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L242)

注册必填 positional，按声明顺序匹配 argv 中非 flag/option 的部分。

<a id="Std.Cli.ArgParser.AddOptionalPositional(string,string)"></a>
### void AddOptionalPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:264`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L264)

注册可选 positional：缺省时 `GetPositional(i)` 返回 ""。

必须在所有必填 positional 之后声明，否则 `AddPositional` 抛 CliException。
help 中渲染为 `[name]`。

<a id="Std.Cli.ArgParser.AddVariadicPositional(string,string)"></a>
### void AddVariadicPositional(string name, string help)

[`src/libraries/z42.cli/src/ArgParser.z42:280`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ArgParser.z42#L280)

注册变长尾参（必须是最后一个 positional）：收集固定槽填满后的**全部剩余** positional（含 `--` 之后的 token），用 `ParseResult.VariadicValues()` 取回。 变长槽不参与必填校验（0 个也合法）；help 的 USAGE 渲染为 `[name...]`。

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
    CommandResolution AttachPersistent(ParseResult p)
    string PersistentOption(string longName)
    bool PersistentFlag(string longName)
}
```

成员：[`CommandResolution`](#Std.Cli.CommandResolution.ctor(int,string[],ParseResult,string,string)) · [`Match`](#Std.Cli.CommandResolution.Match(ParseResult)) · [`Help`](#Std.Cli.CommandResolution.Help(string)) · [`Unknown`](#Std.Cli.CommandResolution.Unknown(string,string)) · [`IsMatch`](#Std.Cli.CommandResolution.IsMatch()) · [`IsHelp`](#Std.Cli.CommandResolution.IsHelp()) · [`IsUnknown`](#Std.Cli.CommandResolution.IsUnknown()) · [`Path`](#Std.Cli.CommandResolution.Path()) · [`Result`](#Std.Cli.CommandResolution.Result()) · [`HelpText`](#Std.Cli.CommandResolution.HelpText()) · [`ErrorMessage`](#Std.Cli.CommandResolution.ErrorMessage()) · [`Prepend`](#Std.Cli.CommandResolution.Prepend(string)) · [`AttachPersistent`](#Std.Cli.CommandResolution.AttachPersistent(ParseResult)) · [`PersistentOption`](#Std.Cli.CommandResolution.PersistentOption(string)) · [`PersistentFlag`](#Std.Cli.CommandResolution.PersistentFlag(string))

<a id="Std.Cli.CommandResolution.ctor(int,string[],ParseResult,string,string)"></a>
### CommandResolution(int kind, string[] path, ParseResult result, string helpText, string errorMessage)

[`src/libraries/z42.cli/src/CommandResolution.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L15)

创建一个 CommandResolution（kind 0=match / 1=help / 2=unknown）。

<a id="Std.Cli.CommandResolution.Match(ParseResult)"></a>
### static CommandResolution Match(ParseResult result)

[`src/libraries/z42.cli/src/CommandResolution.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L26)

命中叶子命令且未请求 help。

<a id="Std.Cli.CommandResolution.Help(string)"></a>
### static CommandResolution Help(string helpText)

[`src/libraries/z42.cli/src/CommandResolution.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L31)

某层请求了 help；`helpText` 是该层的帮助文本。

<a id="Std.Cli.CommandResolution.Unknown(string,string)"></a>
### static CommandResolution Unknown(string errorMessage, string helpText)

[`src/libraries/z42.cli/src/CommandResolution.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L36)

某 router 层遇到不认识的 token。

<a id="Std.Cli.CommandResolution.IsMatch()"></a>
### bool IsMatch()

[`src/libraries/z42.cli/src/CommandResolution.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L41)

是否命中叶子命令（未请求 help）。

<a id="Std.Cli.CommandResolution.IsHelp()"></a>
### bool IsHelp()

[`src/libraries/z42.cli/src/CommandResolution.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L44)

是否在任一层遇到 `-h` / `--help`（或某层无剩余 token，或叶子 ShowHelp() 为真）。

<a id="Std.Cli.CommandResolution.IsUnknown()"></a>
### bool IsUnknown()

[`src/libraries/z42.cli/src/CommandResolution.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L47)

是否在某 router 层遇到不认识的 token。

<a id="Std.Cli.CommandResolution.Path()"></a>
### string[] Path()

[`src/libraries/z42.cli/src/CommandResolution.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L50)

已走过的命中链，如 `["build", "package"]`。

<a id="Std.Cli.CommandResolution.Result()"></a>
### ParseResult Result()

[`src/libraries/z42.cli/src/CommandResolution.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L53)

叶子的 ParseResult；仅在 IsMatch() 时有效。

<a id="Std.Cli.CommandResolution.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/CommandResolution.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L56)

相应层的帮助文本；仅在 IsHelp() / IsUnknown() 时有效。

<a id="Std.Cli.CommandResolution.ErrorMessage()"></a>
### string ErrorMessage()

[`src/libraries/z42.cli/src/CommandResolution.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L59)

错误消息；仅在 IsUnknown() 时有效。

<a id="Std.Cli.CommandResolution.Prepend(string)"></a>
### CommandResolution Prepend(string name)

[`src/libraries/z42.cli/src/CommandResolution.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L62)

递归回卷时在 path 头部前插祖先子命令名；原地修改并返回 this 以便链式调用（Resolve 内部使用）。

<a id="Std.Cli.CommandResolution.AttachPersistent(ParseResult)"></a>
### CommandResolution AttachPersistent(ParseResult p)

[`src/libraries/z42.cli/src/CommandResolution.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L76)

把祖先层剥出的 persistent 解析结果挂到结果**头部**（根层最终在最前； Resolve 递归回卷时调用；原地修改并返回 this）。

<a id="Std.Cli.CommandResolution.PersistentOption(string)"></a>
### string PersistentOption(string longName)

[`src/libraries/z42.cli/src/CommandResolution.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L104)

各层 persistent option 的最终值（根层优先）；名字未在任何层声明抛 CliException。

<a id="Std.Cli.CommandResolution.PersistentFlag(string)"></a>
### bool PersistentFlag(string longName)

[`src/libraries/z42.cli/src/CommandResolution.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/CommandResolution.z42#L114)

各层 persistent flag 是否出现（根层优先）；名字未在任何层声明抛 CliException。

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
    _RepeatedList _variadic
    bool _hasVariadic
    bool _showHelp
    ParseResult()
    bool GetFlag(string longName)
    bool HasFlag(string longName)
    bool HasOption(string longName)
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
    string[] VariadicValues()
    bool HasVariadic()
    bool ShowHelp()
}
```

成员：[`_flagLongs`](#Std.Cli.ParseResult._flagLongs) · [`_flagValues`](#Std.Cli.ParseResult._flagValues) · [`_flagCount`](#Std.Cli.ParseResult._flagCount) · [`_optionLongs`](#Std.Cli.ParseResult._optionLongs) · [`_optionValues`](#Std.Cli.ParseResult._optionValues) · [`_optionWasSet`](#Std.Cli.ParseResult._optionWasSet) · [`_optionRepeated`](#Std.Cli.ParseResult._optionRepeated) · [`_optionLists`](#Std.Cli.ParseResult._optionLists) · [`_optionCount`](#Std.Cli.ParseResult._optionCount) · [`_extras`](#Std.Cli.ParseResult._extras) · [`_extrasCount`](#Std.Cli.ParseResult._extrasCount) · [`_allowExtras`](#Std.Cli.ParseResult._allowExtras) · [`_positionalNames`](#Std.Cli.ParseResult._positionalNames) · [`_positionalValues`](#Std.Cli.ParseResult._positionalValues) · [`_positionalCount`](#Std.Cli.ParseResult._positionalCount) · [`_variadic`](#Std.Cli.ParseResult._variadic) · [`_hasVariadic`](#Std.Cli.ParseResult._hasVariadic) · [`_showHelp`](#Std.Cli.ParseResult._showHelp) · [`ParseResult`](#Std.Cli.ParseResult.ctor()) · [`GetFlag`](#Std.Cli.ParseResult.GetFlag(string)) · [`HasFlag`](#Std.Cli.ParseResult.HasFlag(string)) · [`HasOption`](#Std.Cli.ParseResult.HasOption(string)) · [`GetOption`](#Std.Cli.ParseResult.GetOption(string)) · [`WasOptionSet`](#Std.Cli.ParseResult.WasOptionSet(string)) · [`GetRepeatedOption`](#Std.Cli.ParseResult.GetRepeatedOption(string)) · [`Extras`](#Std.Cli.ParseResult.Extras()) · [`ExtrasCount`](#Std.Cli.ParseResult.ExtrasCount()) · [`GetIntOption`](#Std.Cli.ParseResult.GetIntOption(string)) · [`GetLongOption`](#Std.Cli.ParseResult.GetLongOption(string)) · [`GetDoubleOption`](#Std.Cli.ParseResult.GetDoubleOption(string)) · [`GetBoolOption`](#Std.Cli.ParseResult.GetBoolOption(string)) · [`GetPositional`](#Std.Cli.ParseResult.GetPositional(int)) · [`PositionalCount`](#Std.Cli.ParseResult.PositionalCount()) · [`VariadicValues`](#Std.Cli.ParseResult.VariadicValues()) · [`HasVariadic`](#Std.Cli.ParseResult.HasVariadic()) · [`ShowHelp`](#Std.Cli.ParseResult.ShowHelp())

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

<a id="Std.Cli.ParseResult._variadic"></a>
### _RepeatedList _variadic

[`src/libraries/z42.cli/src/ParseResult.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L90)

变长尾参（`AddVariadicPositional`）的追加存储；未声明时为 null。

<a id="Std.Cli.ParseResult._hasVariadic"></a>
### bool _hasVariadic

[`src/libraries/z42.cli/src/ParseResult.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L92)

是否声明了变长尾参。

<a id="Std.Cli.ParseResult._showHelp"></a>
### bool _showHelp

[`src/libraries/z42.cli/src/ParseResult.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L95)

是否命中了 `-h` / `--help`。

<a id="Std.Cli.ParseResult.ctor()"></a>
### ParseResult()

[`src/libraries/z42.cli/src/ParseResult.z42:98`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L98)

全空的初始结果；由 ArgParser.Parse 填充。

<a id="Std.Cli.ParseResult.GetFlag(string)"></a>
### bool GetFlag(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:120`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L120)

返回指定 long-name flag 是否出现过。

<a id="Std.Cli.ParseResult.HasFlag(string)"></a>
### bool HasFlag(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:130`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L130)

指定 long-name 的 flag 是否注册过（不抛异常的探测）。

<a id="Std.Cli.ParseResult.HasOption(string)"></a>
### bool HasOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:140`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L140)

指定 long-name 的 option 是否注册过（不抛异常的探测）。

<a id="Std.Cli.ParseResult.GetOption(string)"></a>
### string GetOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:150`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L150)

返回 option 的最终值（argv → env → default 三级解析后的结果）。

<a id="Std.Cli.ParseResult.WasOptionSet(string)"></a>
### bool WasOptionSet(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:161`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L161)

该 option 是否真的写在 argv 里（区分「用户显式传了默认值」和「默认值兜底」）。 env 后备命中时仍是 false。

<a id="Std.Cli.ParseResult.GetRepeatedOption(string)"></a>
### string[] GetRepeatedOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:173`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L173)

按 argv 顺序返回重复 option 的全部值。

未传过返回空数组；对非 `AddRepeatedOption` 声明的 option 抛 `CliException`。

<a id="Std.Cli.ParseResult.Extras()"></a>
### string[] Extras()

[`src/libraries/z42.cli/src/ParseResult.z42:188`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L188)

返回 AllowExtras 收集到的未知 argv token；未开启时返回空数组。

<a id="Std.Cli.ParseResult.ExtrasCount()"></a>
### int ExtrasCount()

[`src/libraries/z42.cli/src/ParseResult.z42:199`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L199)

AllowExtras 收集到的未知 token 个数；未开启时为 0。

<a id="Std.Cli.ParseResult.GetIntOption(string)"></a>
### int GetIntOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:207`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L207)

取值后走 int.Parse；失败抛 CliException。

<a id="Std.Cli.ParseResult.GetLongOption(string)"></a>
### long GetLongOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:216`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L216)

取值后走 long.Parse；失败抛 CliException。

<a id="Std.Cli.ParseResult.GetDoubleOption(string)"></a>
### double GetDoubleOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:225`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L225)

取值后走 double.Parse；失败抛 CliException。

<a id="Std.Cli.ParseResult.GetBoolOption(string)"></a>
### bool GetBoolOption(string longName)

[`src/libraries/z42.cli/src/ParseResult.z42:237`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L237)

只认 `true` / `false`（ASCII 大小写不敏感）。

其它值抛 CliException；用于 `--enabled=true` 这种显式布尔 option，
与 `GetFlag` 的「出现即真」不同。

<a id="Std.Cli.ParseResult.GetPositional(int)"></a>
### string GetPositional(int index)

[`src/libraries/z42.cli/src/ParseResult.z42:259`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L259)

返回第 index 个 positional（0-based）。

边界为**声明总数**（非已提供数）：已声明但未提供的可选 positional 返回 ""；
index 超出声明总数抛 CliException。

<a id="Std.Cli.ParseResult.PositionalCount()"></a>
### int PositionalCount()

[`src/libraries/z42.cli/src/ParseResult.z42:268`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L268)

argv 里实际提供的 positional 个数。

<a id="Std.Cli.ParseResult.VariadicValues()"></a>
### string[] VariadicValues()

[`src/libraries/z42.cli/src/ParseResult.z42:271`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L271)

返回变长尾参收集到的全部值（argv 序，含 `--` 之后的 token）；未声明或未出现返回空数组。

<a id="Std.Cli.ParseResult.HasVariadic()"></a>
### bool HasVariadic()

[`src/libraries/z42.cli/src/ParseResult.z42:277`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L277)

是否声明了变长尾参（`AddVariadicPositional`）。

<a id="Std.Cli.ParseResult.ShowHelp()"></a>
### bool ShowHelp()

[`src/libraries/z42.cli/src/ParseResult.z42:280`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/ParseResult.z42#L280)

argv 里出现过 `-h` / `--help`。

## SubcommandRouter

[`src/libraries/z42.cli/src/SubcommandRouter.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L10)

子命令路由：每个子命令要么是叶子 ArgParser，要么是嵌套 SubcommandRouter。

除名字外可挂**别名**（`ls` / `list` 同槽，路由后归一为正名）；同名或别名互撞在
注册期抛 CliException（不再静默覆盖）。`AddPersistent*` 声明的全局旗标会在路由前
从 argv 任意位置剥出（子命令前后皆可，多层各管各的子树）；`SetDefault` 给节点挂
默认动作（argv 为空或首 token 是 flag 且不匹配任何子命令时执行）。未知子命令在
编辑距离足够近时附 `(did you mean '...')` 建议。

```z42
public sealed class SubcommandRouter {
    SubcommandRouter(string programName, string description)
    void Add(string name, string description, ArgParser parser)
    void Add(string name, string description, ArgParser parser, string[] aliases)
    void AddRouter(string name, string description, SubcommandRouter child)
    void AddRouter(string name, string description, SubcommandRouter child, string[] aliases)
    void AddPersistentFlag(string longName, string shortName, string help)
    void AddPersistentOption(string longName, string shortName, string help, string defaultValue)
    void SetDefault(ArgParser parser, string actionDescription)
    SubcommandMatch? Match(string[] argv)
    CommandResolution Resolve(string[] argvIn)
    string HelpText()
    int Count()
    bool Has(string name)
}
```

成员：[`SubcommandRouter`](#Std.Cli.SubcommandRouter.ctor(string,string)) · [`Add`](#Std.Cli.SubcommandRouter.Add(string,string,ArgParser)) · [`Add`](#Std.Cli.SubcommandRouter.Add(string,string,ArgParser,string[])) · [`AddRouter`](#Std.Cli.SubcommandRouter.AddRouter(string,string,SubcommandRouter)) · [`AddRouter`](#Std.Cli.SubcommandRouter.AddRouter(string,string,SubcommandRouter,string[])) · [`AddPersistentFlag`](#Std.Cli.SubcommandRouter.AddPersistentFlag(string,string,string)) · [`AddPersistentOption`](#Std.Cli.SubcommandRouter.AddPersistentOption(string,string,string,string)) · [`SetDefault`](#Std.Cli.SubcommandRouter.SetDefault(ArgParser,string)) · [`Match`](#Std.Cli.SubcommandRouter.Match(string[])) · [`Resolve`](#Std.Cli.SubcommandRouter.Resolve(string[])) · [`HelpText`](#Std.Cli.SubcommandRouter.HelpText()) · [`Count`](#Std.Cli.SubcommandRouter.Count()) · [`Has`](#Std.Cli.SubcommandRouter.Has(string))

<a id="Std.Cli.SubcommandRouter.ctor(string,string)"></a>
### SubcommandRouter(string programName, string description)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L31)

用 programName 与 description 创建一个子命令路由。

<a id="Std.Cli.SubcommandRouter.Add(string,string,ArgParser)"></a>
### void Add(string name, string description, ArgParser parser)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L50)

注册叶子子命令（其参数由 `parser` 解析）；无别名。

<a id="Std.Cli.SubcommandRouter.Add(string,string,ArgParser,string[])"></a>
### void Add(string name, string description, ArgParser parser, string[] aliases)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L56)

注册叶子子命令并挂别名；别名同样参与路由、`Has` 与未知命令建议，命中后归一为正名。 名字或别名与任何已注册名字 / 别名相撞抛 CliException（注册期暴露，不再静默覆盖）。

<a id="Std.Cli.SubcommandRouter.AddRouter(string,string,SubcommandRouter)"></a>
### void AddRouter(string name, string description, SubcommandRouter child)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L69)

注册嵌套子命令路由（`prog build package …`）；无别名。

<a id="Std.Cli.SubcommandRouter.AddRouter(string,string,SubcommandRouter,string[])"></a>
### void AddRouter(string name, string description, SubcommandRouter child, string[] aliases)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L74)

注册嵌套子命令路由并挂别名；相撞语义同 `Add` 的四参重载。

<a id="Std.Cli.SubcommandRouter.AddPersistentFlag(string,string,string)"></a>
### void AddPersistentFlag(string longName, string shortName, string help)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:89`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L89)

声明本层跨子命令全局 flag：Resolve 先把它从 argv 任意位置（子命令前后皆可）剥出， 剩余 token 才进路由；多层可各自声明（各管各的子树，根层优先取值）。 取值用 `CommandResolution.PersistentFlag` / `PersistentOption`。

<a id="Std.Cli.SubcommandRouter.AddPersistentOption(string,string,string,string)"></a>
### void AddPersistentOption(string longName, string shortName, string help, string defaultValue)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L96)

声明本层跨子命令全局 option（`--name v` / `--name=v` / `-s v` / `-sv`）； 剥离与取值语义同 `AddPersistentFlag`。

<a id="Std.Cli.SubcommandRouter.SetDefault(ArgParser,string)"></a>
### void SetDefault(ArgParser parser, string actionDescription)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L104)

给本层挂默认动作：argv 为空、或首 token 是 flag（`-x` / `--x`）且不匹配任何子命令时， 整段 argv 交给 `parser` 解析（其 `-h` 照常生效）。`xtask test --no-build` 这类 「无子命令 = 默认全量」的形态由它表达，不再需要调用方在路由前手写拦截。

<a id="Std.Cli.SubcommandRouter.Match(string[])"></a>
### SubcommandMatch? Match(string[] argv)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:177`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L177)

单层 sugar：只看 argv[0]，命中叶子返回 `SubcommandMatch`。

其余情形（argv 为空 / argv[0] 是 `--help` / `-h` / 名字不认识）一律返回
null。不处理嵌套子命令，也不处理 persistent 剥离与默认动作。

<a id="Std.Cli.SubcommandRouter.Resolve(string[])"></a>
### CommandResolution Resolve(string[] argvIn)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:197`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L197)

推荐入口：递归下钻整棵命令树，返回三态互斥的 `CommandResolution`。

IsMatch / IsHelp / IsUnknown 恰好一个为真。本层 persistent 旗标先从 argv
任意位置剥出（见 `AddPersistentFlag`）；叶子 `ArgParser.Parse` 的 option
级错误（未知 flag / 缺值 / 缺 positional）以 `CliException` 向上透传。

<a id="Std.Cli.SubcommandRouter.HelpText()"></a>
### string HelpText()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:289`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L289)

渲染顶层帮助：programName - description + SUBCOMMANDS 列表（别名括注）， 声明过默认动作 / persistent 旗标时分别附 `(default)` 行与 GLOBAL OPTIONS 段。

<a id="Std.Cli.SubcommandRouter.Count()"></a>
### int Count()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:314`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L314)

已注册的子命令个数。

<a id="Std.Cli.SubcommandRouter.Has(string)"></a>
### bool Has(string name)

[`src/libraries/z42.cli/src/SubcommandRouter.z42:317`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L317)

指定名字（或别名）是否已注册为子命令。

## SubcommandMatch

[`src/libraries/z42.cli/src/SubcommandRouter.z42:361`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L361)

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

[`src/libraries/z42.cli/src/SubcommandRouter.z42:366`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L366)

用命中的子命令名（别名归一后的正名）与解析结果创建匹配对象。

<a id="Std.Cli.SubcommandMatch.Name()"></a>
### string Name()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:372`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L372)

命中的子命令名。

<a id="Std.Cli.SubcommandMatch.Result()"></a>
### ParseResult Result()

[`src/libraries/z42.cli/src/SubcommandRouter.z42:375`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.cli/src/SubcommandRouter.z42#L375)

命中的叶子 ParseResult。

