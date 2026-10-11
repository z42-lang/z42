# z42 工程文件规范（`<name>.z42.toml`）

z42 使用 **`<name>.z42.toml`** 作为工程配置文件，格式为 TOML。
一个目录下最多一个 `*.z42.toml`，支持单工程和多工程工作区两种形态。

> **本文档的边界**：描述用户 manifest 字段（`[build]` / `[[exe]]` / `[dependencies]` / `[workspace]` 等）与构建编排语义。**不描述** `.zbc` / `.zpkg` 二进制格式（归 `compilation.md`）。

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

### 键校验：本页没列的键一律报错

工程清单与工作区清单里**本页没列出的键都是错误**，不会被静默忽略（拼错的 `[build] output-dir`
不会「配了以为生效、实际走默认」）。报错是构建工具的错误行，**不是诊断码**：

| 情况 | 实际输出 | 结果 |
|---|---|---|
| 工程清单有未知键 | ``z42c: <清单>: unknown key `<键>` in [<段>] (known: a, b, c)`` | 逐条报出，exit 2（用法错误），什么都不构建 |
| 工作区清单有未知键 | ``z42c build --workspace: <z42.workspace.toml>: unknown key `<键>` in [workspace] (known: …)`` | 规划阶段失败（exit 1），不构建任何成员 |
| `[lints]` 的值不对 | ``z42c: <清单>: [lints] `<规则>`: unknown severity "<值>" (known: none, hidden, info, warning, error)``；非串值报 ``unknown key `<键>` in [lints] (…)`` | 同工程清单未知键 |

