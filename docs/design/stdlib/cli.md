# z42.cli — CLI argv parser

> 落地版本：2026-05-16（add-z42-cli）
> 包路径：`src/libraries/z42.cli/`
> 命名空间：`Std.Cli`（`CliException` 在 `Std`）

## 职责

`ArgParser` + `ParseResult` + 自动 `-h/--help` 文本生成。最小子集覆盖
flag / option / positional；为 `scripts/*.sh → *.z42` 迁移提供基础。

**对标**：Python `argparse`（最小核心）+ Rust `clap` (builder API) +
Go `flag`（命名风格）。

## API surface

```z42
class ArgParser {
    ArgParser(string programName, string description)
    void AddFlag(string longName, string shortName, string help)
    void AddOption(string longName, string shortName, string help, string defaultValue)
    void AddPositional(string name, string help)
    ParseResult Parse(string[] argv)
    string HelpText()
}

class ParseResult {
    bool   GetFlag(string longName)
    string GetOption(string longName)
    string GetPositional(int index)
    int    PositionalCount()
    bool   ShowHelp()
}

class CliException : Exception     // in Std namespace
```

## 设计决策

| Decision | 选项 | 决定 | 理由 |
|----------|------|------|------|
| 1. Subcommand | 单层 / 嵌套 / 无 | **嵌套已落地** | `SubcommandRouter` 单层（2026-05-27）+ 嵌套 router 树 `AddRouter`/`Resolve`（2026-06-10）|
| 2. Short + long | 都支持 / 只 long | 都支持 | UNIX 惯例；shortName 可空字符串 |
| 3. Value 形式 | `--name v` `--name=v` `-nv` | 前两种 | `-nv` 易歧义 |
| 4. 类型转换 | string + bool only / int / float | string + bool only | 通用；caller 自行 Parse |
| 5. Required mark | required / 默认值即"有" | 默认值 | 简单；caller check `== ""` |
| 6. Positional 数量 | 严格必填 / 可选 | 必填 + **可选** | `AddPositional`（必填）+ `AddOptionalPositional`（可选，缺→`""`）；多余仍抛 |
| 7. Unknown flag | 抛异常 / 忽略 | 抛 | fail-fast |
| 8. -h/--help | auto / 手定义 | auto | 始终 register |
| 9. 异常类 | CliException 在 Std namespace | yes | 同 UriException / RegexException 模式 |
| 10. argv[0] | program-name override / skip | skip | program name 由构造时给定 |

## 实现结构

```
src/CliException.z42  (~10 行)
└── class CliException : Exception   in Std namespace

src/ParseResult.z42   (~75 行)
└── class ParseResult — getter API + parallel arrays（与 ArgParser 共享）

src/ArgParser.z42     (~340 行)
├── declaration API   — AddFlag / AddOption / AddPositional
├── Parse(argv)       — 扫描 + 分发到 _parseLong / _parseShort
├── HelpText()        — auto-generated Usage / FLAGS / OPTIONS / ARGS
└── _ensureXxxCap     — 2x grow helpers（同 stdlib 模式）
```

## 不支持（Deferred）

### ~~cli-future-subcommand~~ — ✅ 已落地 2026-05-27 (`609b870e`)

`Std.Cli.SubcommandRouter` — `git` / `cargo`-style subcommand
dispatch on top of `ArgParser`. Top-level `--help` lists commands;
unknown subcommand surfaces a Levenshtein "did you mean ..."
suggestion.
- **当前 workaround**：调用方手动 `if argv[0] == "build"` 分支再 new sub-parser

### ~~cli-future-optional-positional~~ — ✅ 已落地 2026-06-10 (add-cli-optional-positional)

`ArgParser.AddOptionalPositional(name, help)` —— 可选 positional。由 xtask/launcher
迁移 Std.Cli（migrate-xtask-launcher-to-std-cli）的 dogfood 暴露：其命令遍地
`test e2e [interp|jit]` / `build stdlib [lib]` / `package [release|debug]` 等可选 positional，
而原 `AddPositional` 严格必填、`AllowExtras` 不覆盖 positional。

- 缺省：`GetPositional(i)` 返回 `""`（声明槽位默认空串）；`PositionalCount()` 仍为实际提供数。
- **排序约束**：可选必须在所有必填之后（`AddPositional` 在 optional 之后 → 声明期抛）。使 Parse 的缺-必填校验简化为 `提供数 ≥ requiredCount`（leading 必填数）。
- 多余 positional（超声明总数）仍抛，不进 `Extras()`。
- `GetPositional(i)` 边界从"已提供数"放宽到"声明总数"（未提供可选 → `""`；越声明数仍抛）。
- HelpText：必填 `<name>`、可选 `[name]`。

