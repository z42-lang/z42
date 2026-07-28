# z42 工程文件规范（<name>.z42.toml）

z42 使用 **`<name>.z42.toml`** 作为工程配置文件，格式为 TOML。
一个目录下最多一个 `*.z42.toml`，支持单工程和多工程工作区两种形态。

> **本文档的边界**：描述用户 manifest 字段（`[build]` / `[[exe]]` / `[dependencies]` / `[workspace]` 等）与构建编排语义。**不描述** `.zbc` / `.zpkg` 二进制格式（归 [`compilation.md`](compilation.md)）、编译器内部数据结构（归 [`compiler-architecture.md`](compiler-architecture.md)）。

---

## 层次概览

本文档按复杂度递进，分六个层次描述完整的 manifest 语义：

| 层次 | 内容 | 场景 |
|------|------|------|
| L1 | 包身份 + 入口 | 最小可构建工程 |
| L2 | 源文件配置 | 多文件工程 |
| L3 | 构建产物配置 | 控制输出格式和目录 |
| L4 | 运行时 Profile | debug / release 分离 |
| L5 | 依赖管理 | 引用外部库 |
| L6 | 工作区 | monorepo |

---

## L1 — 包身份（最小工程）

每个工程必须有唯一标识和产物类型。

```toml
[project]
name    = "hello"      # 工程名，kebab-case
version = "0.1.0"      # SemVer
kind    = "exe"        # exe | lib
entry   = "Hello.Main" # 可选；省略时由 PackageCompiler 自动发现 Main
```

**字段说明：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✅ | kebab-case；作为输出文件基名和依赖引用键 |
| `version` | string | ✅ | SemVer，如 `"0.1.0"` |
| `kind` | `"exe"` \| `"lib"` | 单目标必填；多目标用 `[[exe]]` 时省略 | 可执行程序 or 类库 |
| `entry` | string | ❌ 可选 | 完全限定入口函数。**省略时**`PackageCompiler` 自动从编译后的 module 查找 `Main`（优先 `<Namespace>.Main` 再 `<Namespace>.main` 再裸 `Main` / `main`）；找不到则**编译期报错**（2026-05-14 起）|

**`name` 与命名空间的关系：**

`name` 是包的文件标识符，与命名空间**无关**。命名空间完全由源文件中的 `namespace xxx;` 声明决定，编译器在构建时从源文件中收集并写入 zpkg 的 `namespaces` 字段。`[dependencies]` 中填写的是包名（用于找文件），`using` 语句中填写的是命名空间（由源文件决定），两者无需一致。

**一个 zpkg 可包含多个命名空间（C# 风格）：**

一个 zpkg 不限定只含单一命名空间。像 C# 程序集一样，一个 lib 包内不同 `.z42` 源文件可以属于不同命名空间，所有命名空间都会被收集到 zpkg 的 `namespaces` 字段中。

```
# 包 my-sdk 的源文件结构：
my-sdk/src/
  Client.z42       → namespace Company.Sdk;
  ClientBuilder.z42 → namespace Company.Sdk;
  Internal.z42     → namespace Company.Sdk.Internal;
  Testing.z42      → namespace Company.Sdk.Testing;

# 构建产物：
dist/my-sdk.zpkg  namespaces = ["Company.Sdk", "Company.Sdk.Internal", "Company.Sdk.Testing"]
```

命名空间解析规则：
- 编译器的依赖加载：读 zpkg 的 `namespaces` 列表，所有列出的命名空间均可见（无需在 `[dependencies]` 中逐一声明）
- VM 的 lazy loader：通过 `namespaces.iter().any(|n| n == requested_ns)` 判断 zpkg 是否提供某命名空间
- 同一命名空间不允许被两个不同 zpkg 同时提供（`AmbiguousNamespaceError`）

**`kind` 决定默认产物：**

| kind | 默认 emit | 说明 |
|------|-----------|------|
| `exe` | `zbc` | 单文件可执行字节码 |
| `lib` | `zbin` | 打包库（含所有模块）|

**多可执行目标（`[[exe]]`）：**

当一个工程需要产出多个可执行文件时，用 `[[exe]]` 数组表代替 `[project] kind = "exe"`：

```toml
[project]
version = "0.1.0"
# kind 省略 — 由 [[exe]] 推断

[sources]
include = ["src/**/*.z42"]   # 所有 exe 默认共享

[[exe]]
name  = "hello"              # 产物：dist/hello.zbc
entry = "Hello.main"

[[exe]]
name  = "tool"               # 产物：dist/tool.zbc
entry = "Tool.main"
src   = ["src/tool/**/*.z42"] # 可选：覆盖共享 sources
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✅ | exe 名，同时作为产物文件基名 |
| `entry` | string | ❌ 可选 | 完全限定入口函数；省略时走 `PackageCompiler` 的 `Main` 自动发现路径（2026-05-14 起）|
| `src` | string[] | ❌ | 独立 glob，覆盖 `[sources]`；省略则继承 `[sources]` |

```bash
z42c build               # 构建所有 [[exe]]
z42c build --exe hello   # 只构建名为 hello 的目标
```

> `[[exe]]` 与 `[project] kind = "exe"` 不能共存，二选一。

**两种编译模式（职责分离）：**

```bash
# 项目模式 — 读取 <name>.z42.toml，用于正式构建和交付
z42c build                      # 自动发现 *.z42.toml，profile.debug
z42c build --release            # profile.release
z42c build hello.z42.toml       # 显式指定工程文件

# 单文件模式 — 不读取任何 .z42.toml，用于快速编译和调试
z42c hello.z42                  # 编译单文件，默认 --emit ir
z42c hello.z42 --emit zbc       # 指定产物格式
z42c hello.z42 --dump-ast       # 调试：查看 AST
```

---

## L2 — 源文件配置

控制哪些 `.z42` 文件参与编译。

```toml
[project]
name    = "mylib"
version = "0.1.0"
kind    = "lib"

[sources]
include = ["src/**/*.z42"]           # glob，相对于 z42.toml
exclude = ["src/**/*_test.z42"]      # 排除测试文件
```

**字段说明：**

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `include` | string[] | `["src/**/*.z42"]` | glob 模式列表 |
| `exclude` | string[] | `[]` | 排除模式；优先于 include |

**Glob 语法（基于 `Microsoft.Extensions.FileSystemGlobbing.Matcher`）：**

| 模式 | 含义 |
|------|------|
| `*.z42` | 当前目录单层匹配所有 `.z42` 文件 |
| `**/*.z42` | 递归子目录（含当前目录）匹配所有 `.z42` 文件 |
| `src/**/*.z42` | 仅 `src/` 子树下递归匹配 |
| `src/**/foo.z42` | 子树中所有名为 `foo.z42` 的文件 |
| `src/lib/*.z42` | `src/lib/` 下单层匹配 |

**常见用法示例：**

```toml
# A) 默认 — 整个 src/ 树
[sources]
include = ["src/**/*.z42"]

# B) 多目录合并
[sources]
include = ["src/**/*.z42", "vendor/included/**/*.z42"]

# C) include + exclude 组合（exclude 优先）
[sources]
include = ["src/**/*.z42"]
exclude = ["src/internal/**", "src/**/_*.z42"]

# D) per-target 覆盖（[[exe]] 内的 src 字段独立 glob，无视顶层 [sources]）
[[exe]]
name  = "tool"
entry = "Tool.Main"
src   = ["src/tool/**/*.z42"]   # 仅这些文件参与 tool 编译
```

**不支持的语法**：

- 负 pattern（`!path/to/exclude`）—— 用 `exclude` 字段表达
- 大括号扩展（`{a,b,c}`）—— 写成多条 include
- 字符类（`[a-z]`）—— 用具体路径或 `*`

**默认 exclude 为空**：构建按 `include` 命中即编译。若需排除测试 / examples / 缓存目录，按需显式声明（项目级 `exclude` 字段）。

**目录布局约定（无 `[sources]` 时的默认行为）：**

```
my-app/
├── z42.toml
└── src/
    ├── main.z42      ← exe 默认入口文件
    └── lib.z42       ← lib 默认根文件
```

---

## L3 — 构建产物配置

控制产物格式、输出目录和增量编译。

```toml
[project]
name = "myapp"
version = "0.1.0"
kind = "exe"
entry = "MyApp.main"
pack = false           # 工程级 pack 默认值（最低优先级）

[build]
# restructure-publish-output-dirs (2026-06-19): 四件套字段（output_dir / cache_dir /
# dist_dir / publish_dir）都是可选的，未设走级联默认。
# output_dir  = "/build/myproj"      # 顶层根目录（workspace 默认 = artifacts/${project_name}/${profile}）
# cache_dir   = "/dev/shm/cache"     # 中间产物（默认 ${output_dir}/.cache）
# dist_dir    = "/build/dist"        # 最终产物（默认 ${output_dir}/dist）
# publish_dir = "/release/myproj"    # 发布分发目录（默认 ${output_dir}/publish）
incremental = true     # 启用增量编译，默认 true

[profile.debug]
pack  = false          # debug build → indexed zpkg（.cache/*.zbc + dist/<name>.zpkg）
strip = false          # 默认保留 DBUG 内嵌主 zpkg / cache zbc