键名本身就是用户数据、因而**不按本页审计**的位置：`[dependencies]` / `[analyzers]` / `[native]` 下的
包名 / 子表名、`[lints]` 的规则名（值照样校验，见 [L5c](#lints--逐规则-severity-覆盖)）、`[properties]` 与 `[profile.<n>.properties]` 的键、
`[profile.<n>.runtime]` 的旋钮名（按 VM 登记表校验，未知名仅警告，见
[运行时设置](runtime-settings.md#更早一层构建期的名字校验)）、`[optimize]` / `[syntax]` 的名字
（由各自的名表校验，未知名报错，见 [L5d](#l5d--optimize--syntax逐项具名旋钮)）。

---

## L1 — 包身份（最小工程）

每个工程必须有唯一标识和产物类型。

```toml
[project]
name    = "hello"      # 包名，全小写（见下「包名命名规则」）
version = "0.1.0"      # SemVer
kind    = "exe"        # exe | lib | analyzer
entry   = "Hello.Main" # 可选；省略时由 PackageCompiler 自动发现 Main
```

**字段说明：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✅ | 全小写；作为输出文件基名和依赖引用键。命名规则见下节 |
| `version` | string | ✅ | SemVer，如 `"0.1.0"` |
| `kind` | `"exe"` \| `"lib"` \| `"analyzer"` | 单目标必填；多目标用 `[[exe]]` 时省略 | 可执行程序 / 类库 / 编译期扩展（analyzer、generator 都用这个 kind）。写其它值 → 构建报用法错误 `unknown kind` |
| `entry` | string | ❌ 可选 | 完全限定入口函数。**省略时**`PackageCompiler` 自动从编译后的 module 查找 `Main`（优先 `<Namespace>.Main` 再 `<Namespace>.main` 再裸 `Main` / `main`）；找不到则**编译期报错** |
| `pack` | bool | ❌ 可选 | packed / indexed 布局，见 [L3](#l3--构建产物配置) |
| `description` / `authors` / `license` | string / string[] / string | ❌ 可选 | 纯元数据（`authors` 是字符串数组，`license` 建议写 SPDX 标识）；构建不使用 |

### 包名命名规则

包是**分发单位**（一份 `[project]` = 一个包），与命名空间是两个独立概念，命名规则也不同。

```toml
[project]
name    = "z42.collections"      # ✓ 全小写，点分层级
version = "0.1.0"

[dependencies]
"acme.web"  = "1.2.0"            # ✓ 引用时也小写
"my-utils"  = "*"                # ✓ 单段名里的 `-` 也常见
```

- **全小写**。`z42.IO` 与 `z42.io` 不是同一个包名，别指望编译器帮你对齐大小写。
- **`.` 表示层级**，通常是"组织.功能"（`z42.io` / `acme.payments` / `unity.physics`）。
- **`-` 只在单段名内部用**（`my-utils` / `hello-world`），不要拿它当层级分隔符。
- **不用 `_`**，不用大写字母。
- **stdlib 命名族**是 `z42.<topic>`（`z42.core` / `z42.io` / `z42.numerics` / `z42.test` …）。
  `z42.*` 是官方保留前缀——它们随工具链分发、**始终可用**，`[dependencies]` 里
  **只写第三方包**（Rust-std 模型），也不要给自己的包起 `z42.` 开头的名字。
  声明 `z42.*` 是冗余、但无害：编译器不报错也不警告。

- **第三方包必须声明**，漏写会在编译期报 [`E0497`](../appendix/error-codes.md)，
  消息里直接给出要加的那一行。判据是**类型的归属包**，不是 `using` 的命名空间。

> **包名不受[命名约定](../conventions/naming.md)的 PascalCase 规则约束。** 包名出现在
> 命令行、TOML、文件系统路径里，按发布层世界的公约（npm / Cargo / pip 都小写）；命名空间
> 出现在源代码里，按代码层世界的公约（C# / Java 的 PascalCase）。两套规则各管一层。

| 维度 | 包名 | 命名空间 |
|------|------|---------|
| 出现位置 | manifest、CLI、文件系统目录 | 源码 `namespace` / `using` |
| 作用 | 分发 / 依赖管理（构建系统的"地址"）| 类型查找 / 符号路径（编译期的"路径"）|
| 形态 | `z42.collections`、`my-utils` | `Std.Collections`、`Demo.Web.Api` |
| 对照 | npm `@org/lib`、Cargo `tokio`、Maven `com.acme:lib` | C# / Java namespace |

**`name` 与命名空间的关系：**

`name` 是包的文件标识符，与命名空间**无关**。命名空间完全由源文件中的 `namespace xxx;` 声明决定，编译器在构建时从源文件中收集并写入 zpkg 的 `namespaces` 字段。`[dependencies]` 中填写的是包名（用于找文件），`using` 语句中填写的是命名空间（由源文件决定），两者无需一致。包 `z42.collections` 里装的是 `namespace Std.Collections`——名字**刻意**不一样，一个说"谁拥有"，另一个说"代码住哪儿"。

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

**各 `kind` 的产物：**

| kind | 产物 | 说明 |
|------|------|------|
| `exe` | `dist/<name>.zpkg`（带入口） | 可执行程序；用到的依赖随产物部署 |
| `lib` | `dist/<name>.zpkg` | 类库，由最终 exe 决定怎么部署 |
| `analyzer` | `dist/<name>.zpkg` | 编译期扩展（analyzer / generator）：只加载进编译器进程、编译期运行，永不链入运行期产物；被其它工程通过 [`[analyzers]`](#analyzers--加载进编译器编译期运行不链入产物) 引用，见 [compile-time-extensions.md](compile-time-extensions.md) |

packed / indexed 两种布局由 `pack` 决定，见 [L3 — 构建产物配置](#l3--构建产物配置)。

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
include = ["src/tool/**/*.z42"] # 可选：覆盖共享 [sources]
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✅ | exe 名，同时作为产物文件基名 |
| `entry` | string | ❌ 可选 | 完全限定入口函数；省略时走 `PackageCompiler` 的 `Main` 自动发现路径 |
| `include` | string[] | ❌ | 独立 glob，覆盖 `[sources]`；省略则继承 `[sources]` |

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

# D) per-target 覆盖（[[exe]] 内的 include 字段独立 glob，无视顶层 [sources]）
[[exe]]
name  = "tool"
entry = "Tool.Main"
include = ["src/tool/**/*.z42"]   # 仅这些文件参与 tool 编译
```

**不支持的语法**：

- 负 pattern（`!path/to/exclude`）—— 用 `exclude` 字段表达
- 大括号扩展（`{a,b,c}`）—— 写成多条 include
- 字符类（`[a-z]`）—— 用具体路径或 `*`

**exclude 的匹配规则**：按路径段匹配——`**` 匹配零个或多个整段，`*` / `?` 只在段内通配（不跨 `/`）。
不含 `**` 的模式按**任意深度的路径后缀**匹配：`_skip.z42`、`tests/_skip/*` 出现在哪一层都算；
段边界严格——`**/a.z42` 不会误中 `ba.z42`。`dist/`、`.cache/` 下的文件总是被排除。

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
pack = false           # 可选；省略 → debug 为 indexed、release 为 packed

[build]
# 三个目录字段（output_dir / cache_dir / dist_dir）都是可选的，未设走级联默认。
# output_dir  = "/build/myproj"      # 顶层根目录（单工程默认 <清单目录>/artifacts/<profile>；workspace 成员见下表）
# cache_dir   = "/dev/shm/cache"     # 中间产物（默认 ${output_dir}/.cache）
# dist_dir    = "/build/dist"        # 最终产物（默认见下表：单工程未配 output_dir 时是 <清单目录>/dist）
# （发布目录是 [platform.desktop].publish_dir，默认 ${output_dir}/publish）
incremental = true     # 启用增量编译，默认 true
```

**`[build]` 字段说明：**

| 字段 | 类型 | 默认（单工程） | 默认（workspace 成员） | 说明 |
|------|------|------|------|------|
| `output_dir` | string? | `<清单目录>/artifacts/<profile>`（= `${workspace_dir}/artifacts/${profile}`） | 展开 `[workspace.build].output_dir`（相对 workspace 根；未声明 = `artifacts/${project_name}/${profile}`） | 顶层输出根目录；`${output_dir}` 模板变量解析为此值。 |
| `cache_dir` | string? | `${output_dir}/.cache` | `[workspace.build].cache_dir` ?? `${output_dir}/.cache`（模板不含成员名时追加成员子目录防碰撞） | 中间产物（`.zbc` / 索引 / 增量元数据）。 |
| `dist_dir` | string? | 未配 `output_dir` 时 **`<清单目录>/dist`**；配了则 `${output_dir}/dist` | `${output_dir}/dist` | 最终分发产物（`.zpkg` + `.zsym`）。 |
| `generated_dir` | string? | `${output_dir}/generated`（显式 `""` = 不落盘） | 同左 | generator 生成的源码。 |
| `incremental` | bool | `true` | `true` | 基于 source hash 跳过未改动文件。CLI `--no-incremental` 是一次性覆盖，**永远压过本键**；两者任一为「关」即关。 |
| `hooks` | string? | （无） | （无） | **项目 build hook 源目录**（projDir 相对）。声明后 z42b 用注入的同一 `ICompiler` 编该目录 → 动态实例化 `Build.ProjectHooks : BuildHooks` → 注入 `Pipeline.Hooks`。hook 源须 `namespace Build;` + `class ProjectHooks : BuildHooks`。hook 目录里 stdlib 与 **SDK 库**（`z42.build` / `z42.project` 等编译器域包）**自动可见**、免声明，且不拷贝——hook 加载进 z42b 进程，用的就是宿主那份。**z42c 不消费此键**（仅 z42b 编排读），与 `[platform.*]` 同为编排/发布侧配置。用途见下文 publish 一节（`z42 publish` 经 hook 免装 workload 产 apphost）；编排实现属内部细节，本书不展开。 |

发布目录不在 `[build]`：它是 `[platform.desktop].publish_dir`（见下文 publish 一节），默认 `${output_dir}/publish`。

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
# output_dir = ${workspace_dir}/artifacts/${profile}；cache = ${output_dir}/.cache；dist = ./dist（见 z42.project BuildLayout）

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

**workspace 成员继承规则**（对所有构建方式一致）：成员清单的 `[build]` **既没配
`output_dir` 也没配 `dist_dir`** ⇒ 整套走上表「workspace 成员」一列 —— 无论是 `z42c build --workspace`、
单独构建该成员（`z42c build <member>.z42.toml` / `z42 build`）、被别的工程当作 **path 依赖**代建，还是
`z42 run` / `z42 clean` / `z42 publish` 查询产物位置，都是同一处。成员自己配了 `output_dir` 或 `dist_dir`
⇒ 单独构建与 path 依赖代建按成员自己的配置（单工程规则）；`--workspace` 构建始终用 workspace 布局。
`cache_dir` 模板若不含 `${member_name}` / `${project_name}`，会自动追加成员子目录，避免不同成员缓存碰撞。

「成员」的判定：从清单目录向上找**最近**的 `z42.workspace.toml`，其 `members`（缺省 `["*"]`）命中该目录
相对 workspace 根的路径、且 `exclude` 不命中。最近的那个 workspace 不收它 ⇒ 按单工程处理（不越级）。

**`z42c build` / `z42c publish` 行为**：

| 命令 | lib | exe |
|------|-----|-----|
| `z42c build` | 编译到 `dist_dir`，**不**复制到 `publish_dir` | 编译到 `dist_dir`，自动复制产物 + 非 stdlib 依赖到 `publish_dir` |
| `z42c build --no-publish` | 同上（不复制） | 只编译到 `dist_dir`，跳过 publish 步骤 |
| `z42c publish` | 编译 + 复制产物到 `publish_dir` | 编译 + 复制产物 + 非 stdlib 依赖到 `publish_dir` |

**`pack`（packed / indexed 布局）**：`[project].pack` 显式值优先；未写时按 profile 取默认——
`debug` → `false`（indexed），`release` → `true`（packed）。`pack` 只能写在 `[project]`。

**strip（剥离调试信息）**：没有独立的键或 CLI flag，**strip ≡ `--release`**。strip 时主 `<name>.zpkg` 不含 DBUG body，配套产出 `<name>.zsym` sidecar（zpkg 0.4 `SymOnly` flag，含 MDBG + BLID）。runtime 加载主 zpkg 后自动探测同目录 sidecar 并按 build_id 配对合并，缺失或不匹配时静默退化（trace 维持函数名 + 签名）。

**产物输出：**

| pack 值 | strip（`--release`） | 产物 | 说明 |
|---------|---------|------|------|
| `false` | `false` | `dist/<name>.zpkg`（indexed 主文件）+ `dist/<rel>.zbc` 散装 + `.cache/` | 开发态（debug 默认），DBUG 内嵌散装 zbc；未变文件 zbc 字节稳定 → 最小 patch |
| `false` | `true`  | ——（构建报错）| indexed 为开发态，与 `--release` strip 不兼容 |
| `true`  | `false` | `dist/<name>.zpkg` (packed)                  | 发布态，DBUG 内嵌（便于现场 debug）|
| `true`  | `true`  | `dist/<name>.zpkg` + `dist/<name>.zsym`      | 发布态，最小体积，离线可符号化 |

> **flat workspace 一律 packed**：`z42c build --workspace --output-dir <dir>` 让全部成员共用一个 dist，
> indexed 的散装 `<rel>.zbc` 会在成员之间按同名相对路径互相覆盖。所以这种构建下成员**总是** packed
> （debug 也一样）；成员显式写了 `pack = false` 则报错。per-member 布局（不带 `--output-dir`）不受影响。

**增量编译工作方式：**

判定与组装 SoT = **cache**（`<cache>/<rel>.zbc` fullMode + 同名 `.meta`），不再读上次 zpkg
MODS。粒度是**文件级**：只重编「变化文件 + 包内传递依赖方」，其余文件的 IrModule 从
cache zbc 读回（ZbcReader）。其关键特性是**无跨代元数据合并**
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
`--no-incremental` 强制全量。**硬验收 = 暴力对账器 `xtask test compiler incremental`**：语料逐文件
touch，断言增量产物与全量产物**逐字节相等** + D8 计时（增量 vs 全量墙钟）。

**cache 条目格式**：`<rel>.zbc`（fullMode，与 `--emit-zbc` 同一 `ZbcWriter.Write` 产出）+
`<rel>.meta`（z42c 内部行式文本，带 metaVersion/**z42c-fp 编译器语义指纹**/zbc/zpkg 四重版本
pin：源 hash、ns、usedDepNs、模块池原序（hex）、每函数块 label 表（hex）——后两者是 zbc wire
不携带、但参与 STRS 字节的 writer 残留）+ 包级 `package.meta`（上次源清单 + **`deps` 依赖身份**：编译扫描到的
全部依赖 zpkg 的 BLID/内容哈希，任一变化 ⇒ 整包全量；依赖重编但输出不变时 BLID 不变 ⇒ 下游照常命中）。任何 pin 不符/损坏
→ 条目作废按 fresh 处理（宁 fresh 不误命中）。`z42c-fp`（`CacheStore.CompilerFingerprint`）堵住
「源没变 + 格式没 bump 但编译器 codegen/优化/typecheck 变了」的误命中漏洞（bump 纪律见
[version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md#编译器语义指纹非格式失效次元)）。cache 可整目录删除。
indexed 模式（stripped zbc = `<dist>/<rel>.zbc`）自举重写未实现，见
self-hosting.md Deferred。

**调试**：`Z42_INCR_DEBUG=1` 打印失效种子原因（no-entry / hash-diff / src-list）与传播链
（`A invalidated-by B`）。

### incremental-future-tsig-level-invalidation

- **触发原因**：失效边取保守粒度（token ∩ 定义名；被引用文件任何变化即失效引用方），
  过近似只多编不错编；「B 的导出签名（TSIG）不变则不失效 A」的细化需 TSIG 结构化 diff
- **前置依赖**：per-file TSIG 规范化比较（剥全包自由函数泄漏段）
- **触发条件**：实测大包增量命中率低 / 对账器计时显示闭包过宽成为主要成本
- **当前 workaround**：无需——保守边正确性优先

### workspace 增量

workspace 成员（`--workspace`，per-member 与 flat 两种布局）与单工程一样 probe cache；`--no-incremental` 透传到每个成员。
正确性由三道键保证：编译器身份（`CompilerFingerprint` + 格式 Minor，CI `test compiler fingerprint` 守门）、依赖身份
（`package.meta` 的 `deps` 行）、源哈希。自举不动点 gen2 与 CI 两代自举显式 `--no-incremental`。

**成员可见性是封闭的**：编译第 i 个成员时，拓扑序排在它之后的成员（它们的 dist 与 `Z42_LIBS` 里的同名副本）一律不可见，
依赖身份口径相同。否则后序成员上一轮的旧产物会参与编译（依赖索引会因同名方法出现歧义键），构建结果随可见性漂移，
依赖身份也会让前序成员每轮白编一次。

实测（stdlib 25 包，release）：全量 12.4s；无改动 0.6s；z42.core 只改注释 1.3s（24/25 命中）；拓扑序第 5 的 z42.text
改实现 10.5s；靠后的 z42.yaml 改实现 7.2s。各场景增量产物与全量逐字节一致；`xtask test compiler incremental` 含 stdlib 整体读回对账。

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

## 跨包类型签名：从 TYPE/SIGS/IMPL 重建

> zpkg 的类型元数据只有一份**运行时段**（`TYPE` ClassDesc + `SIGS` FuncSig + `IMPL`），VM 执行、反射、
> z42c 跨包解析都读它。`DepScan` 跨包解析走 `TsigReconcile.Rebuild`（从 TYPE/SIGS/IMPL 重建
> ExportedModuleZ），它是跨包类型签名的**唯一来源**；zpkg 不再有独立的 TSIG / EXPT 段。

**机制**（`src/compiler/z42.package/src/TsigReconcile.z42`）：`Rebuild(z, world)` 从 TYPE/SIGS/IMPL
重建 ExportedModuleZ；`world` = 全部相关 zpkg（跨包 base 链：imported 祖先字段/方法从 dep 包 TYPE/SIGS 取）。

**Rebuild 的口径**（镜像 `ExportedTypeExtractor`）：

- **类**：TYPE 每条（跳 interface bit4 / delegate bit6 / enum bit5 / 隐式根 `Std.Object`）→ 裸名
  （剥 ns + 尾部 arity-mangle `$N`）；base 链 topmost-first 展平字段；方法 = Object 四方法（非
  struct）+ 链实例方法合并（override 替换祖先位、`IsVirtual:=false`）+ 本类 static append。
  祖先 SIGS 用 **world 全包 (pkg, mod) 精确定位**（非 ns 首中——同 ns 多模块会错配）。
- **enum**：恒 = 内建 `GCHandleType{Weak,Strong}`（本地 enum 不进跨包签名）。
- **自由函数**：全包 SIGS 中 ns 剥离后**不含 `.`** 的条目（类方法为 `<Class$N>.<m>`——用 `_stripNs`
  保 `.` 判别，勿用剥 `$N` 的 `_bare` 否则 `Foo$1.Bar` 误判为自由函数）；排除 `__static_init__` /
  `__lambda` / `__local_` / `[Native]`。

**IMPL 段**：跨包 `impl Trait for Type` 关联，`Rebuild` 经 `ReadImplInto` 读进 `Impls`——跨包 impl 方法传播 / 反射靠它。`Rebuild` 是编译热路径，每次跨包编译都经它。

---

## L4 — 运行时 Profile

`z42c build` 用 `debug` profile，`z42c build --release` 用 `release` profile。`[profile.<n>]` 本身
**不接受任何直接写的键**，内容只能放进两个子表：

```toml
[profile.release.runtime]      # 运行时旋钮 → 烤进 dist/<name>.runtimeconfig.toml 的 [runtime]
mode = "interp"

[profile.debug.properties]     # 应用自定义配置 → 侧车 [properties]，逐 key 浅覆盖顶层 [properties]
api-endpoint = "http://localhost:8080"
```

| 子表 | 内容 | 详见 |
|------|------|------|
| `[profile.<n>.runtime]` | VM 运行时旋钮；构建期按 VM 登记表校验名字，未知名只警告 | [运行时设置 · 应用侧车](runtime-settings.md#应用侧车) |
| `[profile.<n>.properties]` | 应用属性，运行时经 `Std.Runtime.AppProperties` 只读 | [AppProperties](../stdlib/app-properties.md) |

在 `[profile.<n>]` 下直接写键（`mode` / `optimize` / `debug` / `strip` / `pack` …）是**致命的构建错误**，
报错会指向该放进哪个子表。优化开关写 [`[optimize]`](#l5d--optimize--syntax逐项具名旋钮)；`pack` 写
`[project]`；strip 随 `--release`（见 [L3](#l3--构建产物配置)）。

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

**`path` 的两种形态**（对标 C# 的两种引用）：

| `path` 指向 | 语义 | 对标 |
|---|---|---|
| **工程目录**（其中恰一份工程清单） | z42c 先建该依赖闭包再解析 —— 私有组件跟随工程走 | `<ProjectReference>` / Cargo `{ path = … }` |
| **`.zpkg` 文件** | 已经是产物：**不代建**，直接引用 | `<Reference HintPath="….dll">` |

```toml
[dependencies]
"mylib"     = { path = "../mylib" }                 # 工程目录 → z42c 代建
"vendorlib" = { path = "../vendor/vendorlib.zpkg" } # 已构建产物 → 直接用
```

判据是**扩展名**（`.zpkg`），不是「这个路径上有没有文件」—— 否则把路径写错会被静默当成工程
引用，然后报一句「那里没有工程清单」，一条指向错误方向的诊断。

> **「工程目录」的判据与 `z42c build <dir>` 完全一致**（同一个 `ManifestLocator.FindIn`）：
> 先认裸 `z42.toml`，再认唯一一份 `*.z42.toml`，多份则报歧义并列出候选。
> 所以 `z42 new` 造出来的工程（它写的是**裸 `z42.toml`**）可以直接当 path 依赖。

#### SDK 库：按名声明即可引用编译器域的库

**SDK 库** = 随 SDK 分发、但不在 shipped `libs/` 里的包：编译器域的 `z42.project` / `z42.build` /
`z42.package` / `z42.scripting` / `z42c.*`，住在 SDK 的 `programs/z42c/`。它们**默认不可见**，在
`[dependencies]` 里**按名声明**即开启——不写路径，位置由工具链自己找：

```toml
[dependencies]
"z42c.syntax" = "*"
```

| 事项 | 行为 |
|---|---|
| 谁能看见 | `kind = "exe"` / `"lib"`：**声明了才可见**（连同它们在 SDK 库内的传递依赖）。`kind = "analyzer"`：**自动可见**，免声明（见 [compile-time-extensions.md](compile-time-extensions.md)） |
| 不声明会怎样 | `E0494: 命名空间 … 不存在`，并点名提供它的 SDK 库与声明写法：`它由 SDK 库 \`z42c.semantics\` 提供 —— … "z42c.semantics" = "*"` |
| 部署 | exe：用到的 SDK 库**连同传递依赖**拷进输出目录（runtime 包里没有它们），拷走能跑；lib：不打包，由最终 exe 决定；analyzer / hooks：不拷，由宿主进程（z42c / z42b）提供 |
| 位置怎么找 | 本机编译器目录：`Z42_COMPILER_LIBS` → `Z42_HOME/programs/z42c` → 由 `Z42_PORTABLE_VM` 反推的 SDK 根 → 开发树 |
| stdlib 副本 | `programs/z42c/` 里还有一套 stdlib 副本，对解析**不可见**——stdlib 永远从 `libs/` 解析、不会被拷 |

> ⚠️ SDK 库**不是稳定 API**：编译器内部随版本调整，不承诺兼容。拷进产物后运行期不受 SDK 升级影响，
> 但用新 SDK 重编时可能要跟着改代码。

**不存在 `${compiler_libs}` 路径宏**：写 `{ path = "${compiler_libs}/z42c.syntax.zpkg" }` 会**当场报错**，
并给出等价的按名写法 `"z42c.syntax" = "*"`。依赖的 `path` 不支持任何宏。

产物引用的三条语义：

- 它**所在目录**并入解析域 —— 于是它自己的兄弟依赖也解析得到（把一组 zpkg 一起 vendored
  进同一个目录就能用）；
- zpkg 里的 `[project].name` **必须**与清单里的 key 一致，否则报错。指错文件是最容易犯的错，
  而包名就写在 zpkg 头里，校验零成本；
- **运行期自动随产物走**：vendored 目录不是 shipped `libs/`，所以 exe 构建时会把它复制进
  `dist/` —— 不需要额外声明什么。**它自己的依赖也一起走**：见下「间接依赖」。

**间接依赖**：复制进 `dist/` 的包，它**自己**依赖的包也会被复制进去（递归，直到遇到框架包
或 `deploy = "shared"`）。你只声明自己直接用到的包，不必替别人的依赖操心：

```toml
[dependencies]
"mid" = { path = "../vendor/mid.zpkg" }   # mid 自己依赖 leaf
# 不需要写 "leaf" —— 你的代码里没有它，声明它是替包管理器干活
```

来源是 **zpkg 自己记的依赖表**，不是对方的源码清单 —— 引用一个产物时，对方的 `.z42.toml`
通常根本不在你机器上。

**`deploy` —— 这个依赖运行期从哪儿来**：

```toml
[dependencies]
"bigdata" = { version = "1.0", deploy = "shared" }   # 不复制，运行期从 probing-paths 解析
"z42.io"  = { version = "0.1.0", deploy = "copy" }   # 强制复制进 exe 的 dist（即使它是框架包）
"z42.project" = { version = "*", deploy = "sdk" }     # SDK 库：不复制，运行期从所在 SDK 解析
```

| 值 | 行为 |
|---|---|
| `copy` | 构建期复制进 exe 的 `dist/`，私有、不共享 |
| `shared` | **不复制**，运行期从 [`probing-paths`](runtime-settings.md#probing-paths--依赖的额外搜索目录) 或 `libs/` 解析 |
| `sdk` | **SDK 库专用**：不复制；z42c 在侧车 `probing-paths` 自动补一条 `${Z42_HOME}/programs/z42c`，运行期从**所在 SDK** 解析。用在非 SDK 库上报错 |
| 省略 | 由默认规则决定：**从 shipped `libs/` 找到的不复制**（框架），从别处找到的复制（私有）|

> ⚠️ `deploy = "sdk"` 让程序**依赖目标机器装了 SDK**，且运行期用的是**那台机器 SDK 里的版本**，不是编译时那份——
> SDK 库不是稳定 API，版本不同就可能出问题。它适合「和特定 SDK 一起发布、在该 SDK 上运行」的工具；普通应用用默认
> （复制）最稳。只装了 runtime 的机器上运行会失败，报错附「是否没有安装 z42 SDK？」。

只接受这三个值，写错（`Copy` / typo）**报错** —— 否则会被当成未声明静默走默认规则。校验在编译**之前**
跑，对**所有 `kind`** 生效。

`deploy` 在两个地方**没有意义、写了报错**：

| 写在哪 | 为什么 |
|---|---|
| `kind = "analyzer"` 工程的 `[dependencies]` | 编译期扩展永不链入运行期产物，它的依赖也就没有「部署到哪」 |
| 任何工程的 [`[analyzers]`](#analyzers--加载进编译器编译期运行不链入产物) 条目 | handler zpkg 只加载进编译器进程、编译期运行，永不随产物走 |

（两处都与 `[dependencies]` 共用同一套条目语法，所以键写得出来 —— 报错是为了不让它静默无效。）

> `shared` **不要求那个包此刻存在于任何地方** —— 它的解析是运行期的事。构建期不校验
> 「运行期够不够得着」：那需要把 VM 的 probing-paths 展开规则（相对 entry、通配符、去重）
> 在构建侧重做一遍，两份规则必然漂移。

**设计原则：命名空间与包名解耦**

`[dependencies]` 中填写的是 **zpkg 的 `[project] name` 字段**，而非命名空间名称。编译器在 libs/ 搜索路径中找到对应 zpkg 后，读取其 `namespaces` 字段，将导出的命名空间注册为可用。

```
[dependencies] "my-http" = "*"
  → 编译器在 libs/ 找 name="my-http" 的 .zpkg
  → 读该 zpkg 的 namespaces: ["Http", "Http.Client"]
  → 源码中 using Http; / using Http.Client; 均可解析
```

这意味着 `using` 语句中的命名空间名称与 `[dependencies]` 中的包名**无需一致**，由 zpkg 自身的 manifest 决定。

**stdlib 自动可用，永不声明（Rust-std 模型）：**

标准库（`Std.*` 命名空间 / `z42.*` 包）跟工具链一起分发，**始终可用，无需在任何 manifest section 声明**——就像 Rust 从不在 `Cargo.toml` 写 `std`，`use std::...` 直接可用。机制：编译器对 `meta.Name` 以 `z42.` 开头的包**无条件可见**（`ScanLibsForNamespaces` / `BuildDepIndex` 的 isStdlib 旁路），与是否声明无关；版本跟工具链走。

由此确立的约定：

- **`[dependencies]` / `[tests.dependencies]` / `[benches.dependencies]` 只用于第三方依赖。** stdlib（`z42.*`）出现在其中纯属冗余。
- **声明 `z42.*` 不报错也不警告**，只是冗余。
- **第三方包漏写会报 [`E0497`](../appendix/error-codes.md)**（编译期）——这是本节约定里
  真正有执行的那一半。
- **`Std.*` 命名空间保留（E0605，硬错误）**：非 `z42.*` 包在源码声明 `namespace Std.*`（或裸 `Std`）→ **编译错误**。`Std` / `Std.*` 专属官方 stdlib（同 Rust 保留 `std`/`core`/`alloc`），保证程序里任何 `Std.*` 一定解析到官方、自动可用的 stdlib，永不被第三方 shadow。（消费一个已构建的、占用 `Std.*` 的第三方 zpkg 时另有 W0603 warning 作软网。）

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
# Cascade defaults — dist = ./dist, cache = ./artifacts/<profile>/.cache.
# Override only when needed:
#   dist_dir = "/build/hello"
#   cache_dir = "/dev/shm/hello-cache"

[dependencies]
"my-utils" = "*"

[profile.release.runtime]
mode = "interp"
```

### 依赖必须无环（No Circular Dependencies）

**硬规则**：z42 包之间的依赖关系**必须形成有向无环图（DAG）**。任何包不得直接或间接依赖自身。该约束在所有依赖层级一致生效，没有运行时延迟 import 等后门。

**适用范围与强制状态**：

| 层级 | 约束 | 当前强制 |
|------|------|----------|
| zpkg `[dependencies]` 之间 | A 依赖 B → B 不得（直接或传递）依赖 A | 🔄 编译期解析时检测（错误码待 RFC，建议 `E0610 CircularPackageDependency`） |
| Workspace member 之间 | 同上，DFS 三色检测 | ✅ `WS006 CircularDependency`（见 [error-codes.md](../appendix/error-codes.md)） |
| stdlib 包之间 | 同上，且 `z42.core` 在所有库之下 | ✅ 约定（无固定层级，只要求无环） |

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
- ❌ 新旧 zpkg 共存 + 灰度迁移以"绕开"循环 —— pre-1.0 不留兼容（见 [philosophy.md "不为旧版本提供兼容"](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/philosophy.md#不为旧版本提供兼容)）

**编译器报错要求**（待实现时遵守）：

- 在依赖闭包解析阶段构建依赖图，发现环立即终止，**不进入 TypeChecker**
- 错误信息**必须列出完整环路**（如 `A → B → C → A`），并标注每条边来自哪个包的 `[dependencies]` 字段
- 同一构建中存在多条独立环 → 各报一次，不合并

> **同包内不受此规则约束**：单个 zpkg 内部的文件 / 类型 / 函数互相引用是允许的（同 Rust crate 内 module、Java 同 package、Swift 同 module）。本规则的最小切割单位是 **zpkg（分发单元）**。

---

## L5b — 测试 / Bench / Example 目标配置

> 设计参照 Cargo target 模型（`[[test]]`/`[[bench]]`/`[[example]]`
> + auto-discovery），按 z42 自举子集精简。

声明 test / bench / **example** 运行目标的位置、驱动方式、共享依赖、产物布局。设计原则：**约定优先
（glob 批量发现）+ 显式覆盖（`[[target]]` 具名）+ dev-deps 隔离**。三类目标结构同构，共用模型
（`RunTarget` / `TargetSection`，见 `src/compiler/z42.project/`）。

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
├── benches/                            ← 与 tests/ 同构
│   └── lexer_throughput.z42
└── examples/                          ← 与 tests/ 同构（默认只编不跑，见下）
    └── hello.z42
```

**约定规则**：

1. `tests/*.z42` 顶层文件 → 各自独立目标（auto 名 = 文件 stem）
2. `tests/<name>/source.z42` 入口 + 同目录递归 `*.z42` → 合成一个多文件目标（auto 名 = 目录名）
3. `benches/*` 与 `examples/*` → 同 1/2 规则（默认发现 dir 分别为 `benches/` `examples/`）
4. 子目录内非 `.z42` 文件（fixture / data）随产物打包，运行时 cwd 切到 `<dir>`，相对路径读取
5. `_` 前缀的 `.z42` 文件是 dir-mode 内的辅助；不是目标入口
6. **发现循环必须先按稳定键 sort** 再注册（[common-pitfalls §1](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/common-pitfalls.md)——
   first-wins 禁止依赖 FS 枚举序）
7. `tests/fixtures/`（`benches/`、`examples/` 下同名目录同理）是**保留目录**：约定发现不从里面认领目标，
   「目录里有 `.z42` 源却一个目标都没解析出来」的检查也跳过它。放由外部脚本驱动、先构建再比对的
   夹具工程（自带 `z42.toml` 的多包工程、字节基线等）

### `[tests]` / `[benches]` / `[examples]` 段

共享配置 + dev-deps 隔离（Cargo `[dev-dependencies]` 等价物）。**段名一律复数**——与单数
`[[test]]`/`[[bench]]`/`[[example]]` array-of-tables key 区分开（同名 key 既是 table 又是 AoT 是
非法 TOML）。

```toml
[tests]
# 字段全可省 → 走约定：tests/*.z42（**一文件一单元**，仅此一条）
# include = ["tests/*.z42"]
# exclude = ["tests/_skip/*"]
# auto    = true          # false → 关闭约定扫描，只认 [[test]]
#
# **目录单元必须显式配**，约定不认领它：
#   include = ["tests/*.z42", "tests/secp256k1/**/*.z42"]   # → 单元名 `secp256k1`，整目录一起编
# 为什么不默认扫 `tests/*/source.z42`：本仓 `<lib>/tests/` 下同时住着反射 [Test] 用例和 VM
# golden 用例（`main()` + `expected_output.txt`），**两者都长成 `<name>/source.z42`，glob 分不开**
# （要分得靠扫目录里有没有 `[Test]` 标注，那不是 glob 能表达的）。猜错的代价是把 golden 目录
# 当测试目标编，编不过或跑出零用例 —— 所以不猜。
[tests.dependencies]
"z42.test" = "0.1.0"      # 仅测试合入；release zpkg 元数据不含

[benches]
[benches.dependencies]
"z42.test" = "0.1.0"      # Bencher 在 z42.test 包内

[examples]
# 默认发现 examples/*.z42（同上：目录单元显式配）
```

> **源文件清单统一叫 `include`**——`[sources]` / `[tests]` / `[benches]` / `[examples]` 段与
> `[[exe]]` / `[[test]]` / `[[bench]]` / `[[example]]` 数组**全部同一个键名**。选 `include` 而非
> `sources` 是因为它自带搭档 `exclude`，数组形式同样可以排除文件。

### `[[test]]` / `[[bench]]` / `[[example]]` 数组（显式覆盖）

字段对齐 `[[exe]]`（`entry` = FQ 函数名，`sources` = glob 集）：

```toml
[[test]]
name    = "compile_perf"          # 必填；filter 用 + 合成包名
harness = false                   # 默认 true（反射）；false → 自带 Main 退出码判定
entry   = "Perf.Runner.Main"      # harness=false 必填（FQ 函数名）
include = ["tests/perf/*.z42", "tests/perf/_lib/*.z42"]  # 可选；省略=沿用约定单元文件集
[test.dependencies]               # 该 target 独享 dev-dep（三层合并优先级最高）
"z42.compression" = "0.1.0"

[[example]]
name = "streaming"
entry = "Ex.Streaming.Main"
test = true                       # 破例纳入 xtask test 执行（默认 example 只编不跑）
```

### example 的执行语义（借 Cargo）

- `xtask test`（targets stage，`xtask test toolchain builder` 单跑）：**编译**所有 example 当门禁（确保永远编得过），**默认不执行**。
- 目标写 `test = true` → 纳入该 stage 执行（编 + 跑，退出码判定）。
- 注意与仓库根 `examples/`（学习手册配套示例，`xtask test docs examples`）无关。

### 具名选择运行

`xtask test toolchain builder <name>` / `xtask bench targets <name>` 只跑一个
（裸 `test`/`bench` 是全量 gate / e2e 默认动作，故 test/bench 走 `targets <name>` 子动作）。名不存在
→ 报错列出可用目标名，非零退出（不静默）。**注**：自定义段 `include` glob 运行期暂只扫约定目录
（`tests/`·`benches/`·`examples/`）。

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

测试 / bench 产物在每个 package 的 `output_dir` 下并列两个独立子树，与 L3 [build] 的 `output_dir` / `cache_dir` / `dist_dir` 三字段模型对齐：

```
artifacts/build/libraries/<lib>/<profile>/
├── cache/                          ← 生产中间产物
├── dist/<lib>.zpkg                 ← 生产分发产物（只放生产 zpkg）
├── tests/                          ← 测试子树
│   ├── cache/<test_name>/          ← 每测试独立 cache
│   └── dist/                       ← 测试可执行
│       ├── <lib>.test.<name>.zbc               ← 单文件（emit-zbc 路径；runner 直接吃 .zbc）
│       └── <lib>.test.<dir_name>.zpkg          ← dir-mode（合成 manifest → z42c build → packed zpkg）
└── benches/                         ← bench 子树（与 tests 同构）
    ├── cache/<bench_name>/
    └── dist/
        ├── <lib>.bench.<name>.zbc              ← 单文件
        └── <lib>.bench.<dir_name>.zpkg         ← dir-mode
```

> 单文件单元走轻量 `z42c --emit zbc` 产 `.zbc`；dir-mode 单元(多文件)合成 mini-manifest 跑 `z42c build` 产 packed `.zpkg`。两者都由 z42b（z42.builder.zpkg）经 TIDX 发现 + 调度，落同一 `<subtree>/dist/`。

**zpkg 命名硬约束**：`.test.` / `.bench.` infix 是文件名硬规则（也是 CI 守门正则的 anchor）。`tests_dir` / `bench_dir` 字段**不暴露** — 强制 `<output_dir>/tests/` 和 `<output_dir>/benches/`；改路径走 `output_dir`，两子树一并变。

### xtask 命令 ↔ 目录

| 命令 | 写入 | 读取 deps |
|------|------|----------|
| `./xtask test stdlib [lib]`  | `<lib>/<profile>/tests/{cache/<unit>,dist}/` | `[dependencies]` + `[tests.dependencies]` |
| `./xtask bench stdlib [lib]` | `<lib>/<profile>/benches/{cache/<unit>,dist}/` | `[dependencies]` + `[benches.dependencies]` |
| `./xtask test toolchain builder <name>` / `bench targets <name>` / `example <name>` | 同上（具名单目标）| 三层合并 |
| `./xtask clean`              | 删每个 `<lib>/<profile>/{cache,dist}` + 扁平视图 `intermediate/libraries/flat/`（**保留** tests/benches） | — |
| `./xtask clean tests`        | 删每个 `<lib>/<profile>/tests/` | — |
| `./xtask clean bench`        | 删每个 `<lib>/<profile>/benches/` | — |
| `./xtask clean all`          | 删整个 `artifacts/build/`（全量重置） | — |

`bench`（无 `stdlib` 子参）仍是 e2e hyperfine 场景跑器，与 per-lib micro-bench 分流。[Benchmark] 单元由 z42b 与 [Test] 同调度（zero-arg 调用 + Bencher 采样）。

### 清单校验（构建期，**不是诊断码**）

写错 dev-target 声明会被拦下。校验在 **xtask 发现层**做（`ManifestLoader` 只忠实解析、不校验
语义），报的是构建工具的错误行、**不走 `E`/`WS` 诊断码** —— 下表是实际会打印的文案：

| 规则 | 实际报错 | 实现 |
|------|---------|------|
| `[[test]]` / `[[bench]]` / `[[example]]` 缺 `name` | `[[test]] #1 missing required \`name\`` | `_validateRunTargets` |
| `harness = false` 的目标缺 `entry`（反射目标 harness=true 无需 entry）| `[[test]] 'x' has harness=false but no \`entry\`` | 同上（example 豁免——它恒按 `entry` 跑 Main）|
| 同一 kind 内 `name` 重复 | `duplicate [[test]] name 'x'` | 同上 |
| 目标 `include` glob 无匹配文件 | example / `harness=false` 路径：`✗ <目标>: no source files match (sources glob / convention empty)`；test / bench 转发 z42b 的路径：`compile failed: no .z42 sources under <dir>` | `_compileTarget` / z42b |

三类 kind 的命名 namespace **独立** —— `[[test]] name = "x"` / `[[bench]] name = "x"` /
`[[example]] name = "x"` 可共存（重名只在同一 kind 内判）。auto 与显式撞名以显式为准，不报错。

> 四条都是**干净的错误退出**（非零退出码 + 一条说明），不是未捕获异常。

> 📌 这几条规则**不用** `WS` 诊断码（`WS012` / `WS040`–`WS043` 码号不存在，另见
> [错误码全表](../appendix/error-codes.md)的 WSxxx 节），由 xtask 侧独立实现（上表）。
>
> 「test-only dep 出现在 `[dependencies]`」**不做校验**：这类判据只能靠按名字写死的 curated set
>（`{ "z42.test" }`）加 `.test.` / `.bench.` infix 豁免，而 `z42.test` 是个普通的运行期库、
> **无法自证**自己"只该在测试里出现"——机制化不了。dev-dependency 的正确表达是
> `[tests.dependencies]` / `[benches.dependencies]`（三层合并，见上文）。

---

## L5c — `[analyzers]` / `[lints]`：编译期扩展与诊断开关

完整用法（怎么写一个 analyzer、契约长什么样、`--fix` / `#suppress`）见
[编译期扩展（Analyzer / Generator）](compile-time-extensions.md)。本节只列**清单字段**。

### `[analyzers]` —— 加载进编译器、编译期运行、**不链入产物**

```toml
[analyzers]
"demo.noemptycatch" = "0.1.0"              # 按名：在依赖目录找 demo.noemptycatch.zpkg
"demo.gen"          = { path = "../gen" }  # 按路径：z42c 代建该工程，取其 dist
```

值的写法与 `[dependencies]` 相同，**语义不同**：

| | `[dependencies]` | `[analyzers]` |
|---|---|---|
| 何时用 | 编译目标代码时解析符号 | 加载进编译器、编译期执行 |
| 进不进产物 | 进 | **不进** |
| `path = "..."` | 支持，z42c 代建整个**闭包** | 支持，z42c 代建**那一个工程**（它的依赖由它自己解析）|
| 被引工程的 `kind` | `lib`（引 `analyzer` 会被拒绝）| 必须是 `analyzer`（引普通库会被拒绝）|

**path 条目**：指向目录，其中须恰有一份**工程清单**（判据同
`[dependencies]` 的 path：裸 `z42.toml` 优先，否则唯一一份 `*.z42.toml`），且
`[project].name` 与这里写的名字一致。z42c 会定位它 → 校验 `kind = "analyzer"` → **代为构建** →
把产出的 zpkg 交给 generator/analyzer 引擎。改了扩展的源码，消费方下次构建会重编（handler
指纹含该 zpkg 内容）。

> **代建产物不进消费方的解析域**：与 `[dependencies]` 的 path 闭包刻意不同 —— handler 只活在
> 编译器进程里，把它的 dist 并进依赖目录就等于让编译期扩展对运行期代码可见。

**双向校验**：`kind = "analyzer"` 的工程写进 `[dependencies]` → 报错（它永不链入产物，运行期不会
到场）；非 `analyzer` 工程写进 `[analyzers]` → 报错（否则失败模式是「加载成功、发现 0 个 handler、
什么都不做」的静默空转）。两条都只在 **path 条目**上判得出来 —— 按名引用时手上只有 zpkg，而 zpkg
不记 `kind`。

同一个段**覆盖 analyzer 与 generator 两类**：z42c 从列出的每个 zpkg 里同时寻找
`: Analyzer` / `: Generator` / `: ModuleGenerator`。

> ⚠️ 开发态构建产出的是 indexed zpkg（主文件 + 旁边散装 `.zbc`）。只拷主文件过去，加载时报
> **E0493**；连 `.zbc` 一起拷，或用 `--release` 得到单文件 packed zpkg。

### `[lints]` —— 逐规则 severity 覆盖

```toml
[lints]
DEMO001            = "error"     # warning → error：编译失败、不产产物
"webgen.*"         = "none"      # 支持通配前缀
warnings_as_errors = true        # 特殊布尔键（不是规则名）
```

键是规则 ID，值是 severity 串。**精确 ID 优先于 `pkg.*` 前缀通配**。
`warnings_as_errors` 是**保留的布尔键**，不当规则名解析。
旧拼写 `warnings-as-errors` 已删除，写了按未知键报错。
`DiagRule.EnabledByDefault` 为 `false` 的规则默认不报，要在这里显式打开。

接受的 severity 串只有五个：

| 值 | 含义 |
|---|---|
| `"none"` | 抑制，不报 |
| `"hidden"` | 不显示，仅供 `--fix` 消费 |
| `"info"` | 建议 |
| `"warning"` | 警告 |
| `"error"` | 编译失败、不产产物 |

写错的 severity 串（`DEMO001 = "eror"`）、规则键写成非串值、`warnings_as_errors` 写成非布尔，都按清单错误
报出（与未知键同一通道，exit 2），不会被静默当成「无覆盖」。规则**名**不校验——analyzer 的规则表在编译期
才加载，写一个没有任何 analyzer 声明的规则名不报错。

---

## L5d — `[optimize]` / `[syntax]`：逐项具名旋钮

两段同形：**逐项 bool**，键是名字、值是开关。manifest 只搬运中性 name/value 对，
**解释权在 z42c**（`z42.project` 不认识这些名字的语义）。两段都**参与包级缓存身份**，
所以只改 toml、不碰源码也会触发重编 —— 否则旋钮会「全量生效、增量被忽略」。

### `[optimize]` —— 逐 pass 优化开关

```toml
[optimize]
inline    = true
const-fold = false
```

| 事项 | 说明 |
|---|---|
| 已知名 | `const-fold` / `copy-prop` / `dce` / `inline` / `cse` / `licm` / `stack-alloc` / `loop-alloc-reuse` / `readonly-load` / `pure-call` / `dead-branch` / `devirt`，外加 `all` / `none` |
| 优先级 | **CLI (`--opt` / `--no-opt`) > `[optimize]` > profile 默认**（release=全开、debug=全关）|
| 未知名 | **报错退出**，不静默忽略（与 CLI 侧 `--opt 乱写` 同一口径）|

### `[syntax]` —— 语法特性开关

关掉某个特性后，用到该语法的代码报 **E0301**。

```toml
[syntax]
control_flow  = false     # 关掉 if / while / for / foreach / do / switch + break / continue
exceptions    = false     # 关掉 try + throw
bitwise       = false     # 关掉 | ^ & << >>
ternary       = false     # 关掉 ?:
pattern_match = false     # 关掉模式（`x is 1` / `case 1:` / switch 表达式 / 解构声明）
using_stmt    = false     # 关掉 `using` **语句**（import / 别名**指令**不受影响）
```

| 事项 | 说明 |
|---|---|
| 未知名 | **报错退出**并列出已知名单，不静默忽略 |
| 默认 | 不写本段 = `Phase1Profile`（C# 12 子集全开）|
| 粒度 | 一般是**整个语法构造**；`pattern_match` 例外，它只关「进模式引擎」的那几条路 —— **`x is T` / `x is T v` 是类型测试，仍然可用** |
| 缓存 | 特性集折进包级缓存身份（`depsId`）⇒ 只改 toml 不碰源码也会重编，不会「全量生效、增量被忽略」|

> ⚠️ **上面这 6 个是今天真的关得掉东西的全部。**
> `LanguageFeatures` 里还有 10 个名字（`oop` / `generics` / `lambda` / `tuples` / `delegates` /
> `reflection` / `nullable` / `cast` / `arrays` / `interpolated_str`）—— 它们已登记、可以写进
> `[syntax]` 而不报「未知名」，但**关掉它们不会挡住任何语法**。这是有意暴露的现状而不是承诺：
> 后续接线见 `docs/internals/src/compiler/syntax-customization.md` 的「实施路径」。
>
> ⚠️ **`pattern_match = false` 的连带后果**：`switch` **语句**本身归 `control_flow`（仍解析），
> 但它的每个 `case` 会报 E0301 ⇒ 实际不可用；只有 `default:` 分支不受影响。

---

## L6 — 工作区（Workspace）

管理多工程 monorepo，统一构建与产物布局。

### L6.1 文件名与角色

| 文件 | 数量 | 角色 |
|---|---|---|
| `z42.workspace.toml` | workspace 根目录唯一一份 | virtual manifest：成员清单 + 产物布局 |
| `<name>.z42.toml` | 每个 member 一份 | member 自身配置（普通工程清单） |

`z42.workspace.toml` 是 **virtual manifest**：顶层只认 `[workspace]`（及其子表 `build` / `dependencies`）。
写 `[project]`、`[profile.*]` 或其它任何段都按[未知键](#键校验本页没列的键一律报错)报错。

### L6.2 顶层结构

```toml
# z42.workspace.toml — virtual manifest

[workspace]
members         = ["libs/*", "apps/*"]   # glob 与显式路径混用
exclude         = ["libs/sandbox-*"]     # 从 glob 结果排除
default_members = ["apps/hello"]         # 默认成员子集

[workspace.dependencies]                 # 表形式只认 version / path / deploy
"my-utils" = { path = "libs/my-utils", version = "0.1.0" }

[workspace.build]                        # 集中产物（见 L6.5）；整段可省略
output_dir = "artifacts/${project_name}/${profile}"
```

| 键 | 说明 |
|---|---|
| `members` | 成员目录（glob / 显式路径），缺省 `["*"]` |
| `exclude` | 从展开结果中剔除的成员路径 |
| `default_members` | 默认成员子集。**z42c 目前不消费它**（`z42c build` / `--workspace` 都编全部成员）；仓内 xtask 用它推导成员列表。旧拼写 `default-members` 已删除，写了按未知键报错 |
| `dependencies` | 中央依赖声明，条目写法同 `[dependencies]`。当前只解析与校验键名，构建不消费——成员仍在自己的 `[dependencies]` 里声明依赖 |
| `build` | 只接受 `output_dir` / `cache_dir`，见 [L6.5](#l65-workspacebuild-集中产物) |

**成员发现规则**（z42c `build --workspace`）：

- `members` 逐条展开，缺省等价于 `["*"]`。路径相对 workspace 根，段内可用 `*` / `?` 通配（`libs/*`）。
- **显式路径**（不含通配）必须存在且目录里恰有一份清单，否则构建报错；**通配**展开出的目录没有清单就跳过
  （`["*"]` 会扫到 `artifacts/` 这类目录）。
- 成员目录里的清单可以是 `<name>.z42.toml`，也可以是裸名 `z42.toml`（`z42 new` 生成的形态）；同一目录多份
  `*.z42.toml` 报错（WS005）。
- `exclude` 按成员相对根的路径做段级 glob（`**` 跨段，`*` / `?` 段内），从展开结果里剔除。
- 成员间依赖成环直接报错。

### L6.3 Members 展开

```toml
members = ["libs/*", "apps/main", "experiments/foo"]
exclude = ["libs/sandbox-*"]
```

- glob 仅匹配**目录**，目录内必须恰好一份 `*.z42.toml`（多份 → `WS005`）
- 显式路径与 glob 可混用
- exclude 优先于 members

---

### L6.4 z42c workspace 模式

`z42c build` 只有两种入口形态（全部选项见 `z42c build --help`）：

| 写法 | 行为 |
|---|---|
| `z42c build <manifest>` | 单工程：只编这一个清单（依赖按 `[dependencies]` 解析，path 依赖闭包先建） |
| `z42c build`（不给清单） | 从 CWD 向上找**最近的** `z42.toml` 或 `z42.workspace.toml`：找到工程清单 → 同上；找到 workspace 清单 → 同 `--workspace` |
| `z42c build --workspace` | 按拓扑序编**全部**成员（`members` 展开结果；`default_members` 不参与，见 L6.2） |

```bash
z42c build                      # 最近的清单（工程 → 单工程；workspace → 全部成员）
z42c build --workspace          # 全部成员，拓扑序
z42c build libs/core/core.z42.toml   # 只编一个成员（及其 path 依赖闭包）
z42c build --release            # release profile
z42c build --workspace --output-dir out   # flat 产物：全部成员写进同一个目录
```

#### 拓扑编译顺序

```
core ← utils ← hello
```

`z42c build --workspace` 编译顺序：先 `core`，后 `utils`，最后 `hello`（成员之间串行；成员内部按 `--jobs` 并行）。
**任一成员失败即停止**（打印 `z42c build --workspace: member build failed: <清单>`，返回该成员的退出码），后面的成员不再编译。

#### 规划期错误

规划期（还没编任何成员）发现的问题一律报错退出、一个成员都不建：

| 情况 | 输出 |
|---|---|
| workspace 清单有不认识的键 | `<z42.workspace.toml>: unknown key …`（见[键校验](#键校验本页没列的键一律报错)） |
| 显式成员目录不存在 | `workspace: member \`<pat>\` does not exist under <dir>` |
| 成员目录里有多份清单 | `… has more than one manifest (WS005): …` |
| 成员间依赖成环 | `z42c build --workspace: circular member dependency` |

> ⚠️ **两个成员声明同一个 `[project] name` 目前不报错**（文档曾列的 WS001 未实现）——产物按包名落盘，后建的覆盖先建的。

跨 member 依赖的构建拓扑由 `src/compiler/z42c.pipeline/tests/workspace_topo/` 覆盖。

#### 新建 / 清理（编排器 `z42` / `z42b`，不是 z42c）

```bash
z42 new hello                   # 在当前目录（或 --path 指定的目录）下建 hello/：z42.toml + .gitignore + README + src/；--lib 建库
z42 clean [<manifest|dir>]      # 删工程构建产物（debug + release 的 dist / cache / generated；
                                # output_dir 未显式配置时整个 <工程>/artifacts/）
```

> z42c 是纯编译器（`build` / `--emit-zbc` / `--dump-*`）；产物生命周期（clean）与 test / bench 一样归编排器。
> 文档曾列的 `-p` / `--exclude` / `--no-workspace`、`z42c check` / `info` / `metadata` / `tree` / `lint-manifest` /
> `new --workspace` / `init` / `fmt` 均**未实现**，已从本页删除。

---

### L6.5 `[workspace.build]` 集中产物

workspace 模式下，所有 member 产物**集中**到 workspace 根下的 `artifacts/` 子树：

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
    │       └── publish/         ← [platform.desktop].publish_dir (默认 ${output_dir}/publish)
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
# 省略即等价于以下设置：
# output_dir  = "artifacts/${project_name}/${profile}"
# cache_dir   = "${output_dir}/.cache"    (+ member 子目录防碰撞)
# 成员 dist = ${output_dir}/dist；成员 publish = ${output_dir}/publish（[platform.desktop].publish_dir 未配时）

output_dir = "out/${project_name}/${profile}"   # 模板示例：按成员名 + profile 分流
```

`[workspace.build]` 只接受 `output_dir` / `cache_dir`；成员的 dist / publish 目录跟随 `${output_dir}`，
不能在 workspace 层单独设置（写 `dist_dir` / `publish_dir` 报未知键）。

---

### L6.6 Member 清单

Member 清单就是普通工程清单（L1–L5 的全部键都可用，包括 `[profile.<n>.runtime]` /
`[profile.<n>.properties]`）。工作区专属的 `[workspace]` 写在 member 清单里按未知键报错。

### L6.7 路径模板变量

`[build]` 与 `[workspace.build]` 的目录字段（`output_dir` / `cache_dir` / `dist_dir` / `generated_dir`）
支持 `${...}` 模板，变量表见 [L3](#l3--构建产物配置)。`$$` 写字面 `$`；未知变量与未闭合的 `${`
原样保留、不报错。其它字段不展开模板。

```toml
[workspace.build]
output_dir = "artifacts/${project_name}/${profile}"   # 展开 → artifacts/hello/release
```

### L6.8 配置生效顺序

```
最终 member 配置：

1. member 自身 *.z42.toml 字段
2. 产物目录：member 未配 output_dir / dist_dir 时取 [workspace.build]（见 L3「workspace 成员继承规则」）
3. CLI flag（--release / --no-incremental 等）                                         (最终覆盖)
```

### L6.9 错误码

WSxxx 码见[错误码全表](../appendix/error-codes.md)（该组目前整体未接线）；清单键错误不走诊断码，
见[键校验](#键校验本页没列的键一律报错)。

### L6.10 目录结构样板

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

面向用户的工作区教程与可运行示例见学习手册「依赖与工作区」一章（`examples/engineering/workspaces/`，该章节尚未提供）。

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

下面是清单里**能写的全部键**；没列出的键报错（见[键校验](#键校验本页没列的键一律报错)）。

```toml
# ── 工程清单 <name>.z42.toml ───────────────────────────────────────────────
[project]
name        = "my-app"          # 必填
version     = "0.1.0"           # 必填，SemVer
kind        = "exe"             # exe | lib | analyzer；多目标用 [[exe]] 时省略
entry       = "MyApp.Main"      # 可选；省略时自动发现 Main
pack        = false             # 可选；省略 → debug indexed / release packed
description = ""                # 可选，纯元数据
authors     = []                # 可选，纯元数据
license     = "MIT"             # 可选，纯元数据（SPDX）

[sources]
include = ["src/**/*.z42"]      # 默认值
exclude = []                    # 默认值

[build]                          # 全部可选，未设走级联默认（见 L3）
# output_dir    = "/build/myproj"
# cache_dir     = "/dev/shm/myproj"
# dist_dir      = "/build/myproj/dist"
# generated_dir = "${output_dir}/generated"
# hooks         = "build"         # z42b 编排读，z42c 不消费
incremental = true              # 默认 true

[[exe]]                          # 多可执行目标；与 [project] kind = "exe" 二选一
name    = "tool"
entry   = "Tool.Main"
include = ["src/tool/**/*.z42"] # 可选，覆盖 [sources]

[dependencies]
# "pkg-name" = "*"                                       # 按名：zpkg 包名
# "pkg-name" = { version = "1.0", path = "...", deploy = "copy" }   # 表形式只认这三个键
# stdlib 无需声明

[analyzers]                      # 编译期扩展 zpkg；写法同 [dependencies]，不接受 deploy
"demo.noemptycatch" = "0.1.0"

[lints]                          # 键是规则 ID，值是 severity：none|hidden|info|warning|error
DEMO001            = "error"
warnings_as_errors = true       # 保留布尔键

[optimize]                       # 逐 pass 开关，名字见 L5d
inline = true

[syntax]                         # 语法特性开关，名字见 L5d
exceptions = false

[properties]                     # 应用自定义配置（键自由）
app-name = "demo"

[profile.release.runtime]        # 运行时旋钮（见 L4）
mode = "interp"
[profile.debug.properties]       # 按 profile 覆盖 [properties]
app-name = "demo-dev"

[tests]                          # [benches] / [examples] 同形（见 L5b）
include = ["tests/*.z42"]
exclude = []
auto    = true
[tests.dependencies]
"z42.test" = "0.1.0"

[[test]]                         # [[bench]] / [[example]] 同形（见 L5b）
name    = "compile_perf"
harness = false
entry   = "Perf.Runner.Main"
include = ["tests/perf/*.z42"]
test    = false                 # 仅 example 有意义
[test.dependencies]
"z42.compression" = "0.1.0"

[native.mylib]                   # 本包携带的私有 native 库（逻辑名 mylib）
dir = "native"                  # 预编译库基目录（相对清单），按 <dir>/<rid>/ 定位

[platform.desktop]               # 及 ios / android / wasm，见下节
apphost = true

# ── 工作区清单 z42.workspace.toml（顶层只认 [workspace]）──────────────────────
[workspace]
members         = ["libs/*", "apps/*"]
exclude         = []
default_members = []

[workspace.dependencies]         # 表形式只认 version / path / deploy
# "pkg-name" = { path = "...", version = "0.1.0" }

[workspace.build]                # 只认这两个键
# output_dir = "artifacts/${project_name}/${profile}"
# cache_dir  = "${output_dir}/.cache"
```

## `[platform.*]` 平台配置段

`z42c` 只校验这些段的键名、不消费；由 `z42 export` / `z42 publish` 消费。各子表接受的键：

| 段 | 键 |
|---|---|
| `[platform.desktop]` | `apphost` / `publish_dir` / `icon` / `bundle_id` / `bin` / `payload` / `link` |
| `[platform.ios]` | `bundle_id` / `display_name` / `version` / `min_ios` / `team_id` / `device_families` / `capabilities` |
| `[platform.android]` | `app_id` / `display_name` / `version_code` / `version_name` / `min_sdk` / `target_sdk` / `permissions` |
| `[platform.wasm]` | `title` |

### `[platform.ios]`

```toml
[platform.ios]
bundle_id      = "com.example.myapp"   # required: CFBundleIdentifier
display_name   = "My App"             # optional: CFBundleDisplayName（默认 = project name）
version        = "1.0.0"             # optional: CFBundleShortVersionString（默认 = project version）
min_ios        = "16.0"              # optional: IPHONEOS_DEPLOYMENT_TARGET（默认 "16.0" = SDK 的 platform.ios.min_ios）
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
target_sdk   = 37                   # optional: targetSdk（默认 37 = Android 17；compileSdk 固定为 37）
```

### `[platform.wasm]`

```toml
[platform.wasm]
title = "My App"   # optional: HTML &lt;title&gt;（默认 = project name）
```

### `[platform.desktop]`

```toml
[platform.desktop]
apphost     = true   # GATE：唯有 apphost = true，`z42 publish <toml> --rid <desktop-rid>` 才产 apphost。
                     # 缺省 / false → publish 报 "not configured to publish a desktop apphost" 并退出。
publish_dir = ".."   # 仅输出位置（部署根，相对 toml 所在目录，同 [build].output_dir 基准）。
                     # 不充当 gate；缺省 = ${output_dir}/publish（对齐 [build] 目录默认，
                     # output_dir 未设→workspace 继承→<项目目录>/publish）。
                     # --output 可覆盖。
# 部署布局（可选）：apphost 二进制与 payload zpkg 在
# 部署根（publish_dir）下的相对路径。缺省 → 扁平：apphost = publish_dir/<name>，
# payload 原地内嵌（不复制）。
bin     = "bin/myapp"                 # apphost 二进制落点（相对部署根）
payload = "programs/myapp/myapp.zpkg" # payload zpkg 落点；publish 把已编译 zpkg 复制到此
```

桌面平台的输出是 **apphost**（per-app 原生可执行）：`z42 publish <toml> --rid <desktop-rid>` 在
`apphost = true` 时，读 `publish_dir`（部署根）+ 从 `[build]`/`[project]` 推出已编译 zpkg，patch 原生 apphost
stub 产出 exe。与 ios/android/wasm export 对称——apphost 不是独立命令。

> **部署布局 `bin` / `payload`**：apphost 应用天生两部分——原生启动器
> 二进制 + payload zpkg。两个可选字段把它们放到部署根下的**完整相对路径**：
> - `bin`：apphost 二进制路径（如 `bin/myapp`；不写则 `<name>` 落部署根，即扁平布局）。
> - `payload`：payload zpkg 路径（如 `programs/myapp/myapp.zpkg`）。设了 → publish 把已编译 zpkg
>   **复制**到此处，使部署子树自洽；不设 → 原地内嵌已编译 zpkg（不复制）。
>
> apphost 内嵌的 payload 相对路径（从 `bin` 目录到 `payload`）由 publish **自动计算**，用户不手算。
> 这是面向用户的通用旋钮——用户发布自己的 app 与 z42 SDK 内部布置 z42c/z42b/z42d **共用同一套字段**。
> 解析见 `z42.project` 的 `DesktopConfig.Bin` / `.Payload`；消费见 `launcher_export.z42` 的 `_cmdPublishDesktop`。

> **gate 与位置分离**：`apphost = true` 是唯一 gate，`publish_dir` 只是输出位置
>（不会把"输出目录"与"是否启用"耦合在一个键上）。解析见 `z42.project` 的 `DesktopConfig.Apphost`；gate 实现见 `launcher_export.z42`
> 的 `_cmdPublishDesktop`。apphost 的打桩与签名属实现细节，本书不展开。

### CLI 覆盖

所有 toml 值可通过 CLI 标志覆盖：

```
z42 export ios     <project.z42.toml> [--bundle-id com.x.y] [--output ./MyApp] [--sdk-ver 0.3.0]
z42 export android <project.z42.toml> [--app-id  com.x.y] [--output ./MyApp] [--sdk-ver 0.3.0]
z42 export wasm    <project.z42.toml>                       [--output ./MyApp] [--sdk-ver 0.3.0]
z42 publish <project.z42.toml>                             [--output <publish_dir>]
```

工程生成的实现细节本书不展开。

## `build/` 构建扩展目录（z42b 自定义流程，build-orchestrator）

> ⚠️ 前瞻设计（未实施）。完整设计属实现细节，本书不展开。

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

扩展点基类（`BuildHooks` / `WorkloadBase`）住 [`src/compiler/z42.build/`](https://github.com/z42-lang/z42/blob/main/src/compiler/z42.build)。
