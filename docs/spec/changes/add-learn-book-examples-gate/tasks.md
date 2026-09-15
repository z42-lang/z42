# Tasks: 学习手册 + examples 清零 + 教程示例门禁

> 状态：🟡 进行中（2026-09-16 User 指令：完成安装章节 + 编写并运行 hello world）| 创建：2026-09-15
> 一个 PR 落地（分支 add-learn-book-examples-gate，叠在 #685 / #683 之上）；原计划的 PR-A1 清场并入本 PR。

## 进度概览

- [x] 阶段 1: examples 清场
- [x] 阶段 2: SDK 前置
- [x] 阶段 3: 门禁实现
- [x] 阶段 4: docs/learn 骨架 + 第 1、2 章
- [x] 阶段 5: CI / 部署 / changed 映射
- [ ] 阶段 6: 文档同步 + 验证 + 归档

## 阶段 1: examples 清场

- [x] 1.1 `embedding/{hello,multi_line}.z42`(+toml) → `src/toolchain/workload/fixtures/`；改 `xtask_test_platform.z42`、`ci.yml` 路径过滤、wasm/ios/android 注释与 .gitignore、workload README / platform-contract / desktop README
- [x] 1.2 `target_typed_new.z42` → `src/tests/classes/target_typed_new.z42`（断言化，覆盖 6 种目标类型来源）
- [x] 1.3 `global_using/`：现有 runner 没有「单 exe 多文件工程 + 期望输出」的形态（multi-exe 要求 ≥2 exe，dir 模式 golden 只编 source.z42）；原示例从未被运行，删除不损失覆盖。缺口记入 OUTLINE 第 20 章——该章示例将成为首个端到端覆盖
- [x] 1.4 `json_serde` / `struct_value_semantics`：`z42.json/tests/{serialize,deserialize}.z42` 与 `src/tests/types/struct*.z42` 已覆盖同类场景，删除
- [x] 1.5 4 个已知编不过的示例（exceptions / generics / oop / patterns）用的是 C# 写法（`catch when`、`out var`、`!` 后缀、`Option<int>.Some`、`new[]`、`return default;`、`{x:F2}`、三元里 `new(..)`、`class X;`、`and` 组合子）——多数本就不在 z42 语言设计里，属于「示例写错了」而非「语言欠账」；逐条是否要做由语言线另议（见备注），不在本变更登记为 Deferred
- [x] 1.6 删除其余 examples 内容与 `examples-known-broken.txt`；`_topLevelExamplesGate` 删除
- [x] 1.7 清理全仓对旧 examples 的引用（`.claude/` CLAUDE.md / rules spec.md·workflow.md / skills、README、tests/README、book member-accessors·named-arguments、design project·testing·embedding、workflow packaging·windows、源码注释）；进行中的他人 change（add-json-serde / add-partial-types / add-workload-command-dispatch 等）Scope 里写的 `examples/*.z42` 不改，由各自实施时改写成测试

## 阶段 2: SDK 前置

- [x] 2.1 本地实测：warm `xtask build sdk` 25s（含 apphost stub cargo + 各工具 publish）→ 按 D6 规则 test-host 直接跑 `build sdk`
- [x] 2.2 `xtask test examples` 找不到带 launcher 的 SDK 即红（不 skip），提示 `xtask build sdk`
- [x] 2.3 完整 gate（非 `--no-build`）在 examples stage 前 `_buildSdk`；CI test-host 在 `test all --no-build` 之前单独一步 `build sdk`

## 阶段 3: 门禁实现