[profile.release]
pack  = true           # release build → packed zpkg（单文件，dist/<name>.zpkg）
strip = true           # 默认剥离 DBUG → 配套 <name>.zsym sidecar
```

**`[build]` 字段说明（restructure-publish-output-dirs, 2026-06-19）：**

| 字段 | 类型 | 默认（单工程） | 默认（workspace member） | 说明 |
|------|------|------|------|------|
| `output_dir` | string? | `${workspace_dir}/artifacts/${profile}` | `${workspace_dir}/artifacts/${project_name}/${profile}` | 顶层输出根目录；`${output_dir}` 模板变量解析为此值。 |
| `cache_dir` | string? | `${output_dir}/.cache` | `${output_dir}/.cache` (+ member 子目录防碰撞) | 中间产物（`.zbc` / 索引 / 增量元数据）。 |
| `dist_dir` | string? | `${output_dir}/dist` | `${output_dir}/dist` | 最终分发产物（`.zpkg` + `.zsym`）。替代了 0.1.x 的 `out_dir`。 |
| `publish_dir` | string? | `${output_dir}/publish` | `${output_dir}/publish` | 发布分发目录。`z42c build`（exe）和 `z42c publish` 将产物 + 非 stdlib 依赖复制到此目录。lib 默认不复制（需显式 `z42c publish`）。 |
| `incremental` | bool | `true` | `true` | 基于 source hash 跳过未改动文件。 |
| `hooks` | string? | （无） | （无） | **项目 build hook 源目录**（projDir 相对；wire-z42b-host-build 阶段 7）。声明后 z42b 用注入的同一 `ICompiler` 编该目录 → 动态实例化 `Build.ProjectHooks : BuildHooks` → 注入 `Pipeline.Hooks`。hook 源须 `namespace Build;` + `class ProjectHooks : BuildHooks`。**z42c 不消费此键**（仅 z42b 编排读），与 `[platform.*]` 同为编排/发布侧配置。用途见 [build-orchestrator.md](../toolchain/build-orchestrator.md#自定义扩展manifest-声明-hook-目录)（含 `z42 publish` 经 hook 免装 workload 产 apphost）。 |

**模板变量（`${...}`）**：

| 变量 | 解析为 |
|------|------|
| `${workspace_dir}` | workspace 根目录（单工程时 = toml 所在目录） |
| `${member_dir}` | member toml 所在目录 |
| `${member_name}` / `${project_name}` | member 的 `[project].name`（两者等价，推荐 `${project_name}`） |
| `${profile}` | 当前 build profile（`debug` / `release`） |
| `${output_dir}` | 经展开后的 `output_dir` 绝对路径 |

**三字段级联示例：**

```toml
# A) 全部不设 → 全部默认
[build]
# output_dir = toml 所在目录；cache = ./.cache；dist = ./dist

# B) 只设顶层 → cache / dist 跟随
[build]
output_dir = "/build/myproj"
# → cache = /build/myproj/.cache; dist = /build/myproj/dist

# C) 单独把 cache 放 RAM disk
[build]
output_dir = "/build/myproj"
cache_dir  = "/dev/shm/myproj-cache"
# → dist 仍然 = /build/myproj/dist；cache 解耦

# D) 三字段都显式 → 三者独立
[build]
output_dir = "/a"
cache_dir  = "/b"
dist_dir   = "/c"
```

**workspace member 继承规则**：member `[build]` 的任一字段 unset → 继承 workspace `[workspace.build]` 的对应字段；workspace 字段 unset → 走全局默认。Workspace 模式下 `cache_dir` 模板若不含 `${member_name}` 或 `${project_name}`，会自动追加 member 子目录，避免不同 member 缓存碰撞。

**`z42c build` / `z42c publish` 行为**：

| 命令 | lib | exe |
|------|-----|-----|
| `z42c build` | 编译到 `dist_dir`，**不**复制到 `publish_dir` | 编译到 `dist_dir`，自动复制产物 + 非 stdlib 依赖到 `publish_dir` |
| `z42c build --no-publish` | 同上（不复制） | 只编译到 `dist_dir`，跳过 publish 步骤 |
| `z42c publish` | 编译 + 复制产物到 `publish_dir` | 编译 + 复制产物 + 非 stdlib 依赖到 `publish_dir` |

**从 0.1.x `out_dir` 迁移**：直接把 `out_dir = "..."` 改成 `dist_dir = "..."`；若值就是 `"dist"`（默认），整行可以删除。pre-1.0 不留 alias —— 老字段触发 WS008 警告 + Levenshtein 建议 `dist_dir`。

**`pack` 字段说明（三层优先级，高→低）：**

| 层次 | 位置 | 说明 |
|------|------|------|
| 最高 | `[profile.debug/release].pack` | 当前 profile 覆盖 |
| 中 | `[[exe]].pack` | 单个可执行目标覆盖 |
| 最低 | `[project].pack` | 工程级全局默认 |

**内置默认值（未显式配置时）：** `debug` → `false`（indexed），`release` → `true`（packed）

**`strip` 字段说明（2026-05-10 split-debug-symbols）：**

控制 DBUG section 的产出位置。strip=true 时主 `<name>.zpkg` 不含 DBUG body，配套产出 `<name>.zsym` sidecar（zpkg 0.4 `SymOnly` flag，含 MDBG + BLID）。runtime 加载主 zpkg 后自动探测同目录 sidecar 并按 build_id 配对合并，缺失或不匹配时静默退化（trace 维持函数名 + 签名）。

| 层次 | 位置 | 说明 |
|------|------|------|
| 最高 | `--strip-symbols=true\|false` | CLI override |
| 中 | `[profile.debug/release].strip` | toml profile 覆盖 |
| 最低 | 内置默认 | `debug` → `false`，`release` → `true` |

**产物输出：**

| pack 值 | strip 值 | 产物 | 说明 |
|---------|---------|------|------|
| `false` | `false` | `dist/<name>.zpkg`（indexed 主文件）+ `dist/<rel>.zbc` 散装 + `.cache/` | 开发态（debug 默认），DBUG 内嵌散装 zbc；未变文件 zbc 字节稳定 → 最小 patch（add-indexed-zpkg-min-patch，zpkg 0.24 实装）|
| `false` | `true`  | ——（构建报错）| indexed 为开发态，与 `--release` strip 不兼容 |
| `true`  | `false` | `dist/<name>.zpkg` (packed)                  | 发布态，DBUG 内嵌（便于现场 debug）|
| `true`  | `true`  | `dist/<name>.zpkg` + `dist/<name>.zsym`      | 发布态，最小体积，离线可符号化 |

> **z42c 实现现状（2026-07-08）**：`pack` 三层中 `[project].pack` + 内置默认已生效；
> `[profile.*].pack` / `[[exe]].pack` 随 profiles 解析延后线（z42c 尚未解析 profile 段）。
> z42c 的 strip ≡ `--release`（无独立 `--strip-symbols` flag）。

**增量编译工作方式（C5 2026-04-27 引入整包版；文件级 add-file-level-incremental，2026-07-08）：**

判定与组装 SoT = **cache**（`<cache>/<rel>.zbc` fullMode + 同名 `.meta`），不再读上次 zpkg
MODS。粒度是**文件级**：只重编「变化文件 + 包内传递依赖方」，其余文件的 IrModule 从
cache zbc 读回（ZbcReader）。与 C# 当年被放弃的混合重建的本质区别：**无跨代元数据合并**
——TSIG/符号每次由当前源 AST 全包重算（每文件 TSIG 天然全包耦合：自由函数兄弟泄漏 +
全包 AST 依赖），zbc 来自 hash 校验一致的 cache，两来源不一致的根因被结构性消除。

```
z42c build <toml> [--release] [--no-incremental]      （单工程模式；workspace/flat 不走）
  ├─ IncrementalBuild.ProbeFiles（z42c.pipeline）
  │    ├─ 种子：源 hash != meta / 条目缺失·版本 pin 不符 / 包级源清单不一致（增删文件→全量）
  │    └─ 全命中 ∧ dist zpkg 在盘 → 完全跳过（preserved；exe 仍复制非 stdlib 依赖）
  ├─ IrDump.ParseAll（全包 parse，恒做——TSIG/符号全包耦合）
  ├─ IncrementalBuild.Close：token 保守边闭包（文件 i 标识符 token ∩ 文件 j 包内定义名
  │    （类型+自由函数+成员名）→ 边 i→j；j fresh → i fresh；边每次从当前源重算）
  ├─ cached 读回：ZbcReader.Read(cache zbc) + meta 残留回填（块 label 原文改名（含终结符/
  │    异常表引用）、模块池原序、TIDX idx——wire 不携带的 writer 残留，见 D5a）；失败→降级 fresh
  ├─ IrDump.BuildPackageCus：仅失效子集跑 typecheck/codegen；TSIG 全包重算
  ├─ fresh 落 cache（zbc + meta）+ 包级源清单
  └─ BuildPackedD(全部 IrModule 同构组装) → dist/<name>.zpkg（任一变更即整包重写）
```

**编译日志输出**：`cached: N/M files`（stderr）；全命中时 `no changes; preserved -> <zpkg>`。
`--no-incremental` 强制全量。**硬验收 = 暴力对账器 `xtask test incremental`**：语料逐文件
touch，断言增量产物与全量产物**逐字节相等** + D8 计时（增量 vs 全量墙钟）。

**cache 条目格式**：`<rel>.zbc`（fullMode，与 `--emit-zbc` 同一 `ZbcWriter.Write` 产出）+
`<rel>.meta`（z42c 内部行式文本，带 metaVersion/zbc/zpkg 三重版本 pin：源 hash、ns、
usedDepNs、模块池原序（hex）、每函数块 label 表（hex）——后两者是 zbc wire 不携带、但参与
STRS 字节的 writer 残留）+ 包级 `package.meta`（上次源清单）。任何 pin 不符/损坏 → 条目作废
按 fresh 处理（宁 fresh 不误命中）。cache 可整目录删除。
indexed 模式（stripped zbc = `<dist>/<rel>.zbc`）自举重写未实现，见
[self-hosting.md Deferred](self-hosting.md#self-hosting-future-indexed-zpkg)；散装自包含
zbc 的最小 patch 分发方向见 `docs/spec/changes/add-indexed-zpkg-min-patch/`（DRAFT）。

**调试**：`Z42_INCR_DEBUG=1` 打印失效种子原因（no-entry / hash-diff / src-list）与传播链
（`A invalidated-by B`）。

### incremental-future-tsig-level-invalidation

- **来源**：add-file-level-incremental design D3（2026-07-08）
- **触发原因**：第一版失效边取保守粒度（token ∩ 定义名；被引用文件任何变化即失效引用方），
  过近似只多编不错编；「B 的导出签名（TSIG）不变则不失效 A」的细化需 TSIG 结构化 diff
- **前置依赖**：per-file TSIG 规范化比较（剥全包自由函数泄漏段）
- **触发条件**：实测大包增量命中率低 / 对账器计时显示闭包过宽成为主要成本
- **当前 workaround**：无需——保守边正确性优先

### incremental-future-workspace-wiring

- **来源**：port-incremental-build-cache（2026-07-05，User 裁决移出该 change）
- **触发原因**：workspace / flat 构建（`--workspace` / 显式 `--output-dir`）经
  `outputDirOverride` 传 dist，`WsPlan` 不携带 cache 目录 → 该路径暂不落 cache、不 probe。
- **前置依赖**：`WorkspaceBuild.PlanLayout` 展开 `[workspace.build].cache_dir` 模板并随
  `WsPlan` 传入 `_build`；xtask gen 系脚本（自举 gen1/gen2 字节对比）显式 `--no-incremental`
  ——否则 gen2 全命中跳过会使字节对比空洞化。
- **触发条件**：stdlib 22 包 / z42c 7 包 warm 重建成为迭代瓶颈时（整包跳过在 workspace
  构建收益最大）。
- **当前 workaround**：workspace 构建永远全量（与本 change 前行为逐字节一致）。

**目录结构（含产物，单工程默认）：**

```
my-app/
├── z42.toml
├── src/
│   └── main.z42
└── artifacts/
    └── debug/              ← output_dir（默认 artifacts/${profile}）
        ├── dist/           ← dist_dir（默认 ${output_dir}/dist；加入 .gitignore）
        │   └── my-app.zpkg ← indexed 或 packed，取决于 pack 设置
        ├── .cache/         ← cache_dir（默认 ${output_dir}/.cache；加入 .gitignore）
        │   └── src/main.zbc ← 仅 pack=false 时生成
        └── publish/        ← publish_dir（exe 时 z42c build 自动填充）
            └── my-app.zpkg
