# Spec: 单文件运行（single-file-run）

## ADDED Requirements

### Requirement: `z42 run <file>.z42` 编译并运行单个源文件

#### Scenario: 最小程序
- **WHEN** 当前目录有 `hello.z42`，内容为 `using Std.IO;` + 顶层 `void Main()` 打印 `Hello, World!`，
  执行 `z42 run hello.z42`
- **THEN** 标准输出为 `Hello, World!`，退出码 0，且**当前目录不产生任何新文件或目录**
  （产物全部落在缓存目录）

#### Scenario: 不需要 namespace
- **WHEN** 单文件源码没有 `namespace` 声明
- **THEN** 正常编译运行（与工程模式一致；`namespace` 始终是可选的）

#### Scenario: 传递程序参数
- **WHEN** 执行 `z42 run greet.z42 -- 小明`
- **THEN** 程序通过 `Environment.GetCommandLineArgs()` 读到 `小明`，输出 `Hello, 小明!`

#### Scenario: 简写形式
- **WHEN** 执行 `z42 hello.z42`
- **THEN** 行为与 `z42 run hello.z42` 完全一致（与既有 `.zpkg` / `.zbc` 简写同一规则）

#### Scenario: 第二次运行走增量
- **WHEN** 源文件未修改，再次 `z42 run hello.z42`
- **THEN** 输出与首次一致，且不重新编译（stderr 无构建进度行）

#### Scenario: 源文件不存在
- **WHEN** 执行 `z42 run nope.z42`，该文件不存在
- **THEN** stderr 给出「文件不存在」的明确报错，退出码 2，**不**创建缓存目录

#### Scenario: 与 `--bin` 同用
- **WHEN** 执行 `z42 run hello.z42 --bin foo`
- **THEN** 报错说明单文件没有多目标概念，退出码 2

### Requirement: 单文件的诊断位置指向用户的文件

#### Scenario: 编译错误的路径是用户输入的路径
- **WHEN** `hello.z42` 第 3 行第 5 列把 `Console` 误拼成 `Consle`，在该文件所在目录执行
  `z42 run hello.z42`
- **THEN** stderr 含 `hello.z42(3,5): E0401: undefined: Consle`，退出码 1
- **AND** 输出中**不出现**缓存目录路径，也**不出现**源文件的绝对路径

#### Scenario: 源文件不在当前目录之下
- **WHEN** 在 `/tmp` 执行 `z42 run /Users/me/code/hello.z42` 且该文件有编译错误
- **THEN** 诊断路径为该文件的绝对路径（不生成 `../../…` 形式的相对路径）

### Requirement: 单文件只能使用标准库

#### Scenario: 需要依赖时的指引
- **WHEN** 单文件源码引用了 stdlib 之外的包
- **THEN** 编译报错（沿用现有跨包解析诊断），且 `z42 run` 的帮助文本说明单文件只能用标准库、
  需要依赖时用 `z42 new` 建工程

### Requirement: `SourceDiscovery` 接受 rooted 字面文件路径

#### Scenario: rooted 字面路径命中
- **WHEN** `[sources].include` 含一个存在的绝对文件路径（不含 `*` / `?`）
- **THEN** 该文件被发现，且返回的路径与 include 中给出的一致

#### Scenario: rooted 字面路径不存在
- **WHEN** 该绝对路径不存在
- **THEN** 该 include 项贡献零个文件（由上层报 `no sources matched [sources].include`）

#### Scenario: rooted 但含通配符
- **WHEN** include 为 `/abs/dir/*.z42`
- **THEN** 仍走既有 glob 展开分支（字面直通只对无通配符的 rooted 路径生效）

#### Scenario: 相对字面路径语义不变
- **WHEN** include 为 `src/Main.z42`（相对、字面）
- **THEN** 仍按相对 `projectDir` 解析（**不**改为相对进程工作目录）

## MODIFIED Requirements

### Requirement: 诊断中的源文件路径

**Before:** 诊断与警告里的源路径直接取 `[sources]` 发现结果；工程模式下形如 `./src/Main.z42`，
绝对路径清单则原样打出绝对路径。

**After:** 读取源文件仍用发现结果，**显示**路径按当前工作目录相对化：文件在 cwd 之下 → 相对路径，
否则保持绝对。工程模式下发现结果本就是相对路径 ⇒ **现有输出逐字不变**。

## Pipeline Steps

受影响的 pipeline 阶段：

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及
- [ ] TypeChecker —— 不涉及
- [x] 源发现（`SourceDiscovery`）—— rooted 字面路径直通
- [x] Driver（`z42c.driver`）—— 显示名与读取路径分离
- [x] Launcher（`z42` 命令面）—— 单文件分支、合成清单、简写路由
- [ ] IR Codegen / VM interp —— 不涉及