- [x] 3.1 `xtask_examples_transcript.z42`：解析（锚点行 / `[exit: N]`）、`[..]` / `...` 匹配、`[ROOT]` 与 CRLF 规范化、逐行 diff、`--bless` 重写
- [x] 3.2 `xtask_examples_run.z42`：沙箱（临时目录 + 复制排除 *.console / example.toml / dist / artifacts）、ClearEnv 白名单、独立 HOME、单步超时、失败保留沙箱
- [x] 3.3 命令：`z42` → SDK 绝对路径（Windows `.exe`）；内建 `cd` / `cat` / `ls`；其余判红
- [x] 3.4 `example.toml`：本批章节不需要（timeout 走 `--timeout`），未实现；复制时已排除该文件，需要时再加（见备注）
- [x] 3.5 `xtask_examples_book.z42`：B1–B10 + `example=` 属性
- [x] 3.6 `xtask_test_examples.z42` 入口 + 报告；CLI `xtask test examples [path] [--sdk] [--book-only] [--bless] [--keep] [--timeout]`
- [x] 3.7 `_exampleRun`（清单 `[[example]]`）并入 targets stage（`_testManifestExamples`）；删 `xtask example` 顶层命令
- [x] 3.8 阴性对照 14 例全部判红且可定位：输出失配、退出码失配、B1 缺文件、B2 缺锚点、B3 行号 include、B4 整文件含锚点、B5 跨章节、B6 内联代码块、B7 孤儿脚本、B8 未覆盖工程、B9 无对应页面、B10 SUMMARY 缺页 / 死链、缺 SDK；外层设 `Z42_MODE=jit Z42_LIBS/Z42_HOME/Z42_CONFIG=/nonexistent` 结果不变；`--bless` 能把改坏的期望恢复成原内容

## 阶段 4: docs/learn 骨架 + 第 1、2 章

- [x] 4.1 `docs/learn/book.toml`（`create-missing=false`、site-url、edit-url）、`SUMMARY.md`、前言、`OUTLINE.md`
- [x] 4.2 `theme/z42-highlight.js`：`z42` 注册为 csharp 别名后重高亮（bundled highlight.js 有 csharp + registerAliases）
- [x] 4.3 第 1 章「安装 z42」（平台 / 一行安装 / 验证 / 选项 / 更新 / 卸载 / 手动安装 / 常见问题）
- [x] 4.4 第 2 章「Hello, World」+ `examples/getting-started/hello-world/{new,greet,typo}`（3 个会话脚本 9 步）
- [x] 4.5 `examples/README.md`
- [x] 4.6 `docs/agent/rules/learn-writing.md`

## 阶段 5: CI / 部署 / changed 映射

- [x] 5.1 `deploy-book.yml`：双书构建到 `_site` / `_site/learn`；触发加 `docs/learn/**`、`examples/**`；PR 仅构建 + `[ERROR]` / 残留 include / 空代码块判红
- [x] 5.2 `ci.yml`：test-host 加 `build sdk` 步骤；package-host 在 `test dist` 后 `test examples --sdk <pkg>`（含 Windows）；platform 路径过滤加 `examples/**`、`docs/learn/**`
- [x] 5.3 `xtask_test_changed.z42`：`examples/<part>/<chapter>` → `test examples <part>/<chapter>`；`docs/learn/` → `--book-only`；launcher / builder 追加 examples；test-gate.md 映射表同步

## 阶段 6: 文档同步 + 验证 + 归档

- [x] 6.1 `doc-system.md`：一节目标结构加 learn/、二节「学习手册」附注、D10、§8 例外（中文先行）
- [x] 6.2 `test-gate.md` stage 表 / 流程图 / 映射 / 实现表；`project.md` 删 `xtask example`；`verify-by-change.md` 加行；`.claude/rules/README.md` 登记 learn-writing
- [ ] 6.3 GREEN：`xtask test` 全绿 + 两书 mdbook build 无 ERROR；CI linux/macos/windows 各一次绿
- [ ] 6.4 归档（随本 PR）

## 备注

- **编译器 bug（Scope 外，待独立 change）**：release 构建下，局部数组以 `ref` 传给被调函数、在被调函数里被重新赋值为新数组后，
  逃逸分析仍把原数组栈分配，读回时 VM 报 `stack-alloc array handle used after its creating frame exited`。
  最小复现（`z42c build --release` 后运行）：`string[] got = new string[0]; Fill(ref got); Concat(acc, got);`，
  其中 `void Fill(ref string[] a) { a = new string[2]; }`。当前 main 与 nightly 均复现；`z42 run`（debug）不复现。
  门禁代码已改用返回值对象规避（`PageCheck`），不依赖该修复。
- 旧示例暴露的 C# 写法清单（1.5）交语言线评估：哪些应进 z42（如 `{x:F2}` 格式说明符、`catch when`），哪些明确不做。
- `example.toml`（`platforms` / `timeout-seconds` / 外部工具 `requires`）等到嵌入章节（需要 `cc` / `cargo`）时一并实现。