```

---

## 跨包类型签名：从 TYPE/SIGS/IMPL 重建（unify-type-metadata P2→P3，2026-07-11）

> 背景：zpkg 曾并存两份类型元数据——**运行时段**（`TYPE` ClassDesc + `SIGS` FuncSig，VM 执行 +
> 反射读）与**编译期段**（`TSIG` ExportedModuleZ + `IMPL`，z42c 跨包解析读）。unify-type-metadata
> P1 把 TSIG 独有的字段（可见性 / virtual·abstract / min_arg / 参数名·默认值 / enum 值 / delegate /
> impl）全部补进 TYPE/SIGS/IMPL。**P2** 建 `TsigReconcile.Rebuild`（从 TYPE/SIGS/IMPL 重建
> ExportedModuleZ）+ 对账器与 TSIG oracle 逐字段验证 29 包 0-DIFF（安全网）。**P3（已落地
> 2026-07-11，zpkg 0.31）删 TSIG + EXPT 两段**：`DepScan` 跨包解析改走 `Rebuild`，`Rebuild` 是
> 跨包类型签名的**唯一来源**；对账器 + verb + `ReadTsig` 随 TSIG 一并移除。**每 zpkg ~半：
> z42.core 193KB→92KB。**

**机制**（`src/libraries/z42.ir/src/TsigReconcile.z42`，driver verb `z42c reconcile-tsig <zpkg>...`；converge-z42c-ir-metadata：随 zpkg 后端下沉 z42.ir）：

```
world = 全部待对账 zpkg（跨包 base 链：imported 祖先字段/方法从 dep 包 TYPE/SIGS 取）
每包：
  oracle   = ZpkgReader.ReadTsig(z)          # 现有 TSIG 段（当基准）
  rebuilt  = Rebuild(z, world)               # 从 TYPE/SIGS/IMPL 重建 ExportedModuleZ
  Compare(oracle, rebuilt)                    # 逐字段（归一化后）assert；空差异 = OK
```

**Rebuild 的口径**（镜像 `ExportedTypeExtractor`——TSIG 的生产端）：

- **类**：TYPE 每条（跳 interface bit4 / delegate bit6 / enum bit5 / 隐式根 `Std.Object`）→ 裸名
  （剥 ns + 尾部 arity-mangle `$N`）；base 链 topmost-first 展平字段；方法 = Object 四方法（非
  struct）+ 链实例方法合并（override 替换祖先位、`IsVirtual:=false`）+ 本类 static append。
  祖先 SIGS 用 **world 全包 (pkg, mod) 精确定位**（非 ns 首中——同 ns 多模块会错配）。
- **enum**：TSIG 恒 = 内建 `GCHandleType{Weak,Strong}`（本地 enum 不进 TSIG）。
- **自由函数**：全包 SIGS 中 ns 剥离后**不含 `.`** 的条目（类方法为 `<Class$N>.<m>`——用 `_stripNs`
  保 `.` 判别，勿用剥 `$N` 的 `_bare` 否则 `Foo$1.Bar` 误判为自由函数）；排除 `__static_init__` /
  `__lambda` / `__local_` / `[Native]`。

**归一化**（TYPE/SIGS 与 TSIG 是**不同编码**，语义等价即通过——非字节等价）：

| 维度 | TSIG（oracle） | TYPE/SIGS（rebuilt） | 归一 |
|------|---------------|---------------------|------|
| 可见性 | `"internal"`（SymbolCollector 无修饰默认）| u8 `0=public` | internal ≈ public（无消费方按 internal 门禁）|
| 类型拼写 | 解析短名 `Type` / `int` | 源拼写 `Std.Type` / `i32`，点缀名 → `"unknown"` | 短名 + prim 别名（byte↔u8…）+ nullable/泛型实参擦除 + `unknown ≈ 任一非基本类型` |
| 方法集 | getter-only（**漏报属性 `private set`**）| getter + setter 齐全 | **子集语义**：oracle ⊆ rebuilt（按 name+isStatic+paramCount 匹配）|

**关键发现**：**TSIG 是有损的**——它漏报属性 `private set`，SIGS 更全。故子集语义（oracle ⊆
rebuilt）是「无信息丢失」安全网的**正确判据**：rebuilt 多出的方法是**信息增益**，非重建 bug。
⟹ **P3 删 TSIG 零信息损失、反而更完整。**

**对账 gate 揪出的 SIGS 源码精度缺陷**（`IrGen` 根因修，P3「SIGS 作唯一真相」的前置）：native
桩返回/参数类型原硬编码 `"object"` → 改真实声明类型；property/indexer/隐式 ctor 合成点补逻辑
`MinArg`；auto-prop 后备字段可见性 public → private。

**验证**：P2 期全 29 包 `reconcile-tsig` 全 OK；P3 切换后 `Rebuild` 是编译热路径，每次跨包编译都
经它，自举不动点 + GREEN 天然覆盖。**IMPL 段保留**（跨包 impl Trait for Type 关联，`Rebuild`
经 `ReadImplInto` 读进 `Impls`——跨包 impl 方法传播/反射靠它）。

---

## L4 — 运行时 Profile

区分开发和发布构建，覆盖执行模式和优化级别。

```toml
[project]
name  = "myapp"
version = "0.1.0"
kind  = "exe"
entry = "MyApp.main"

[build]
mode = "interp"        # 全局默认执行模式

[profile.debug]        # z42c build（默认）
mode     = "interp"
optimize = 0
debug    = true        # 保留调试信息（zbc META section）

[profile.release]      # z42c build --release
mode     = "jit"
optimize = 3
strip    = true        # 剥除 META section
```

**Profile 字段说明：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `mode` | `interp\|jit\|aot` | 执行模式，覆盖 `[build].mode` |
| `optimize` | 0–3 | 优化级别（0=无，3=最激进）|
| `debug` | bool | 生成调试信息，默认 debug=true / release=false |
| `strip` | bool | 剥除调试信息，默认 false |
| `pack` | bool | 覆盖 `[[exe]]` 和 `[project].pack`；null = 不覆盖 |

**执行模式三级优先级（高→低）：**

```
源码注解 @interp / @jit / @aot   ← 最高，作用于单个命名空间
  ↓
profile 中的 mode 字段
  ↓
[build].mode 全局默认             ← 最低
```

**构建命令：**

```bash
z42c build              # 使用 profile.debug
z42c build --release    # 使用 profile.release
z42c build --profile staging   # 使用自定义 profile（如有）
```

---

## L5 — 依赖管理

声明项目依赖的外部 `.zpkg` 库。

```toml
[project]
name    = "myapp"
version = "0.1.0"
kind    = "exe"
entry   = "MyApp.main"

[dependencies]
"my-utils" = "*"         # 在 libs/ 中找 name="my-utils" 的 .zpkg
"my-http"  = "*"         # 版本约束目前只做存在性校验，不做 semver 比较
```

**设计原则：命名空间与包名解耦**

`[dependencies]` 中填写的是 **zpkg 的 `[project] name` 字段**，而非命名空间名称。编译器在 libs/ 搜索路径中找到对应 zpkg 后，读取其 `namespaces` 字段，将导出的命名空间注册为可用。

```
[dependencies] "my-http" = "*"
  → 编译器在 libs/ 找 name="my-http" 的 .zpkg
  → 读该 zpkg 的 namespaces: ["Http", "Http.Client"]
  → 源码中 using Http; / using Http.Client; 均可解析
