# z42c.driver

## 职责
CLI 入口（命令路由）。唯一 **exe** 子包，对外别名 = 用户 `z42c` 命令。命令面：前端 dump（`--dump-keywords` / `--dump-tokens` / `--dump-ast` / `--dump-bound` / `--dump-ir`）、`--emit-zbc`（源 → IrGen → ZbcWriter → `.zbc`）、`build [<manifest>]`（产 zpkg，含文件级增量 + 运行配置侧车 + `[profile.*.runtime]` 旋钮名校验）、`build --workspace`（多包拓扑序）。编译实现在 `z42c.pipeline` / `z42c.semantics`，本包只做命令路由、构建编排与 dist 装配。

## 功能索引
入口 `Z42.Driver.Main`（auto-detected exe 入口）。

| 命令 | 说明 |
|------|------|
| `z42c --dump-tokens\|--dump-ast\|--dump-bound\|--dump-ir <file>` | 前端 / IR dump |
| `z42c --emit-zbc <file.z42> <out.zbc>` | 单文件编 `.zbc` |
| `z42c build [<manifest>] [--release] [--no-incremental] [--output-dir <d>] [--jobs <n>] [--opt <name>] [--no-opt <name>] [--compile-libs <dirs>] [--fix] [-q]` | 编工程；省略 manifest 时 `ManifestLocator.FindUp` 定位；`[project].pack` 决议 packed/indexed（debug 默认 indexed——散装 zbc + FILE 主文件；`pack=false ∧ --release` 报错） |
| `z42c build --workspace [--output-dir <d>]` | 按拓扑序构建全部 workspace 成员 |

完整选项见 `z42c build -h` 与 [cli-z42c-z42b.md](../../../docs/reference/src/toolchain/cli-z42c-z42b.md)。

## 增量编译（文件级）
`build` 的判定与组装 SoT = cache（`<rel>.zbc` fullMode + `<rel>.meta` + 包级源清单，`[build].cache_dir` → `${output_dir}/.cache`；workspace 成员由 `WsPlan.CacheDirs` 给出）。cache 不论是否增量都落盘（单工程 `--output-dir` 例外：该路径从不读 cache，故不写）。种子（hash / 条目 / 清单）→ token 保守边传递闭包 → **仅失效闭包重编**（typecheck + codegen），其余 IrModule 经 ZbcReader 读回 + meta 残留回填（块 label / 模块池原序 / TIDX idx）；TSIG 恒全包重算；全命中完全跳过（`no changes; preserved`）。`--no-incremental` 强制全量；`Z42_INCR_DEBUG=1` 看种子与传播链。

`_build` 遇本地 path 依赖（`DepEntry.Path` 非空）时，先经 `z42.project` 的 `PathDepPlan.Resolve`（`PathDepBuild.z42`）建叶子在前的传递闭包 → 逐成员现建 + 累积 libsDirs，`_bundleExeDeps` 再把私有 path 依赖 zpkg colocate 进消费方 dist（真-stdlib 走 Z42_LIBS 不复制）。

## 基础用法
SDK 安装后 `z42c` 直接可用；dev 树下经 VM 运行 driver zpkg：
```bash
z42vm <programs/z42c>/z42c.driver.zpkg -- --emit-zbc <file.z42> <out.zbc>
z42vm <out.zbc> Main        # 执行产物
```
跨包 dep 解析读 `Z42_LIBS`。通常无需手动设置：z42vm 把它解析出的 libs 目录（`<binary>/../libs` SDK 布局 / dev flat view）回写进 `$Z42_LIBS`（见 vm-architecture.md「VM 启动流程」的 `libs_env_to_publish`）。仅当 libs 不在 VM 默认搜索路径时才显式 `Z42_LIBS=<flat>`——此时**必须是单个**目录含全部依赖（见 self-hosting.md 的 Z42_LIBS 单目录陷阱）。

## 如何测试验证
```bash
./xtask test compiler       # z42c 自举不动点 + smoke（含自检程序 + div-by-zero oracle）
./xtask test compiler incremental    # 增量 == 全量 逐字节对账 + 计时（增量编译的硬验收）
```

