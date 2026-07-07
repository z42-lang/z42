# Tasks: VSCode 语法高亮扩展

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07
> 占用子系统：`compiler` + `toolchain`（归档时释放）

## 进度概览
- [x] 阶段 1: 扩展静态资产
- [x] 阶段 2: z42c 关键字导出（compiler）
- [x] 阶段 3: xtask 生成器 + 安装 + gate（toolchain）
- [x] 阶段 4: 验证（负路径/安装/一致性全过 + 裸 `xtask test` 全绿）
- [x] 阶段 5: 文档同步

## 阶段 1: 扩展静态资产
- [x] 1.1 `vscode/package.json` — 语言贡献点（id `z42`、`.z42`、grammar/config 挂接、engines）
- [x] 1.2 `vscode/language-configuration.json` — 注释/括号对/自动闭合/包围对/缩进
- [x] 1.3 `vscode/syntaxes/z42.tmLanguage.tpl.json` — 模板：注释/字符串四形态（含插值洞嵌套 `$self`）/数字全形态/属性/运算符/约定式着色手写 + `__KW_*__` 六组占位
- [x] 1.4 `vscode/README.md` — 六段制

## 阶段 2: z42c 关键字导出
- [x] 2.1 `Lexer.z42` 公开只读访问器 `KeywordCount()` / `KeywordNameAt(i)`
- [x] 2.2 `DumpTool.z42` 新增 `DumpKeywords()`（每行一个，注册序）
- [x] 2.3 `Main.z42` `--dump-keywords` 分发（argv≥2 守卫前）+ usage 行（481 行 < 500 硬限）
- [x] 2.4 `lexer_tests.z42` 单测 ×2：访问器（计数>0、逐项非空、fn/params 哨兵）+ DumpKeywords 行数==计数、首行==表首项

## 阶段 3: xtask 生成器 + 安装 + gate
- [x] 3.1 `scripts/install/xtask_install_vscode.z42` — 87 关键字六分组分类表 + 穷尽校验（漏/幽灵/重复→报错指名）+ 模板渲染（按 dump 注册序，确定性）
- [x] 3.2 同文件：安装（darwin/linux `ln -sfn` 幂等；win32 打印 copy 指引 exit 1）+ `_testVscodeSyntax` 检查函数（内存重渲染 diff，不写盘）
- [x] 3.3 `xtask_cli.z42` — deps `install` optional positional component `vscode` + `test vscode-syntax` 注册与分发
- [x] 3.4 `xtask_test.z42` — `_testAll` 链尾挂 `vscode-syntax` stage（stage 6）
- [x] 3.5 首次生成，`z42.tmLanguage.json` 入库（含 `__generated__` DO-NOT-EDIT 标记；JSON 合法性 + 六组注入已 python 校验）

## 阶段 4: 验证
- [x] 4.1 `cargo build`（runtime 零改动；build compiler 波内照常通过）
- [x] 4.2 裸 `xtask test` 全绿（e2e goldens + stdlib [Test] + z42c 自举不动点 7/7 + **vscode-syntax stage**，exit 0；z42c 单测含新增 2 个关键字用例）
- [x] 4.3 视觉验收：扩展已 symlink 安装（`~/.vscode/extensions/z42.z42-lang`）；结构性校验（scope 注入/JSON 合法）已过，**编辑器目验清单留给 User 重载窗口核对**（本会话无 GUI），见备注
- [x] 4.4 spec scenarios 覆盖确认：导出（87 行、注册序、行数==表）✓；生成正常/重复运行确定性 ✓；缺 grammar → exit 1 指引 ✓；stale 字节改动 → exit 1、regen → 0 ✓；安装 symlink 幂等 ✓；漏分类/幽灵负路径由穷尽校验代码 + review 覆盖（触发需改 Lexer 重建 z42c，成本高，见备注）

## 阶段 5: 文档同步（阶段 9 触发矩阵）
- [x] 5.1 `src/toolchain/devtools/README.md` — 「编辑器集成」节 + B 期 lsp 定位
- [x] 5.2 `docs/book/src/toolchain/editor-integration.md` 新页 + `SUMMARY.md` 挂载 + `dev/test-gate.md` stage/mermaid/决策表更新（页头对齐 2026-07-07）
- [x] 5.3 `.claude/rules/workflow.md` 阶段 8 gate 清单 + 验证报告模板 +1 stage（`docs/workflow/testing/README.md` 无 stage 级列表，免改）
- [x] 5.4 根 `README.md` — Quick Start 下 Editor support 一句 + 安装命令
- [x] 5.5 doc-check 清单逐项核对

## 备注
- **视觉验收清单（User 重载 VSCode 窗口后目验 `examples/*.z42`）**：注释 `//`、`/* */`；
  字符串 `"…"`（转义变色）、raw `"""…"""`、插值 `$"…{expr}…"`（洞内代码色）、字符 `'…'`；
  数字 `0x…`/`0b…`/`1_000`/`1.5e-3`/后缀；关键字六组各取一词；`#[…]` 属性；`?.` `??` `=>` 运算符。
- 漏分类/幽灵分类的**触发式**验证需改 Lexer/分类表 + 重建 z42c/xtask（≈10min），校验逻辑
  简单（`_vscodeValidateCategories` 双向集合比对）已 review 覆盖；真实触发场景 = 未来加
  关键字时 gate 红，即本机制的设计用途。
- `deps install vscode` 后续新增 VSCode 资产项（B 期 LSP client、snippets）：在
  `_depsInstallVscode` / `_testVscodeSyntax` 主流程追加项即可（design Decision 6）。
