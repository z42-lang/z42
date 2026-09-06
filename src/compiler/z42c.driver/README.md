# z42c.driver

## 职责
CLI 入口（命令路由）。唯一 **exe** 子包，对外别名 = 用户 `z42c` 命令。命令面：前端 dump（`--dump-tokens` / `--dump-ast` / `--dump-bound`）、`--emit-zbc`（源 → IrGen → ZbcWriter → `.zbc`）、`build <project.z42.toml>`（产 zpkg，含文件级增量 + 运行配置侧车 + `[profile.*.runtime]` 旋钮名校验）、`build --workspace`（多包拓扑序）、manifest-check。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Main.z42` | `void Main()`：读 `Environment.GetCommandLineArgs()`，路由 `--dump-keywords` → `DumpTool.DumpKeywords`、`--dump-tokens`/`--dump-ast` → `DumpTool`、`--dump-bound` → `SemanticDump`、`--emit-zbc <src> <out>` → `IrDump.ZbcBytes` + `File.WriteAllBytes`、`build` → `_build`（`namespace Z42.Driver`）|
| `src/IndexedDist.z42` | indexed dist 投影（add-indexed-zpkg-min-patch）：散装 zbc 原样落盘（字节相等不触碰→最小 patch）+ FILE 主文件 + 孤儿清理 |
| `src/BuildPaths.z42` | dist/cache 级联解析 + pack 模式守卫（`_distModeMatches`：packed↔indexed 切换使 preserved 失效）|
| `src/ProfileKnobs.z42` | 构建期旋钮名校验（compiler-checks-knob-names）：`_validateProfileKnobs` 在 `_build` 早期扫全部 `[profile.<n>.runtime]`——未知名 → warning + 最近邻建议（全集问 `Std.Runtime.RuntimeConfig.Names()`，不留第二份清单）；`[profile.<n>]` 下直接写键 → 致命，库工程同样管 |
| `src/RuntimeConfigSidecar.z42` | `dist/<name>.runtimeconfig.toml` 侧车生成（`[runtime]` 旋钮 + `[properties]` 应用属性，分表）|
| `src/IncrementalDriver.z42` | 文件级增量编排（add-file-level-incremental）：`Prepare`（种子 → parse-all → **声明面闸门** → token 边闭包 → cached zbc 读回 + meta 残留回填，失败降级 fresh）/ `WriteMetas`（meta + 包级源清单落 cache）/ `_writeCacheZbc` |
| `src/SurfaceHash.z42` | 声明面指纹（fix-z42c-incremental-closure）：token 流去掉方法/属性/索引器**体内** token 后哈希 —— 增量传播闸门的输入，让「改注释 / 改函数体」不波及引用方 |

## 入口点
`Z42.Driver.Main`（auto-detected exe 入口）。
用法：`z42c --dump-tokens|--dump-ast|--dump-bound <file.z42>` / `z42c --emit-zbc <file.z42> <out.zbc>` /
`z42c build <project.z42.toml> [--release] [--no-incremental]`（`[project].pack` 决议 packed/indexed：debug 默认 indexed——散装 zbc + FILE 主文件，`pack=false ∧ --release` 报错） / `z42c build --workspace [--output-dir <d>]`。

## 增量编译（文件级，add-file-level-incremental 2026-07-08）
单工程 `build` 的判定与组装 SoT = cache（`<rel>.zbc` fullMode + `<rel>.meta` + 包级源清单，
`[build].cache_dir` → `${output_dir}/.cache` → `<projectDir>/.cache` 级联）。种子（hash/
条目/清单）→ token 保守边传递闭包 → **仅失效闭包重编**（typecheck+codegen），其余 IrModule
经 ZbcReader 读回 + meta 残留回填（块 label / 模块池原序 / TIDX idx）；TSIG 恒全包重算；
全命中完全跳过（`no changes; preserved`）。`--no-incremental` 强制全量；`Z42_INCR_DEBUG=1`
看种子与传播链。硬验收 = `xtask test incremental` 暴力对账器（增量 == 全量逐字节 + 计时）。
workspace/flat 模式不落 cache、不 probe（见 [project.md 增量编译节](../../../docs/design/compiler/project.md)）。

## 依赖关系
→ z42c.syntax, z42c.semantics, z42c.core, z42c.pipeline, z42.ir, z42.project。stdlib（Std / Std.IO）自动可用。

`_build` 遇本地 path 依赖（`DepEntry.Path` 非空）时，先经 `z42c.pipeline` 的 `PathDepPlan.Resolve` 建叶子在前的传递闭包 → 逐成员现建 + 累积 libsDirs，`_bundleExeDeps` 再把私有 path 依赖 zpkg colocate 进消费方 dist（真-stdlib 走 Z42_LIBS 不复制）。机制见 book `compiler/project-model.md` 路径依赖闭包。

## 运行（自举产物）
z42c 跨包 dep 解析读 `Z42_LIBS`。**通常无需手动设置**：z42vm 会把它解析出的 libs 目录
（`<binary>/../libs` SDK 布局 / dev flat view）回写进 `$Z42_LIBS`，z42c 透明读到（见
vm-architecture.md 的「VM 启动流程」`libs_env_to_publish`）。SDK 安装后直接：
```
z42vm <programs/z42c>/z42c.driver.zpkg -- --emit-zbc <file.z42> <out.zbc>
z42vm <out.zbc> Main        # 执行自举编译器产物
```
仅当 libs 不在 VM 的默认搜索路径（如把 z42c 后端包 + 前端库 + stdlib 临时合到自定义 flat 目录）时，
才需显式 `Z42_LIBS=<flat>` 覆盖——此时**必须是单个**目录含全部依赖（见 self-hosting.md 的
Z42_LIBS 单目录陷阱）。端到端冒烟由 `xtask test compiler` 的 e2e 步骤覆盖（自检程序 +
div-by-zero oracle）。