实现：`ArgParser._positionalOptional: bool[]` parallel array + `AddOptionalPositional`
+ Parse requiredCount 校验 + HelpText 分支；`ParseResult.GetPositional` 边界。
11 [Test]（缺/有、必填+可选混合、与 flag 混用、排序违规、多余报错、help 渲染、help 短路）。

### 变长 positional（Deferred）

`args...` 收集剩余为 list —— 暂未实现；当前可用可重复 option（`-D k=v`）或调用方手切分替代。

### ~~cli-future-nested-subcommand~~ — ✅ 已落地 2026-06-10 (add-cli-nested-subcommands)

多层命令树（`prog build package …`），消费端 xtask（`build package`）/
launcher 不再手撸多层 `if-else`。在单层 `SubcommandRouter` 之上加：

- `AddRouter(name, desc, child)` — 注册嵌套子 router（与叶子 `Add(name, desc, ArgParser)` 并存；parallel array `_childRouters`/`_isRouter`）。
- `Resolve(argv) → CommandResolution` — 递归下钻整棵树，回溯 `Prepend` 拼 `Path()`；三态正交：
  - `IsMatch()`：命中叶子且非 help → `Result()` 叶子 `ParseResult`，`Path()` 命中链。
  - `IsHelp()`：任一层 `-h`/`--help`、router 层无 token、或叶子 `ShowHelp()` → `HelpText()` 为**该层**帮助（router 列子命令；叶子走 `ArgParser.HelpText()`）。**这是 `xtask build package -h` 不再误触发打包的修复点**。
  - `IsUnknown()`：任一 router 层遇未知 token → `ErrorMessage()` + 该层 `HelpText()`。
- `CommandResolution`（`src/CommandResolution.z42`）：三态结果类，静态工厂 `Match`/`Help`/`Unknown` 保证互斥。

**设计要点**：
- 叶子 `ArgParser.Parse` 的 option 级错误（未知 flag / 缺 value / 缺 positional）以 `CliException` **向上透传**，`Resolve` 不吞——它只做路由 + help/unknown-subcommand 分流。
- 递归用**回溯前插**（子节点公有 `Resolve` + `Prepend`），规避同类型跨实例私有访问；命令树 2~3 层、path 极短，重建数组开销可忽略。
- 单层 `Match`/`SubcommandMatch` **保留不变**，作单层场景 sugar（回归 `cli_subcommand.z42` 全绿）。

消费端范式：
```z42
CommandResolution res = root.Resolve(argv);
if (res.IsHelp())    { Console.WriteLine(res.HelpText()); Environment.Exit(0); }
if (res.IsUnknown()) { ConsoleError.WriteLine(res.ErrorMessage());
                       Console.WriteLine(res.HelpText()); Environment.Exit(2); }
string[] path = res.Path();   // e.g. ["build", "package"]
ParseResult r = res.Result();
```

测试：`tests/cli_nested_subcommand.z42`（14 [Test]）覆盖 2/3 层命中、各层 help、各层 unknown、三态互斥、叶子 CliException 透传、单层 Match 回归。

### ~~cli-future-required-option~~ — **✅ 已落地 2026-05-26 (add-cli-required-option)**

Shipped: `ArgParser.AddRequiredOption(long, short, help)` + tracking of
"was this option explicitly written from argv" via the new
`ParseResult.WasOptionSet(name) → bool` accessor.

Behaviour:
- Missing required option → `CliException` ("required option '--X' missing")
- `--help` / `-h` short-circuits the check so users can print help with
  no other args
- Help text auto-appends " (required)" to the option's help string

Implementation:
- New `bool[] _optionRequired` parallel array on `ArgParser` (grown
  alongside the existing 4 option arrays in `EnsureOptionCap`)
- New `bool[] _optionWasSet` on `ParseResult`, written by `ParseLong` /
  `ParseShort` whenever an option's value is consumed from argv
- Validation pass at the end of `Parse` iterates required options and
  throws if `_optionWasSet[i]` is still false

### ~~cli-future-type-conversion~~ — **✅ 已落地 2026-05-26 (add-cli-type-conversion)**

Shipped: 4 typed getters on `ParseResult` for the common scalar shapes,
no need for a separate `AddIntOption` declaration API (all options stay
declared as strings; conversion happens at retrieval time).

| 方法 | 行为 |
|---|---|
| `GetIntOption(name) → int` | `int.Parse`; throws `CliException` with `"option '--X' value 'Y' is not a valid int"` on failure |
| `GetLongOption(name) → long` | `long.Parse` (i64) |
| `GetDoubleOption(name) → double` | `double.Parse` (f64) |
| `GetBoolOption(name) → bool` | ASCII-fold + match `true`/`false`; throws on other strings |