## 关联文档
- 增量编译 / 项目模型：[z42-toml.md](../../../docs/reference/src/toolchain/z42-toml.md)、[project-model.md](../../../docs/internals/src/compiler/project-model.md)
- 自举：[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Main.z42` | `void Main()`：读 `Environment.GetCommandLineArgs()`，路由 `--dump-keywords` → `DumpTool.DumpKeywords`、`--dump-tokens`/`--dump-ast` → `DumpTool`、`--dump-bound` → `SemanticDump`、`--emit-zbc <src> <out>` / `--dump-ir <src>` → `_cmdSingleFile`（同一条单文件编译：Z42_LIBS 解析依赖 → `IrDump.BuildModuleDOpt` → 有错打诊断并非零退出，否则写 `.zbc` / 打 IR 文本）、`build` → `_cmdBuild`（`namespace Z42.Driver`）|
| `src/BuildCommand.z42` | `z42c build` 参数解析：未知选项报错、`-h`、`--quiet`；不给清单时 `ManifestLocator.FindUp` 定位（工作区 → `--workspace`）→ `_build` / `_buildWorkspace` |
| `src/WorkspaceBuild.z42` | workspace 构建编排（`_buildWorkspace*` / `_findWorkspaceToml`）：按 `[workspace.build].output_dir` 展开成员布局，成员产物落各自 dist，兄弟解析扫全成员 dist + Z42_LIBS |
| `src/ExeDeps.z42` | exe 产物的兄弟包依赖 bundling（`_bundleExeDeps`）：非框架依赖的已构建 zpkg（+ `.zsym`）复制进 exe dist |
| `src/BuildCache.z42` | 构建 cache 落盘：逐文件写 fullMode `.zbc` + meta + 包级源清单（单产物与多 exe 共用）|
| `src/BuildLog.z42` | 进度行开关（`--quiet` 抑制 `cached:` / `wrote ->` / `cache ->`；诊断不受影响）|
| `src/PathDepBuild.z42` | path 依赖闭包代建（`_buildPathDepClosure`）：`PathDepPlan.Resolve` 叶子在前的闭包 → 逐个 `_build` + 累积 libsDirs，结果经 `PathClosureOut` 回给调用方 |
| `src/IndexedDist.z42` | indexed dist 投影：散装 zbc 原样落盘（字节相等不触碰→最小 patch）+ FILE 主文件 + 孤儿清理 |
| `src/BuildPaths.z42` | pack 模式守卫（`_distModeMatches`：packed↔indexed 切换使 preserved 失效）+ handler 指纹 + 可复现 build_id；dist/cache 目录解析在 z42.project `BuildLayout` |
| `src/ProfileKnobs.z42` | 构建期旋钮名校验：`_validateProfileKnobs` 在 `_build` 早期扫全部 `[profile.<n>.runtime]`——未知名 → warning + 最近邻建议（全集问 `Std.Runtime.RuntimeConfig.Names()`，不留第二份清单）；`[profile.<n>]` 下直接写键 → 致命，库工程同样管 |
| `src/RuntimeConfigSidecar.z42` | `dist/<name>.runtimeconfig.toml` 侧车生成（`[runtime]` 旋钮 + `[properties]` 应用属性，分表）|
| `src/IncrementalDriver.z42` | 文件级增量编排：`Prepare`（种子 → parse-all → **名字级指纹 diff** → 失效闭包 → cached zbc 读回 + meta 残留回填，失败降级 fresh）/ `WriteMetas`（meta + 包级源清单落 cache）/ `_writeCacheZbc`。**`Prepare(..., canPreserve)`**：`canPreserve` 由调用方按「dist 主文件在 + pack 模式一致 + 非多 exe」预先算好——只有它为真时，全命中才可廉价早退（调用方马上 preserved、用不到 IrModule）；为假时**必须**把 cached zbc 读回来，否则调用方装配 dist 时拿不到模块只能全部重编 |
| `src/SurfaceHash.z42` | **名字级**声明面指纹：token 流剥掉方法/属性/索引器**体内** token 后，按「上一个声明的收尾符」切片，逐名字（类型/enum/自由函数/成员方法/成员字段/enum 成员）各出一个指纹 + 该文件声明面标识符集。增量闭包的判据来源——「改注释 / 改函数体」零波及、「新增类型 / 新增函数」只波及真正提到新名字的文件 |

## 依赖关系
`z42c.syntax` / `z42c.semantics` / `z42c.core` / `z42c.pipeline`（编译实现）、`z42.package`（IR + zbc/zpkg 读写）、`z42.project`（清单模型）、`z42.io`、`z42.toml`（运行配置侧车序列化）、`z42.text`（旋钮名最近邻建议）、`z42.crypto`（indexed 散装 zbc 内容 hash）。
