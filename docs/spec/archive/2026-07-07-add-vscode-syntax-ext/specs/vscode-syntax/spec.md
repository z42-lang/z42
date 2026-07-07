# Spec: VSCode 语法高亮（vscode-syntax）

> 变更类型：toolchain（+ compiler 导出动词）。非 lang/ir/vm——无 IR Mapping /
> Pipeline Steps 段；zbc/zpkg 格式零变更、VM 零改动。

## ADDED Requirements

### Requirement: z42c 关键字导出（`--dump-keywords`）

#### Scenario: 正常导出
- **WHEN** 运行 `z42c --dump-keywords`
- **THEN** stdout 输出 Lexer 关键字表全部条目，每行一个、按 `_initKeywords()` 注册序，exit 0

#### Scenario: 与 Lexer 表一致
- **WHEN** Lexer 关键字表含 N 项
- **THEN** 导出恰为 N 行，且逐项与 `KeywordNameAt(i)` 相等（单元测试覆盖）

#### Scenario: 不影响既有动词
- **WHEN** 运行 `z42c build …` / `--dump-tokens <file>` 等既有命令
- **THEN** 行为与本变更前完全一致（自举不动点 gen1==gen2 仍成立）

### Requirement: grammar 生成器（模板 + 分类表）

#### Scenario: 正常生成
- **WHEN** 运行 `xtask deps install vscode`（依赖 `simplify-xtask-deps` 收敛后的 deps install，optional positional component）
- **THEN** 由 `z42.tmLanguage.tpl.json` + `--dump-keywords` 输出渲染出
  `syntaxes/z42.tmLanguage.json`（含 GENERATED 标记），同输入重复运行字节相同

#### Scenario: 漏分类（新关键字未入分类表）
- **WHEN** dump 中存在分类表未覆盖的关键字
- **THEN** 生成失败，报错指名该关键字与需补的分类表位置，exit 非 0

#### Scenario: 幽灵/重复分类
- **WHEN** 分类表含 dump 中不存在的关键字，或同一关键字落在两个分组
- **THEN** 生成失败并指名，exit 非 0

#### Scenario: 漂移检测（`test vscode-syntax`，install 不留 `--check` flag）
- **WHEN** 运行 `xtask test vscode-syntax` 且入库 `z42.tmLanguage.json`
  与重新生成结果不一致
- **THEN** exit 非 0，提示运行 `xtask deps install vscode` 重生成；一致则 exit 0，不写盘、不安装

### Requirement: 扩展安装

#### Scenario: macOS / Linux 安装
- **WHEN** 运行 `xtask deps install vscode`（darwin/linux）
- **THEN** `~/.vscode/extensions/z42.z42-lang` 为指向
  `src/toolchain/devtools/vscode/` 的 symlink；重复运行幂等（`ln -sfn` 语义）

#### Scenario: Windows
- **WHEN** 在 win32 运行安装
- **THEN** 打印手动 copy 指引，exit 1（自动化 deferred）

#### Scenario: 安装后生效
- **WHEN** 安装并重载 VSCode 窗口后打开任意 `.z42` 文件
- **THEN** 语言模式识别为 `z42`，高亮/括号匹配/注释切换（`Cmd+/` → `//`）可用

### Requirement: 高亮覆盖面（视觉验收清单）

#### Scenario: 词法元素全覆盖
- **WHEN** 打开覆盖以下元素的样例（`examples/*.z42`）：`//` 与 `/* */` 注释、
  `"…"`/raw `"""…"""`/插值 `$"…{e}…"`/字符 `'…'`、十进制/`0x`/`0b`/`_` 分隔/小数/
  指数/后缀数字、五组关键字、`#[…]` 属性、运算符（含 `?.` `??` `=>` `::` `..`）
- **THEN** 各元素获得对应 TextMate scope（主流主题下可辨着色）；插值洞内表达式
  以代码形态高亮而非字符串色

### Requirement: GREEN gate 防漂移

#### Scenario: Lexer 加关键字未重生成
- **WHEN** `_initKeywords()` 新增关键字但未运行生成器（grammar 未更新）
- **THEN** 裸 `xtask test` 在 `vscode-syntax` stage 失败（漏分类或字节 diff 命中）

#### Scenario: gate 常态通过
- **WHEN** grammar 与 Lexer 一致
- **THEN** `xtask test vscode-syntax` 通过，且耗时相对既有 stage 可忽略