Design note: the lazy "parse at retrieval" approach (vs eager
`AddIntOption("port", ..., 8080)`) lets callers keep the existing
string-based `AddOption` API + opt into type checks only where
needed. Smaller API surface, easier composition.

19 new tests across both features: required missing/provided
(long/short/equals forms); --help short-circuits required check;
WasOptionSet true/false; int (basic / explicit / negative / invalid);
long max i64; double + invalid; bool true/false/uppercase/invalid;
combined required + typed in a realistic "tiny HTTP server" parser.

### ~~cli-future-env-fallback~~ — ✅ 已落地 2026-05-26 (`473eb0bf`)

`ArgParser.AddOptionWithEnv(longName, shortName, help, default,
envName)` — flag value taken from `Environment.GetEnvironmentVariable(envName)`
when not provided on argv. Common CI pattern (`API_TOKEN=...`).

### ~~cli-future-mutually-exclusive~~ — ✅ 已落地 2026-05-26 (`473eb0bf`)

`ArgParser.AddMutuallyExclusive(string[] names)` registers a group;
parse-time conflict surfaces `CliException` listing the offending
flag pair.

### ~~cli-future-repeated-option~~ — **✅ 已落地 2026-05-26 (add-cli-repeated-option)**

Shipped: `AddRepeatedOption(long, short, help)` + `GetRepeatedOption(name)
→ string[]`. Each `-D foo=1 -D bar=2` appends; `GetOption(name)` still
returns the last value for back-compat.

Implementation:
- New `bool[] _optionRepeated` parallel array on ArgParser
- New `_RepeatedList[] _optionLists` on ParseResult (internal
  `_RepeatedList` wrapper has `_values: string[] + _count: int` plus
  `Append(v)` / `Snapshot()` since z42 has no native `string[][]`)
- `ParseLong` / `ParseShort` now append to the per-option list when
  `_optionRepeated[oi]` is true

Help text auto-appends " (repeatable)" to the option's help string.

### ~~cli-future-short-flag-cluster~~ — **✅ 已落地 2026-05-26 (add-cli-short-flag-cluster)**

Shipped: GNU-style `-abc` = `-a -b -c` for **boolean flags only**.
Clustering with an option-short character (one that requires a value)
throws `CliException` with a clear message — the ambiguity around
"is this `-p 8080` or `-p` followed by value `8080`?" makes cluster +
option composition genuinely unsafe.

Behaviour:
- `-vd` where both `v` and `d` are registered flag-shorts → both set
- Order independent (`-cab` == `-abc`)
- Unknown short in cluster → "unknown option" error
- Cluster containing option-short → "clusters are flags only" error
  with the offending char called out
- Single-char short (`-v`) bypasses the cluster path entirely
  (matches existing v0 behaviour)

Two-pass validation: first pass verifies every char is a registered
flag-short; second pass commits the writes. Avoids partial-cluster
state when the cluster is invalid mid-way.

11 new tests cover: 2-flag / 3-flag clusters; order independence;
single-short flag and option regression checks; cluster then separate
flag; cluster then `--option value`; cluster + option-short rejection;
cluster + unknown rejection; `tar -xv -f out.tar` pattern; single-char
short doesn't trigger cluster path.

### ~~cli-future-strict-vs-extras~~ — **✅ 已落地 2026-05-26 (add-cli-strict-vs-extras)**

Shipped: opt-in pass-through mode via `ArgParser.AllowExtras()`. With it
enabled, unknown flags / options go into `ParseResult.Extras() →
string[]` instead of throwing `CliException`. Strict mode (default)
unchanged.

Behaviour:
- `AllowExtras()` is a one-shot toggle on the parser
- Unknown long / short tokens (after exhausting registered lookups)
  appended verbatim to `_extras` and consumed (no value-positional
  consumption attempted — keeps the model simple)
- `--help` / `-h` still triggers `ShowHelp` ahead of pass-through
- Short-flag cluster path is bypassed when extras allowed (cluster
  ambiguity vs unknown-token preservation isn't worth resolving in v0)
- Known options + repeated options + required validation all still
  apply alongside extras

Real-world use: wrapper tools (`docker-wrap --my-flag <docker args>...`)
that own a subset of the namespace and forward the rest.

## 跨 stdlib 交互

- 依赖 `z42.core`（基础类型 + Exception）
- 依赖 `z42.text`（StringBuilder for HelpText 拼接）
- 与 `z42.io.Environment`（GetCommandLineArgs / Exit）配合：标准 main 流程
- 与 `z42.diagnostics.Log` 配合：典型 main 解析后调 `Log.SetMinLevel`

## 实施期发现

无特别 surprise。z42 `string` 字面值在 source 里用普通 `"..."`，escape 比照
JSON/C 风格。`string.Length` / `string.CharAt(i)` / `string.Substring(start, len)`
都按预期工作。