```

这意味着 `using` 语句中的命名空间名称与 `[dependencies]` 中的包名**无需一致**，由 zpkg 自身的 manifest 决定。

**stdlib 自动可用，永不声明（Rust-std 模型，simplify-stdlib-auto-import 2026-06-06）：**

标准库（`Std.*` 命名空间 / `z42.*` 包）跟工具链一起分发，**始终可用，无需在任何 manifest section 声明**——就像 Rust 从不在 `Cargo.toml` 写 `std`，`use std::...` 直接可用。机制：编译器对 `meta.Name` 以 `z42.` 开头的包**无条件可见**（`ScanLibsForNamespaces` / `BuildDepIndex` 的 isStdlib 旁路），与是否声明无关；版本跟工具链走。

由此确立的约定：

- **`[dependencies]` / `[tests.dependencies]` / `[bench.dependencies]` 只用于第三方依赖。** stdlib（`z42.*`）出现在其中纯属冗余。
- **WS013 lint（warning）**：非 `z42.*` 项目在任一 deps section 声明了 `z42.*` 包 → 警告「冗余，可删」。（`z42.*` 包**自身**声明 `z42.*` inter-dep 不警告——那是 workspace build 排序需要的，见下「workspace member 构建顺序」。）
- **`Std.*` 命名空间保留（E0605，硬错误）**：非 `z42.*` 包在源码声明 `namespace Std.*`（或裸 `Std`）→ **编译错误**。`Std` / `Std.*` 专属官方 stdlib（同 Rust 保留 `std`/`core`/`alloc`），保证程序里任何 `Std.*` 一定解析到官方、自动可用的 stdlib，永不被第三方 shadow。（消费一个已构建的、占用 `Std.*` 的第三方 zpkg 时另有 W0603 warning 作软网。）

> 历史背景：早期约定 / 示例鼓励「用啥 stdlib 声明啥」，但编译器本就自动加载，声明从无作用。本次把隐式机制正式化为「显式约定 + lint 守护」。stdlib 各包 manifest 自身曾带的 `[tests.dependencies] "z42.test"` 也一并清掉（z42.test 是 stdlib，自动可用）。

**有 `[dependencies]` vs 无 `[dependencies]`：**

| 情况 | 编译器行为 |
|------|-----------|
| 有 `[dependencies]` | 只扫描声明的包，libs/ 中其他 zpkg 不参与 `using` 解析 |
| 无 `[dependencies]`（脚本/单文件模式）| 自动扫描 libs/ 全部 zpkg 和 Z42_PATH 全部 zbc |

**依赖解析后的编译器行为：**

- TypeChecker：可见依赖库导出的类型和函数签名
- IR Codegen：生成跨库调用的 `call` 指令（含模块引用）
- 输出 zpkg 的 `dependencies` 字段：记录编译期实际解析到的文件名和命名空间（不是 `[dependencies]` 的原样复制）

**版本管理：** 目前不做 semver 比较，也不生成 lockfile。用户通过控制 libs/ 中实际存放的文件版本来锁定依赖。

**完整示例（含依赖）：**

```toml
[project]
name    = "hello"
version = "0.1.0"
kind    = "exe"
entry   = "Hello.main"

[sources]
include = ["src/**/*.z42"]

[build]
# Cascade defaults — dist_dir = ./dist, cache_dir = ./.cache.
# Override only when needed:
#   dist_dir = "/build/hello"
#   cache_dir = "/dev/shm/hello-cache"

[dependencies]
"my-utils" = "*"

[profile.debug]
mode  = "interp"
debug = true

[profile.release]
mode     = "jit"
optimize = 3
strip    = true
```

### 依赖必须无环（No Circular Dependencies）

**硬规则**：z42 包之间的依赖关系**必须形成有向无环图（DAG）**。任何包不得直接或间接依赖自身。该约束在所有依赖层级一致生效，没有运行时延迟 import 等后门。

**适用范围与强制状态**：

| 层级 | 约束 | 当前强制 |
|------|------|----------|
| zpkg `[dependencies]` 之间 | A 依赖 B → B 不得（直接或传递）依赖 A | 🔄 编译期解析时检测（错误码待 RFC，建议 `E0610 CircularPackageDependency`） |
| Workspace member 之间 | 同上，DFS 三色检测 | ✅ `WS006 CircularDependency`（见 [error-codes.md](error-codes.md)） |
| Preset `include` 链 | 同上 | ✅ `WS020 CircularInclude` |
| stdlib 层级 | `L0 ← L1 ← L2 ← L3`，下层不得依赖上层 | ✅ 设计规则（见 [stdlib-organization.md](../stdlib/organization.md)） |

**为什么禁止循环依赖**：

1. **构建顺序确定**：DAG 给出唯一拓扑序，编译器、增量构建、链接器能并行/缓存；环会要求"同时编译两个包"或人工切环
2. **初始化语义清晰**：循环依赖会把"模块初始化顺序"暴露成运行时竞态；DAG 保证下层先初始化
3. **工具链可推理**：类型解析、增量编译、IDE 跳转、文档生成在 DAG 下都是良定义问题
4. **大型工程已被验证**：Rust crate、Go module、.NET assembly、Java JPMS module、Swift module 一律强制 DAG —— z42 沿用同一边界

**破环手法（推荐）**：

当两个包"看起来"必须互引时，几乎总能用以下手法之一拆开：

- **下沉公共抽象**：把双方共享的接口/协议下沉到更底层的包，双方都依赖它（典型：`z42.core` 沉淀 `IComparable` / `IEquatable` / `IComparer`，让 `z42.collections` 的 `Sort` 与 `z42.core` 的 `List` 都能用）
- **接口反转**：抽象在下层定义，实现在上层；下层无需感知具体实现（Rust trait 跨 crate 实现、Go "接口归消费者所有" 都是这个套路）
- **跨包扩展**（cross-zpkg `impl Trait for Type`，L3-Impl2 已支持）：在外部包给已有类型挂方法，不必把方法塞回类型所在包
- **再导出（re-export）**：用户使用路径扁平化，但物理依赖图保持 DAG（参考 Rust `pub use`、.NET `[TypeForwardedTo]`）
- **拆 domain 后合或分**：发现 A、B 互相依赖，往往说明它们是同一概念被错拆（→ 合并），或可以抽出共享部分 C 让 A → C ← B

**禁止手法（反模式）**：

- ❌ 运行时延迟 import / 函数体内 import（Python 风格） —— z42 不提供此后门
- ❌ "源码引用"打洞（Haskell `{-# SOURCE #-}` 风格） —— z42 不引入此机制
- ❌ 新旧 zpkg 共存 + 灰度迁移以"绕开"循环 —— pre-1.0 不留兼容（见 [philosophy.md "不为旧版本提供兼容"](../../.claude/rules/philosophy.md#不为旧版本提供兼容2026-04-26-强化)）

**编译器报错要求**（待实现时遵守）：

- 在依赖闭包解析阶段构建依赖图，发现环立即终止，**不进入 TypeChecker**
- 错误信息**必须列出完整环路**（如 `A → B → C → A`），并标注每条边来自哪个包的 `[dependencies]` 字段
- 同一构建中存在多条独立环 → 各报一次，不合并

> **同包内不受此规则约束**：单个 zpkg 内部的文件 / 类型 / 函数互相引用是允许的（同 Rust crate 内 module、Java 同 package、Swift 同 module）。本规则的最小切割单位是 **zpkg（分发单元）**。

---

## L5b — 测试 / Bench / Example 目标配置（add-tests-bench-manifest-config, 2026-07-29 落地）

> **本节 2026-07-29 重写**（原 2026-06-06 纸面设计从未实现）。落地形态与旧稿的四处差异见节末
> 「与 2026-06-06 旧稿的差异」。设计参照 Cargo target 模型（`[[test]]`/`[[bench]]`/`[[example]]`
> + auto-discovery），按 z42 自举子集精简。

声明 test / bench / **example** 运行目标的位置、驱动方式、共享依赖、产物布局。设计原则：**约定优先
（glob 批量发现）+ 显式覆盖（`[[target]]` 具名）+ dev-deps 隔离**。三类目标结构同构，共用模型
（`RunTarget` / `TargetSection`，见 `src/libraries/z42.project/`）。

### 两层模型（批量 vs 逐个）

- **段配置**（`[tests]`/`[benches]`/`[examples]`）= 约定扫描：一条 `include` glob 把匹配到的每个
  文件/目录变成一个**自动具名**目标（名从路径推导），零配置即批量覆盖。
- **显式目标**（`[[test]]`/`[[bench]]`/`[[example]]`）= 具名声明：仅当需要多文件合并 / 自定义 entry /
  独享 dep / `harness` 覆盖时才写；同名时**覆盖**约定扫出的 auto 目标。

### 驱动方式：`harness` 布尔（借 Cargo）

| `harness` | 谁驱动 | 判定 |
|-----------|--------|------|
| `true`（默认）| z42b 反射跑本单元的 `[Test]` / `[Benchmark]` 自由函数 | assert 失败 / 非零退出 |
| `false` | 跑目标 `entry`（FQ 函数名）指定的 Main | **退出码**（非零即失败；**无 golden / expected 比对**）|

> **golden（`expected_output.txt` stdout 比对）不进本模型**——它由 `src/tests/**` 的独立约定
> harness（`scripts/test/xtask_test_vm.z42`）承载，与清单目标正交并存。清单目标一律 exit-code 语义。
> example 恒 Main 程序，`harness` 对其无意义（发现方一律按 `entry` Main 跑）。

### 约定（自动发现）

```
<package>/
├── z42.toml
├── src/                              ← 产品代码
├── tests/
│   ├── foo_basic.z42                 ← 单文件测试（独立编译单元）
│   ├── bar_errors.z42                ← 同上
│   └── integration_roundtrip/        ← 多文件测试（dir-mode）
│       ├── source.z42                ← 入口（约定名，不可改）
│       ├── _helpers.z42              ← 同 dir 内任意 *.z42 递归 include
│       └── data/                     ← 非 .z42 数据文件；运行时相对路径读取
├── bench/                            ← 与 tests/ 同构
│   └── lexer_throughput.z42
└── examples/                          ← 与 tests/ 同构（默认只编不跑，见下）
    └── hello.z42
```

**约定规则**：

1. `tests/*.z42` 顶层文件 → 各自独立目标（auto 名 = 文件 stem）
2. `tests/<name>/source.z42` 入口 + 同目录递归 `*.z42` → 合成一个多文件目标（auto 名 = 目录名）
3. `bench/*` 与 `examples/*` → 同 1/2 规则（默认发现 dir 分别为 `bench/` `examples/`）
4. 子目录内非 `.z42` 文件（fixture / data）随产物打包，运行时 cwd 切到 `<dir>`，相对路径读取
5. `_` 前缀的 `.z42` 文件是 dir-mode 内的辅助；不是目标入口
6. **发现循环必须先按稳定键 sort** 再注册（[common-pitfalls §1](../../../.claude/rules/common-pitfalls.md)——
   first-wins 禁止依赖 FS 枚举序）

### `[tests]` / `[benches]` / `[examples]` 段

共享配置 + dev-deps 隔离（Cargo `[dev-dependencies]` 等价物）。**段名一律复数**——与单数
`[[test]]`/`[[bench]]`/`[[example]]` array-of-tables key 区分开（同名 key 既是 table 又是 AoT 是
非法 TOML）。

```toml
[tests]
# 字段全可省 → 走约定：tests/*.z42 + tests/*/source.z42
# include = ["tests/*.z42", "tests/*/source.z42"]
# exclude = ["tests/_skip/*"]
# auto    = true          # false → 关闭约定扫描，只认 [[test]]
[tests.dependencies]
"z42.test" = "0.1.0"      # 仅测试合入；release zpkg 元数据不含

[benches]
[benches.dependencies]
"z42.test" = "0.1.0"      # Bencher 在 z42.test 包内

[examples]
# 默认发现 examples/*.z42 + examples/*/source.z42
```

### `[[test]]` / `[[bench]]` / `[[example]]` 数组（显式覆盖）

字段对齐 `[[exe]]`（`entry` = FQ 函数名，`sources` = glob 集）：

```toml
[[test]]
name    = "compile_perf"          # 必填；filter 用 + 合成包名
harness = false                   # 默认 true（反射）；false → 自带 Main 退出码判定
entry   = "Perf.Runner.Main"      # harness=false 必填（FQ 函数名）
sources = ["tests/perf/*.z42", "tests/perf/_lib/*.z42"]   # 可选；省略=沿用约定单元文件集
[test.dependencies]               # 该 target 独享 dev-dep（三层合并优先级最高）
"z42.compression" = "0.1.0"

[[example]]
name = "streaming"
entry = "Ex.Streaming.Main"
test = true                       # 破例纳入 xtask test 执行（默认 example 只编不跑）
```

### example 的执行语义（借 Cargo）

- `xtask test`：**编译**所有 example 当门禁（确保永远编得过），**默认不执行**。
- `xtask example <name>`：显式编译并运行单个 example（退出码判定）。
- 目标写 `test = true` → 纳入 `xtask test` 执行（编 + 跑）。

### 具名选择运行

`xtask test targets <name>` / `xtask bench targets <name>` / `xtask example <name>` 只跑一个
（裸 `test`/`bench` 是全量 gate / e2e 默认动作，故 test/bench 走 `targets <name>` 子动作）。名不存在
→ 报错列出可用目标名，非零退出（不静默）。**注**：自定义段 `include` glob 运行期暂只扫约定目录
（`tests/`·`bench/`·`examples/`），见归档 spec「Known Limitations」。

### 三层依赖合并

编译目标时合并三层依赖：

```
final_deps = [dependencies]
          ∪ [<plural>.dependencies]      (test→[tests]，bench→[benches]，example→[examples])
          ∪ [[target]].dependencies      (若该目标声明了独享 dep)
```

冲突解决优先级：`[[target]]` > `[<plural>]` > `[dependencies]`（精确覆盖广泛）。

**Release 产物**：`xtask build`（非 test 路径）忽略所有 `[tests]`/`[benches]`/`[examples]`/
`[[test]]`/`[[bench]]`/`[[example]]` 字段；release zpkg 元数据只含 `[dependencies]`。

### 编译产物布局

测试 / bench 产物在每个 package 的 `output_dir` 下并列两个独立子树，与 L3 [build] 的 `output_dir` / `cache_dir` / `dist_dir` 三字段模型对齐（见 [restructure-build-output-dirs](../../spec/archive/2026-06-06-restructure-build-output-dirs/)）：

```
artifacts/build/libraries/<lib>/<profile>/
├── cache/                          ← 生产中间产物
├── dist/<lib>.zpkg                 ← 生产分发产物（只放生产 zpkg）
├── tests/                          ← 测试子树
│   ├── cache/<test_name>/          ← 每测试独立 cache
│   └── dist/                       ← 测试可执行
│       ├── <lib>.test.<name>.zbc               ← 单文件（emit-zbc 路径；runner 直接吃 .zbc）
│       └── <lib>.test.<dir_name>.zpkg          ← dir-mode（合成 manifest → z42c build → packed zpkg）
└── bench/                          ← bench 子树（与 tests 同构）
    ├── cache/<bench_name>/
    └── dist/
        ├── <lib>.bench.<name>.zbc              ← 单文件
        └── <lib>.bench.<dir_name>.zpkg         ← dir-mode
```

> 单文件单元走轻量 `z42c --emit zbc` 产 `.zbc`；dir-mode 单元(多文件)合成 mini-manifest 跑 `z42c build` 产 packed `.zpkg`。两者都由 z42b（z42.builder.zpkg）经 TIDX 发现 + 调度，落同一 `<subtree>/dist/`。统一单文件也产 `.zpkg` 是后续可选 polish（runner 无所谓）。

**zpkg 命名硬约束**：`.test.` / `.bench.` infix 是文件名硬规则（也是 CI 守门正则的 anchor）。`tests_dir` / `bench_dir` 字段**不暴露** — 强制 `<output_dir>/tests/` 和 `<output_dir>/bench/`；改路径走 `output_dir`，两子树一并变。

### xtask 命令 ↔ 目录（Phase 3.2 / 3.4，2026-06-07）

| 命令 | 写入 | 读取 deps |
|------|------|----------|
| `./xtask test stdlib [lib]`  | `<lib>/<profile>/tests/{cache/<unit>,dist}/` | `[dependencies]` + `[tests.dependencies]` |
| `./xtask bench stdlib [lib]` | `<lib>/<profile>/bench/{cache/<unit>,dist}/` | `[dependencies]` + `[benches.dependencies]` |
| `./xtask test targets <name>` / `bench targets <name>` / `example <name>` | 同上（具名单目标）| 三层合并 |
| `./xtask clean`              | 删每个 `<lib>/<profile>/{cache,dist}` + 聚合 `libraries/dist/`（**保留** tests/bench） | — |
| `./xtask clean tests`        | 删每个 `<lib>/<profile>/tests/` | — |
| `./xtask clean bench`        | 删每个 `<lib>/<profile>/bench/` | — |
| `./xtask clean all`          | 删整个 `artifacts/build/`（全量重置） | — |

`bench`（无 `stdlib` 子参）仍是 e2e hyperfine 场景跑器，与 per-lib micro-bench 分流。[Benchmark] 单元由 z42b 与 [Test] 同调度（zero-arg 调用 + Bencher 采样）。

### 错误码

| 码 | 严重度 | 触发 |
|---|:---:|------|
| WS012 | warning | test-only dep 出现在 `[dependencies]`（leak 提示）|
| WS040 | error | `[[test]]` / `[[bench]]` / `[[example]]` 缺 `name` |
| WS041 | error | `harness = false` 的目标缺 `entry`（反射目标 harness=true 无需 entry）|
| WS042 | error | 同一 kind 内 name 重复（含 auto 与显式撞名以显式为准，不报错）|
| WS043 | error | 目标 `sources` glob 无匹配文件 |

> 校验在 **xtask 发现层**做（`ManifestLoader` 只忠实解析、不校验语义）。
> `KnownTestOnlyDeps` 当前为 `{ "z42.test" }`，curated set，不靠启发式。三类 kind 命名 namespace 独立
> —— `[[test]] name = "x"` / `[[bench]] name = "x"` / `[[example]] name = "x"` 可共存。

**WS012 例外**：`[project].name` 含 `.test.` 或 `.bench.` infix 时抑制。xtask dir-mode 生成的 synthetic mini-manifest（`<lib>.test.<unit>` / `<lib>.bench.<unit>`）合法在 `[dependencies]` 写 z42.test —— harness 项目本质是测试程序，不存在 leak。用户 zpkg 命名应避免该 infix。

### 与 2026-06-06 旧稿的差异（落地时修订）

| 维度 | 旧稿（纸面，未实现）| 落地（2026-07-29）|
|------|------|------|
| 目标入口字段 | `src`（单入口**文件**，必填）| `entry`（FQ **函数**名）+ `sources[]`（glob），对齐 `[[exe]]` |
| 驱动方式 | 无区分（隐含 Main + golden）| `harness` 布尔（true=z42b 反射 / false=Main 退出码）|
| 验证 | dir-mode 隐含 golden 比对 | harness=false 一律**退出码**；golden 归 `xtask_test_vm` 独立 harness |
| example | 「future iteration」无配置 | **一等目标**：默认只编不跑，`test=true` 才跑，`xtask example <name>` 显式跑 |
| 段名 | `[bench]`（与 `[[bench]]` 撞 key，非法 TOML）| 复数 `[benches]`/`[examples]`（避撞）|

详见归档 spec `docs/spec/archive/*-add-tests-bench-manifest-config/`。

---

## L6 — 工作区（Workspace）

管理多工程 monorepo，统一构建、版本与共享元数据。

> **C1 落地范围（2026-04-26）**：本节描述 schema 与共享继承的最终形态。
> include 机制（C2）、policy 强制（C3）、CLI 工具链（C4）见后续章节。

### L6.1 文件名与角色

| 文件 | 数量 | 角色 |
|---|---|---|
| `z42.workspace.toml` | workspace 根目录唯一一份 | virtual manifest：协调 + 共享配置 |
| `<name>.z42.toml` | 每个 member 一份 | member 自身配置 |

`z42.workspace.toml` 是 **virtual manifest**：仅含 `[workspace.*]` / `[profile.*]` /
`[policy]` 段，**不允许**有 `[project]` 段（违反报 `WS036`）。`[workspace]` 段也只能
出现在文件名为 `z42.workspace.toml` 的文件中（违反报 `WS030`）。

### L6.2 顶层结构

```toml
# z42.workspace.toml — virtual manifest

[workspace]
members         = ["libs/*", "apps/*"]   # glob 与显式路径混用
exclude         = ["libs/sandbox-*"]     # 从 glob 结果排除
default-members = ["apps/hello"]         # 不带 -p 时的默认编译子集
resolver        = "1"

[workspace.project]                      # 共享元数据（D5：仅以下 4 个字段可共享）
version     = "0.1.0"
authors     = ["z42 team"]
license     = "MIT"
description = "..."

[workspace.dependencies]                 # 中央版本声明
"my-utils" = { path = "libs/my-utils", version = "0.1.0" }

[workspace.build]                        # 集中产物（C3 落地实际行为）
# restructure-publish-output-dirs (2026-06-19): 四件套；unset = 级联默认。
# 默认等价于：
#   output_dir  = "artifacts/${project_name}/${profile}"
#   cache_dir   = "${output_dir}/.cache"
#   dist_dir    = "${output_dir}/dist"
#   publish_dir = "${output_dir}/publish"
# 整段可以省略，留这里只为展示语法。
dist_dir = "dist/${profile}"

[policy]                                 # 强制策略（C3 落地）
"build.dist_dir" = "dist"

[profile.debug]                          # 集中 profile（成员不可覆盖）
mode  = "interp"
debug = true

[profile.release]
mode     = "jit"
optimize = 3
strip    = true
```

### L6.3 Member 引用 workspace 共享

```toml
# apps/hello/hello.z42.toml

[project]
name              = "hello"              # 身份字段必须 member 自己声明
kind              = "exe"
entry             = "Hello.main"
version.workspace = true                 # ← Cargo 风格：引用 workspace.project.version
license.workspace = true

[sources]
include = ["src/**/*.z42"]

[dependencies]
"my-utils".workspace = true              # ← 引用 workspace.dependencies."my-utils"
"my-http"  = { workspace = true, optional = true }   # 局部修饰
```

**身份字段 vs 共享字段（D5）：**

| 字段 | 共享 | 备注 |
|---|---|---|
| `name` / `kind` / `entry` | ❌ | member 必须自己声明（防止意外冲突） |
| `version` / `authors` / `license` / `description` | ✅ | 可用 `xxx.workspace = true` 引用 |

**禁用旧语法：** `version = "workspace"` 不再支持（报 `WS035`），改为 key 层面
`xxx.workspace = true` 或 `{ workspace = true }`。

### L6.4 Members 展开

```toml
members = ["libs/*", "apps/main", "experiments/foo"]
exclude = ["libs/sandbox-*"]
```

- glob 仅匹配**目录**，目录内必须恰好一份 `*.z42.toml`（多份 → `WS005`）
- 显式路径与 glob 可混用
- exclude 优先于 members
- `default-members` 必须是展开结果的子集（否则 `WS031`）

orphan member（子树有 manifest 但未被 `members` 命中）→ `WS007`（warning，不阻塞构建）。

### L6.5 include 机制（C2，2026-04-26）

Member 与 preset 文件可通过 `include` 字段拉入**子树/分组共享配置**（如 "libs/* 都用 lib defaults"），位置自由、不依赖目录层级。

```toml
# libs/foo/foo.z42.toml
include = [
    "${workspace_dir}/presets/lib-defaults.toml",
    "${workspace_dir}/presets/strict-lints.toml",
]

[project]
name              = "foo"
version.workspace = true
```

#### 合并语义

| 字段类型 | 合并规则 |
|---|---|
| 标量（`kind` / `license` / `version`） | 后写者覆盖前写者；自身（声明 include 的文件）覆盖所有 preset |
| 表（如 `[project]`） | 字段级合并；同名字段后者覆盖 |
| 数组（如 `[sources].include`） | **整体覆盖**（不连接，避免菱形依赖时列表难推测） |
| `[dependencies]` | 按 `name` 字典合并；同名后者整体替换 |

#### 路径规则

- 路径相对于声明 include 的文件
- 支持 `${workspace_dir}` / `${member_dir}` 模板变量
- **不允许**：绝对系统路径 / URL / glob 模式（违反报 `WS024`）

#### Preset 文件段限制（WS021）

Preset 不允许：
- `[workspace.*]`、`[policy]`、`[profile.*]` —— 治理一致性，必须从 workspace 根下发
- `[project].name`、`[project].entry` —— 身份字段，member 必须自己声明

Preset **允许**：`[project] kind / license / authors / description / pack`、`[sources]`、`[build]`、`[dependencies]`。

#### 嵌套 / 循环 / 菱形

- preset 内可再写 `include` 拉入其他 preset
- 嵌套深度上限 8 层（超过 → `WS022`）
- 循环 include（A→A 或 A→B→A）→ `WS020`，错误信息列完整环
- 菱形 include（同一文件被多次拉入）→ 去重，仅合并一次（不报错）

#### 配置生效顺序（C2 完整）

```
1. workspace 根 [workspace.project] / [workspace.dependencies]   （C1 默认）
2. member 的 include 链按声明顺序展开 + 合并                     （C2 新增）
3. member 自身字段                                              （member 覆盖）
4. workspace 根 [policy]                                        （C3 强制）
5. CLI flag                                                     （C4 最终）
```

#### 错误码（C2 新增）

| 码 | 含义 | 级别 |
|---|---|---|
| WS020 | 循环 include | error |
| WS021 | preset 含禁用段 | error |
| WS022 | include 嵌套深度超过 8 层 | error |
| WS023 | include 路径不存在 | error |
| WS024 | include 路径不允许（绝对路径 / URL / glob） | error |

#### 示例

完整可解析示例见 `examples/workspace-with-presets/`：
- `presets/lib-defaults.toml` 提供 `kind=lib` + `[sources]` 默认
- `presets/strict-lints.toml` 提供 `[build].mode = "interp"`
- `libs/foo/` include lib-defaults
- `libs/bar/` include lib-defaults + strict-lints（后者覆盖前者重叠字段）

---

### L6.6 z42c workspace 模式（C4a，2026-04-26）

z42c 在执行命令前先尝试发现 workspace 根：从 CWD 向上找 `z42.workspace.toml`。

| 情况 | z42c 行为 |
|---|---|
| CWD 在 workspace 内（无显式 path / `--no-workspace`） | workspace 模式：调 `WorkspaceBuildOrchestrator` 编译 |
| CWD 在 member 子目录 + 无 `-p` / `--workspace` | 自动 `-p <当前 member>`，编译该 member 与依赖闭包 |
| CWD 在 workspace 根 + 无 `-p` / `--workspace` | 编译 `default-members`（无则全部） |
| 给出显式 path / `--no-workspace` / 不在 workspace 内 | 单工程模式 / 单文件模式（行为不变） |

#### 命令矩阵（C4a 范围）

```bash
z42c build                      # 自动发现 workspace；按 default-members 或当前 member 编译
z42c build --workspace          # 编译所有 members
z42c build -p hello             # 仅编译 hello 与依赖闭包
z42c build -p foo -p bar        # 多选
z42c build --exclude experiments  # 配合 --workspace 排除
z42c build --release            # 切到 release profile
z42c build --no-workspace       # 强制单工程模式
z42c check ...                  # 同 build，但仅类型检查（不写产物）
```

#### 拓扑编译顺序

```
core ← utils ← hello
```

`z42c build --workspace` 编译顺序：先 `core`，后 `utils`，最后 `hello`（C4a 串行，并行 future）。
任一 member 失败 → 其传递下游被标记为 `blocked`（不编译）；姐妹分支不受影响。

#### 错误码（C4a 新增）

| 码 | 含义 | 级别 |
|---|---|---|
| WS001 | 两个 members 声明同一 `[project] name` | error |
| WS002 | `-p` 与 `--exclude` 同时指定同一 member | error |
| WS006 | Member 间依赖图含环 | error |

#### 示例

完整跨 member 依赖示例见 `examples/workspace-full/`：含 core / utils / hello 三层依赖。

#### 查询命令（C4b，2026-04-26）

```bash
z42c info                       # 列出 workspace 概览（members/kinds/默认 profile）
z42c info --resolved -p hello   # hello 的最终生效配置 + 每字段来源标注
z42c info --include-graph -p X  # 显示 member X 的 include 链（preset 展开树）
z42c metadata --format json     # 机读 JSON（含 schema_version: "1" + dependency_graph）
z42c tree                       # ASCII 显示跨 member 依赖树
z42c lint-manifest              # 静态校验所有 manifest（不编译；返回 WSxxx 报告）
```

错误输出（含 WSxxx 前缀）经 `CliOutputFormatter` 格式化：
- 默认彩色（red error / yellow warning / dim help|note）
- 检测到 `NO_COLOR` 环境变量或 stderr 重定向时自动禁用
- ManifestException 原 message 内容完整保留

#### 脚手架 + 清理（C4c，2026-04-26）

```bash
z42c new --workspace mymonorepo       # 生成新 workspace 完整骨架
z42c new -p foo --kind lib            # 在当前 workspace 加 lib member（libs/foo/）
z42c new -p hello --kind exe          # 加 exe member（apps/hello/，src 含 Hello.Main 由 BuildTarget 自动定位）
z42c init                             # 把当前单 manifest 升级为 workspace
z42c fmt                              # 格式化所有 *.z42.toml（Tomlyn round-trip）

z42b clean                            # 删当前目录 dist/ + cache/（clean 已从 z42c 移到 z42b）
z42b clean <dir>                      # 删 <dir>/dist + <dir>/cache
```

> **clean 归 z42b**：z42c 是纯编译器（只 build / --emit-zbc / --dump-*）；产物生命周期
> （clean）与 test / bench 一样归编排器 z42b。`z42b clean [<dir>]` 删 `<dir>/{dist,cache}`。

`z42c new --workspace` 默认布局：

```
mymonorepo/
├── z42.workspace.toml      （[workspace] / [workspace.project] / [workspace.build]）
├── .gitignore              （dist/ + .cache/）
├── presets/
│   ├── lib-defaults.toml
│   └── exe-defaults.toml
├── libs/
└── apps/
```

C4c 同时完成 WS004 完全移除（C3 标记 `[Obsolete]` 后）；归并入 WS010。

---

### L6.7 Policy 与集中产物（C3，2026-04-26）

#### `[workspace.build]` 集中产物布局

workspace 模式下，所有 member 产物**集中**到 workspace 根下的 `artifacts/` 子树（restructure-publish-output-dirs 2026-06-19 新默认）：

```
<workspace_root>/
└── artifacts/
    ├── foo/
    │   └── debug/               ← output_dir (默认 artifacts/${project_name}/${profile})
    │       ├── dist/            ← dist_dir (默认 ${output_dir}/dist)
    │       │   ├── foo.zpkg
    │       │   └── foo.zsym
    │       ├── .cache/
    │       │   └── foo/         ← 防碰撞：cache 追加 member 子目录
    │       │       └── src/Foo.zbc
    │       └── publish/         ← publish_dir (默认 ${output_dir}/publish)
    │           ├── foo.zpkg     （exe 才自动填充；lib 需 z42c publish）
    │           └── dep.zpkg     （exe 的非 stdlib 依赖）
    ├── bar/
    │   └── debug/ ...
    └── hello/
        └── debug/ ...
```

```toml
# z42.workspace.toml
[workspace.build]
# restructure-publish-output-dirs (2026-06-19): 默认 output_dir 已改为
# artifacts/${project_name}/${profile}，省略即等价于以下设置：
# output_dir  = "artifacts/${project_name}/${profile}"
# cache_dir   = "${output_dir}/.cache"    (+ member 子目录防碰撞)
# dist_dir    = "${output_dir}/dist"
# publish_dir = "${output_dir}/publish"

# 若要按 profile 做顶层区分（0.3.x 以前旧默认），显式设置：
# output_dir = "artifacts/${project_name}/${profile}"
dist_dir = "dist/${profile}"   # ${profile} 模板示例：debug/release 各自分流
```

#### `[policy]` 强制策略

`[policy]` 段锁定字段值，member 不可覆盖（违反报 `WS010`）：

```toml
[policy]
"profile.release.strip" = true        # release 产物必须 strip
"build.mode"            = "interp"    # 全 workspace 仅用 interp 模式
```

**字段路径表达式**（D3.1）：用点分隔的扁平字符串 key。

**默认锁定字段**（D3.2，restructure-publish-output-dirs 2026-06-19 扩展为四件套）：

| 字段路径 | 默认锁定值 |
|---|---|
| `build.output_dir` | `[workspace.build].output_dir` |
| `build.cache_dir` | `[workspace.build].cache_dir` |
| `build.dist_dir` | `[workspace.build].dist_dir` |
| `build.publish_dir` | `[workspace.build].publish_dir` |

无需在 `[policy]` 显式声明；自动生效。member 若试图覆盖产物路径 → `WS010`。

#### Policy 检测语义

`PolicyEnforcer` 仅检查 member **显式声明**的字段。例：

- Member 不写 `[build]` → 无冲突，使用 workspace cascade 默认（`${output_dir}/dist`）
- Member 写 `[build] dist_dir = "dist"`（与 workspace 锁定值相同）→ 不冲突，origin 标 PolicyLocked
- Member 写 `[build] dist_dir = "custom"` → `WS010`

#### 字段路径不存在（WS011）

```toml
[policy]
"build.outdir" = "dist"   # 拼写错误（应为 build.dist_dir）
```

报 `WS011 PolicyFieldPathNotFound`，附 fuzzy 建议（编辑距离 ≤ 3）：`did you mean 'build.dist_dir'?`

#### ResolvedManifest 集中产物字段

C3 在 `ResolvedManifest` 上的 effective 路径字段（restructure-build-output-dirs
2026-06-06 扩为三件套；workspace 和单工程**两种模式都填充**，不再是
workspace-only）：

| 字段 | 含义 |
|---|---|
| `IsCentralized` | true = workspace 集中布局；false = 单工程 |
| `EffectiveOutputDir` | 顶层输出根目录绝对路径（`${output_dir}` 模板变量解析为此） |
| `EffectiveCacheDir` | 该 member 的 cache 目录绝对路径（workspace 模式下含 member 子目录） |
| `EffectiveDistDir` | 该 member 的 dist 目录绝对路径（替代了原 `EffectiveOutDir` 字段） |
| `EffectiveProductPath` | 该 member 产物完整路径 (`<EffectiveDistDir>/<name>.zpkg`) |

C4 的 `WorkspaceBuildOrchestrator` 直接消费 `EffectiveProductPath` 写产物；
单工程 `PackageCompiler.Run` 也走同一字段（restructure-build-output-dirs
统一了两条路径，避免之前各自计算 effective 路径的双份逻辑）。

#### 错误码（C3 新增）

| 码 | 含义 | 级别 |
|---|---|---|
| WS010 | Policy 冲突：member 字段值与 workspace 锁定值不一致 | error |
| WS011 | Policy 字段路径不存在（含 fuzzy 建议） | error |

> WS004（C1 占位）在 C3 标记 `[Obsolete]`，C4c 阶段已彻底移除（归并入 WS010）。

#### 示例

完整可解析示例见 `examples/workspace-with-policy/`：含 `[workspace.build] dist_dir = "dist/${profile}"` + `[policy] "profile.release.strip" = true`。

---

### L6.8 Member 段限制

Member `<name>.z42.toml` **不允许**以下段（违反报 `WS003`）：

- `[workspace]` / `[workspace.*]` —— 全仓共享必须从根下发
- `[policy]` —— 治理一致性
- `[profile.*]` —— profile 集中在 workspace 根

### L6.9 路径模板变量（D8）

路径字段允许 4 个内置只读变量：

| 变量 | 含义 |
|---|---|
| `${workspace_dir}` | workspace 根绝对路径 |
| `${member_dir}` | 当前 member 目录绝对路径 |
| `${member_name}` | 当前 member `[project].name` |
| `${profile}` | 当前激活 profile 名 |

**语法**：
- 占位 `${name}`；字面量 `$` 写 `$$`
- 嵌套 / 未闭合 / 非法字符 → `WS038`
- 未知变量（含 `${env:NAME}` 暂不支持）→ `WS037`

**允许字段白名单**（其他字段出现 `${...}` 报 `WS039`）：

- `include` 数组各元素（C2 用）
- `[build] output_dir / cache_dir / dist_dir`（restructure-build-output-dirs, 2026-06-06）
- `[workspace.build] output_dir / cache_dir / dist_dir`（同上）
- `[workspace.dependencies] xxx.path` / `[dependencies] xxx.path`
- `[sources] include / exclude`

**禁止字段**：标量元数据（`version` / `name` / `kind` / `entry` / `description` /
`license` / `authors`）以及 `members` glob 模式。

```toml
# 合法
[workspace.build]
dist_dir = "dist/${profile}"              # 展开 → dist/release

# 非法（WS039）
[project]
version = "${profile}"                    # 标量字段不允许变量
```

### L6.10 配置生效顺序

```
最终 member 配置 = 以下层按顺序合并：

1. workspace 根 [workspace.project] / [workspace.build] / [workspace.dependencies]   (C1 默认)
2. member 的 include 链按声明顺序展开 + 合并                                          (C2)
3. member 自身 *.z42.toml 字段                                                       (C1 member 覆盖)
4. workspace 根 [policy] 段                                                          (C3 强制覆盖)
5. CLI flag（--release / --profile X / --no-incremental 等）                          (C4 最终覆盖)
```

C1 仅落实步骤 1 + 3（含路径模板展开）；C2 加 2；C3 加 4；C4 加 5。

### L6.11 错误码索引（C1+C2+C3+C4a 范围）

| 码 | 含义 | 级别 |
|---|---|---|
| WS003 | Member 内出现禁用段（`[workspace.*]` / `[policy]` / `[profile.*]`） | error |
| WS005 | 同目录两份 `*.z42.toml` 引发歧义 | error |
| WS007 | Manifest 在 workspace 子树内但未被 members 命中 | warning |
| WS030 | `[workspace]` 段出现在非 `z42.workspace.toml` 文件 | error |
| WS031 | `default-members` 含未匹配项 | error |
| WS032 | Member 引用 workspace 共享字段，但根未声明 | error |
| WS033 | `[workspace.project]` 字段类型错误 / 不可共享字段被声明 | error |
| WS034 | Member 引用未声明的 workspace 依赖 | error |
| WS035 | 出现已废弃的 `version = "workspace"` 语法 | error |
| WS036 | Workspace 根 manifest 同时含 `[workspace]` 与 `[project]` | error |
| WS037 | 路径模板含未知变量 | error |
| WS038 | 路径模板语法非法 | error |
| WS039 | 模板变量出现在不允许的字段 | error |

> WS020-024 为 C2 启用（include）。WS010/011 为 C3 启用（policy）。WS001/002/006 为 C4a 启用（编译运行时）。WS004 已在 C4c 移除。

### L6.12 目录结构样板

```
monorepo/
├── z42.workspace.toml
├── libs/
│   ├── greeter/
│   │   ├── greeter.z42.toml          ← member（kind=lib）
│   │   └── src/
│   └── ...
└── apps/
    └── hello/
        ├── hello.z42.toml            ← member（kind=exe）
        └── src/
```

**完整可解析示例**：见 `examples/workspace-basic/`。

---

## 产物文件汇总

| 文件 | 含义 | 谁写 | 纳入 VCS |
|------|------|------|---------|
| `<name>.z42.toml` | 工程 / 工作区配置 | 开发者 | ✅ |
| `.cache/*.zbc` | 单文件增量字节码（pack=false 时生成）| 编译器 | ❌ |
| `dist/*.zpkg` | 工程包（indexed 或 packed）| 编译器 | ❌ |

`.cache/` 和 `dist/` 加入 `.gitignore`。

---

## 完整字段速查

```toml
[project]
name        = "my-app"          # 必填
version     = "0.1.0"           # 必填，SemVer
kind        = "exe"             # 必填，exe | lib
entry       = "MyApp.Main"      # 可选；省略时自动发现 Main
description = ""                # 可选
authors     = []                # 可选
license     = "MIT"             # 可选，SPDX
pack        = false             # 可选，工程级 pack 默认值

[sources]
include = ["src/**/*.z42"]      # 默认值
exclude = []                    # 默认值

[build]
# restructure-build-output-dirs (2026-06-06): 三件套字段全 optional；
# 不设走级联默认（output_dir 默认 = toml 所在目录；cache_dir / dist_dir
# 默认 = ${output_dir}/.cache 和 ${output_dir}/dist）。
# output_dir = "/build/myproj"    # 顶层
# cache_dir  = "/dev/shm/myproj"  # 中间产物（可独立放 tmpfs）
# dist_dir   = "/build/myproj/dist"  # 最终产物
incremental = true              # 默认 true
mode        = "interp"          # 全局默认执行模式

[dependencies]
# "pkg-name" = "*"              # zpkg 包名（匹配 zpkg manifest 的 [project] name）
# "pkg-name" = "1.2.0"         # 版本约束（目前仅做存在性校验）
# stdlib 无需声明，由 VM 自动加载

[profile.debug]
mode     = "interp"
optimize = 0
debug    = true
pack     = false                # → indexed zpkg（默认）

[profile.release]
mode     = "jit"
optimize = 3
strip    = true
pack     = true                 # → packed zpkg（默认）

# ── workspace 模式（仅 z42.workspace.toml 中合法） ─────────────────────────
[workspace]
members         = ["libs/*", "apps/*"]
exclude         = []
default-members = []
resolver        = "1"

[workspace.project]              # 共享元数据；成员用 xxx.workspace = true 引用
version     = "0.1.0"
authors     = []
license     = "MIT"
description = ""

[workspace.dependencies]         # 中央版本声明；成员用 dep.workspace = true 引用
# "pkg-name" = { path = "...", version = "0.1.0" }

[workspace.build]                # 集中产物（C3 实施实际行为）
# restructure-build-output-dirs (2026-06-06): 三件套同 [build]；不设
# 走 ${workspace_root}/.cache 和 ${workspace_root}/dist 的默认。
# output_dir = "/build/${workspace_dir}"
# cache_dir  = ".cache"
# dist_dir   = "dist/${profile}"

[policy]                         # 强制策略（C3 实施）
# "build.dist_dir" = "dist"

[workspace]                     # L6，与 [project] 可共存
members = []
[workspace.dependencies]
# name = { path = "...", version = "..." }
```

## `[platform.*]` 平台配置段（add-export-command, 2026-06-14）

`z42c` **不读取**这些段；由 `z42 export` 命令消费。注册到 `ProjectManifest.KnownTopLevelKeys` 以避免 WS008 告警。

### `[platform.ios]`

```toml
[platform.ios]
bundle_id      = "com.example.myapp"   # required: CFBundleIdentifier
display_name   = "My App"             # optional: CFBundleDisplayName（默认 = project name）
version        = "1.0.0"             # optional: CFBundleShortVersionString（默认 = project version）
min_ios        = "15.0"              # optional: IPHONEOS_DEPLOYMENT_TARGET（默认 "15.0"）
team_id        = ""                  # optional: CODE_SIGN_TEAM（留空 = Automatic）
device_families = [1, 2]            # optional: 1=iPhone 2=iPad（默认 [1,2]）
```

### `[platform.android]`

```toml
[platform.android]
app_id       = "com.example.myapp"  # required: Gradle applicationId
display_name = "My App"             # optional: app_name string resource（默认 = project name）
version_code = 1                    # optional: versionCode（默认 1）
version_name = "1.0.0"             # optional: versionName（默认 = project version）
min_sdk      = 26                   # optional: minSdk（默认 26 = Android 8.0）
target_sdk   = 34                   # optional: targetSdk（默认 34 = Android 14）
```

### `[platform.wasm]`

```toml
[platform.wasm]
title = "My App"   # optional: HTML &lt;title&gt;（默认 = project name）
```

### `[platform.desktop]`（apphost-as-config, 2026-06-17；apphost gate 显式化 2026-06-30）

```toml
[platform.desktop]
apphost     = true   # GATE：唯有 apphost = true，`z42 publish <toml> --rid <desktop-rid>` 才产 apphost。
                     # 缺省 / false → publish 报 "not configured to publish a desktop apphost" 并退出。
publish_dir = ".."   # 仅输出位置（部署根，相对 toml 所在目录，同 [build].output_dir 基准）。
                     # 不再充当 gate；缺省 = ${output_dir}/publish（对齐 [build] 四件套默认，
                     # add-build-toolchain 2026-07-05；output_dir 未设→workspace 继承→<项目目录>/publish）。
                     # --output 可覆盖。
# 部署布局（可选，add-package-layout-config）：apphost 二进制与 payload zpkg 在
# 部署根（publish_dir）下的相对路径。缺省 → 扁平：apphost = publish_dir/<name>，
# payload 原地内嵌（不复制）。
bin     = "bin/myapp"                 # apphost 二进制落点（相对部署根）
payload = "programs/myapp/myapp.zpkg" # payload zpkg 落点；publish 把已编译 zpkg 复制到此
```

桌面平台的输出是 **apphost**（per-app 原生可执行）：`z42 publish <toml> --rid <desktop-rid>` 在
`apphost = true` 时，读 `publish_dir`（部署根）+ 从 `[build]`/`[project]` 推出已编译 zpkg，patch 原生 apphost
stub 产出 exe。与 ios/android/wasm export 对称——apphost 不是独立命令。

> **部署布局 `bin` / `payload`（add-package-layout-config）**：apphost 应用天生两部分——原生启动器
> 二进制 + payload zpkg。两个可选字段把它们放到部署根下的**完整相对路径**：
> - `bin`：apphost 二进制路径（如 `bin/myapp`；不写则 `<name>` 落部署根，保持旧扁平行为）。
> - `payload`：payload zpkg 路径（如 `programs/myapp/myapp.zpkg`）。设了 → publish 把已编译 zpkg
>   **复制**到此处，使部署子树自洽；不设 → 原地内嵌已编译 zpkg（不复制）。
>
> apphost 内嵌的 payload 相对路径（从 `bin` 目录到 `payload`）由 publish **自动计算**，用户不手算。
> 这是面向用户的通用旋钮——用户发布自己的 app 与 z42 SDK 内部布置 z42c/z42b/z42d **共用同一套字段**。
> 解析见 `z42.project` 的 `DesktopConfig.Bin` / `.Payload`；消费见 `launcher_export.z42` 的 `_cmdPublishDesktop`。

> **gate 与位置分离（2026-06-30）**：旧逻辑用「`publish_dir` 是否存在」充当「是否产 apphost」的开关，
> 把"输出目录"与"是否启用"耦合在一个键上。现拆分——`apphost = true` 是唯一 gate，`publish_dir` 退化为
> 纯输出位置。解析见 `z42.project` 的 `DesktopConfig.Apphost`；gate 实现见 `launcher_export.z42`
> 的 `_cmdPublishDesktop`。机制详见 [launcher.md](../runtime/launcher.md) apphost 段。

### CLI 覆盖

所有 toml 值可通过 CLI 标志覆盖：

```
z42 export ios     <project.z42.toml> [--bundle-id com.x.y] [--output ./MyApp] [--sdk-ver 0.3.0]
z42 export android <project.z42.toml> [--app-id  com.x.y] [--output ./MyApp] [--sdk-ver 0.3.0]
z42 export wasm    <project.z42.toml>                       [--output ./MyApp] [--sdk-ver 0.3.0]
z42 publish <project.z42.toml>                             [--output <publish_dir>]
```

详细设计见 [`docs/design/toolchain/export.md`](../toolchain/export.md)。

## 条件配置：类型化轴子表（前瞻设计，未实施）

> ⚠️ 前瞻设计（未实施）。决策见 [build-orchestrator.md](../toolchain/build-orchestrator.md) Decision #8。

z42.toml **不引入 csproj 式 `Condition` 表达式求值**。沿已知变化轴（profile / platform / rid）
的条件内容，用**类型化轴子表 + 确定性合并**表达，而非字符串布尔表达式——"条件"靠表键匹配，
不靠表达式求值：

```toml
[dependencies]                      # 公共依赖
http = "1.0"

[platform.ios.dependencies]         # 仅 ios 目标合入
swift-bridge = "0.3"

[profile.release.defines]           # 仅 release profile
NDEBUG = true
```

**合并优先级**（低 → 高，后者覆盖前者；按 key 合并，dep/define 同名取高优先级值）：

```
base（[dependencies] / [build] / 顶层）
  → [profile.<profile>].*           # 当前 profile（debug/release）
  → [platform.<family>].*           # 当前目标平台族（由 --rid 分类）
  → CLI 标志                         # 最高
```

**为什么不引入 condition 表达式引擎**：

- 类型化 + 可校验（轴段字段有 schema）；condition 的 `'$(X)'=='true'` 是字符串，拼错静默变 false。
- **顺序无关**；MSBuild condition 依赖 property 自上而下求值的状态（"为什么这值不对"调试地狱）。
- 无需解析器 / 求值器 / property 作用域引擎。

**组合条件**（如 windows ∧ release）多轴嵌套若过于啰嗦，可后续评估 cargo 式**有界 `cfg()` 谓词**
（封闭 key 集 + `all()`/`any()`/`not()`，仍可校验，**非**任意表达式）——见 build-orchestrator.md Deferred。

**任意逻辑**（超出已知轴的条件判断 / 计算）→ 归 [`build/` hooks](#build-构建扩展目录z42b-自定义流程build-orchestrator)
（z42 代码里 `if` 判断），**不进 config**。即「声明式变化用封闭类型化轴 + 确定性合并；任意逻辑用代码」。

## `build/` 构建扩展目录（z42b 自定义流程，build-orchestrator）

> ⚠️ 前瞻设计（未实施）。完整设计见 [`docs/design/toolchain/build-orchestrator.md`](../toolchain/build-orchestrator.md)。

项目可选地用一个 **`build/` 目录**（与 `src/` 平级）放构建流程的**自定义扩展** z42 源；
`z42b` 编排器发现并编译它们进一次性 driver（约定优于配置，类比 `build.rs`）。

```
myapp/
  z42.toml
  src/                       # 应用代码
  build/                     # ← 可选；构建扩展（z42b 编译进自定义 driver）
    ProjectHooks.z42         #   class ProjectHooks : BuildHooks   —— 平台无关编译前后 hook
    iOSBuild.z42             #   class iOSBuild : iOSWorkload      —— 平台尾相位 override
```

- **固定类名约定**（静态绑定、不需反射）：
  - `ProjectHooks`（`: BuildHooks`）→ 注入 `Pipeline.Hooks`；
  - `<Family>Build`（如 `iOSBuild` / `DesktopBuild`，`: <Platform>Workload`）→ 覆盖该平台标准 workload。
  - 缺则用默认（空 Hooks / 标准 workload）。
- **编译前 / 编译后**自定义 = `ProjectHooks` override `BeforeCompile` / `AfterCompile`
  （及 `Before/After` × `Trim` / `Assets`，共 6 个 hook）；平台专属定制走 `<Family>Build`
  override + `base.X(ctx)`。
- **`build/` 不存在或为空** → 标准路径（z42b 进程内组合，无 driver 生成）。
- **相位封闭**（八个，线性，不可增删改序）：所有自定义只落在 Hooks / Workload override 上，
  不开放注册新相位（保证构建确定性与缓存模型）。

扩展点基类（`BuildHooks` / `WorkloadBase`）住 [`src/libraries/z42.build/`](../../../src/libraries/z42.build/)。
