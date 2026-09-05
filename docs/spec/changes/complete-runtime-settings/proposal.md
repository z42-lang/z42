# Proposal: 补完运行时设置系统——CLI 层 + 可用性矩阵 + 脚本读取 + 结构化查询（complete-runtime-settings）

> 状态：🔴 DRAFT（待 User 审批）| 创建：2026-09-05 | 更新：2026-09-05（User 裁决：profile 走侧车 TOML / `--set` 只认完整 key + 显式 alias / 每旋钮声明可接受层）
> 类型：`vm`（+ `stdlib` 表面 + `compiler` 侧车生成）→ 完整流程
> 更新 2：2026-09-05（User 裁决：运行配置格式只有 TOML，不引入 JSON）
> 前置：[`archive/2026-07-29-unify-run-modes`](../../archive/2026-07-29-unify-run-modes/) P0（`KNOWN_KNOBS` SoT + `env > 文件 > 默认` 分层）已在 main
> 设计 SoT（落地时同步）：新增 `docs/book/src/runtime/runtime-settings.md`（补 unify-run-modes 未落地的那份）

---

## Why

「运行时设置」在 unify-run-modes P0 建了地基，但只完成了一半。现状缺口是**四个具体的洞**，不是"重做"：

| 能力 | 现状 | 缺口 |
|---|---|---|
| 环境变量层 | ✅ `RuntimeConfig::resolve` 全量 21 旋钮 | — |
| 配置文件层 | ✅ `Z42_CONFIG` → `[runtime]` TOML + launcher 侧车发现 | ① 侧车与用户配置**抢同一个通道**（见下）② 侧车无生成器，全靠手写 |
| **CLI 层** | ❌ 只有 `--mode` 一个专用 flag 硬编码在 `main.rs` | **21 个旋钮里 20 个无法从命令行设置** |
| **脚本读取** | ❌ 无 | z42 代码只能读**原始 env**，读不到生效值与来源 |
| **可用性声明** | ❌ 无 | 旋钮不声明平台 / feature / build profile / 可接受的输入层；设了本 build 不支持的旋钮 = **静默无效** |
| **结构化查询** | 🟡 `--info` 人类可读文本 | 无机器可读形态；无 schema 转储；无「为什么这个值没生效」 |

**最痛的是"静默无效"**：`Z42_JIT_PROFILE` 在 `--no-default-features` 的 interp-only build 上设了完全没反应，没有任何提示；`Z42_STRESS_ITERS` 是测试专用旋钮却与生产旋钮平等展示。CoreCLR 用 `RETAIL_CONFIG_*` vs `CONFIG_*` 宏在**编译期**区分 retail/debug-only 旋钮（[clrconfig.h](../../../../../codesigner-ui/runtime/src/coreclr/inc/clrconfig.h) 的 `#ifdef _DEBUG` 分支），并用 `INTERNAL_/UNSUPPORTED_/EXTERNAL_` 符号前缀区分支持级别。z42 需要同样的分级，且**比 CoreCLR 更进一步**——CoreCLR 对 release build 里设了 debug-only 旋钮是静默忽略，本 change 要**明确告知**。

### 现存缺陷 1：侧车与用户配置抢通道

launcher 把应用侧车 `<app>.runtimeconfig.toml` 的路径塞进 `Z42_CONFIG`，且仅在用户没设 `Z42_CONFIG` 时才塞（[launcher.z42:321-326](../../../../src/toolchain/launcher/core/launcher.z42)）。后果：**用户一旦显式设了 `Z42_CONFIG`，应用自带的配置就被整份丢弃**，而不是被逐 key 覆盖。「用户想改一个旋钮」和「应用自带一组默认」是两件事，不该共用一个通道。

### 现存缺陷 2：工程 profile 落在 env 层

launcher 把 `[profile.debug].mode` 注入成 `Z42_MODE` 环境变量（[launcher.z42:437-440](../../../../src/toolchain/launcher/core/launcher.z42)），于是工程 profile 实际落在 env 层、压过配置文件——与文档承诺的链相反。且它只能带 `mode` 一个旋钮。

**两个缺陷有同一个正解**（User 裁决 2026-09-05）：**工程配置直接写进 `runtimeconfig.toml`**，作为独立的「应用配置层」，与「用户配置层」分开通道、逐 key 叠加。这正是 dotnet 的做法——`<app>.runtimeconfig.json` 由 SDK 从项目属性**生成**，不是手写。

---

## What（六件事）

### A. CLI 成为最高优先级层

```
z42vm --set gc-mode=concurrent --set safepoint-throttle=1 app.zpkg
```

- `--set <key>=<value>` 可重复；`key` **只认旋钮定义里的完整 key**（`toml_key`，kebab-case）。
- 需要短写法的旋钮在 `KnobSpec` 里**显式声明 `aliases`**——不自动接受 `Z42_*` env 名（避免"两套等价写法"的隐式约定，也避免 env 名与 kebab key 将来不同步时产生歧义）。
- 现有专用 flag（`--mode`）与 `--set mode=` **同层**；同时给出 → 显式报错（不猜）。
- 未知 key → 报错 + 最近邻建议（不静默）。

**为什么不逐个加 flag**：21 个旋钮逐个加 clap flag 会让 `--help` 爆炸，且每加一个旋钮要改三处。通用 `--set` 让 `KNOWN_KNOBS` 继续是唯一 SoT——新增旋钮 = 一处表格编辑。

### B. 每个旋钮声明「接受哪些输入层」（"有些支持 CLI 配置"）

`KnobSpec` 增 `sources: LayerMask` —— 不是所有旋钮都该从所有层设置：

| 旋钮 | 可接受层 | 理由 |
|---|---|---|
| `Z42_GC_MODE` 等绝大多数 | CLI ∪ env ∪ 用户文件 ∪ 应用文件 | 常规旋钮 |
| `Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG` | CLI ∪ env **only** | 元旋钮——写在配置文件里会自指（一个文件指定读哪个文件 / 提升它自己的严重度）|
| `Z42_STRESS_ITERS` | env only | 测试脚手架，不进用户 CLI 表面 |

从不被接受的层设置某旋钮 → 与"旋钮不可用"同一套诊断（见 D）。

### C. 每个旋钮声明可用性（平台 / feature / build profile / 支持级别）

`KnobSpec` 另增四个字段：

| 字段 | 取值 | 对应 CoreCLR |
|---|---|---|
| `value` | `Bool/Int/Float/Str/Path/PathList/Enum(&[..])` | `ConfigDWORDInfo` vs `ConfigStringInfo` |
| `build` | `Always` \| `DebugOnly` | `RETAIL_CONFIG_*` vs `CONFIG_*`（`#ifdef _DEBUG`）|
| `requires` | feature 名列表（`jit` / `native-interop` / …）| 无对应（z42 特有，Cargo feature 模型）|
| `platforms` | `All` \| `Only(&[..])` \| `Except(&[..])` | 无对应（z42 特有）|
| `tier` | `Public` \| `Unsupported` \| `Internal` | `EXTERNAL_/UNSUPPORTED_/INTERNAL_` 符号前缀 |

### D. 不可用 / 不接受时**明确告知**，严重度按来源分层

| 设置来源 | 该旋钮不可用或不接受该层时 | 理由 |
|---|---|---|
| **CLI**（`--set` / 专用 flag）| **致命错误 + 退出码 2** | 用户此刻、在这台机器上手敲的；静默忽略 = 最坏体验 |
| **env / 用户文件 / 应用文件** | **一行 warn + 忽略该值 + 用默认继续跑** | 这些跨机器/跨 build 传播（CI 全局 export、容器镜像、随产物分发的侧车）；一个 release 二进制不能因为环境里有个 debug 旋钮就起不来 |

严格模式逃生门：`--strict-config` / `Z42_STRICT_CONFIG=1` 把 warn 升级为 error（CI 用）。

告知信息必须完整回答「为什么不可用 + 这个 build 是什么 + 现在用的是什么值」：

```
z42: 旋钮 `jit-profile`（Z42_JIT_PROFILE，来源 [env]）在本 build 不可用：
     需要 feature `jit`；本 z42vm 编译时启用的是：interp, native-interop。
     → 已忽略该值，使用默认（JIT profiling off）。
     用 `z42vm --list-knobs --all` 查看全部旋钮的可用性。
```

### E. 工程配置写进 `runtimeconfig.toml`（修两个现存缺陷）

- **生成**：`z42 build` 时，z42c driver 在产 `dist/<name>.zpkg` 的同时产 `dist/<name>.runtimeconfig.toml`，把 manifest 的 `[profile.<用的那个>]` 旋钮烤成 `[runtime]` 表。**对齐 dotnet**（SDK 从 csproj 生成 `<app>.runtimeconfig.json`，不手写）。
- **传递**：launcher 改用**独立通道** `Z42_APP_CONFIG` 传侧车路径，不再抢占 `Z42_CONFIG`。
- **叠加**：VM 端「用户配置层」（`Z42_CONFIG`）**逐 key 高于**「应用配置层」（`Z42_APP_CONFIG`）——用户改一个旋钮不会丢掉应用自带的其余配置。
- **不再注入 `Z42_MODE`**：profile 的 mode 走侧车，落回正确的层。profile 也从"只能带 mode"升级为"能带任意旋钮"。
- **手写侧车不被覆盖**：若 `dist/` 下已存在**非生成的**同名侧车，生成器报错而非静默覆盖；预留 dotnet 式 `runtimeconfig.template.toml` 合并入口（本 change 不实现）。

### F. z42 脚本可读（只读）

新增 `Std.Runtime.RuntimeConfig` 静态类 + 6 个 builtin（追加注册，保持 BuiltinId 稳定）：

```z42
string? v   = RuntimeConfig.Get("gc-mode");        // 生效值（分层解析后）
string  src = RuntimeConfig.Source("gc-mode");     // "cli"|"env"|"user-config"|"app-config"|"default"
bool    ok  = RuntimeConfig.IsAvailable("jit-profile");
string[] ns = RuntimeConfig.Names();
string[] d  = RuntimeConfig.Dump();                // "key=value|source" 扁平条目
string? doc = RuntimeConfig.Describe("gc-mode");
```

**只读，无 setter**——见 design.md Decision 5。

### G. 结构化查询（`z42vm` 端）

| 命令 | 输出 |
|---|---|
| `z42vm --list-knobs [--all] [--json]` | **schema**：名字 / CLI key + alias / TOML key / 类型 / 默认 / **可接受层** / 可用性 / tier / consumed_by / 说明。默认只列 `Public`；`--all` 含 `Unsupported` + `Internal` |
| `z42vm --show-config [--json]` | **生效值 + 来源**：逐旋钮 `[cli]/[env]/[user-config]/[app-config]/[default]`，含"某层的值为什么没生效"的解释行 |
| `z42vm --info` | 保持现状（build 信息 + 旋钮块），旋钮块改为调用同一渲染器（不重复实现）|

### H. 运行配置格式只有 TOML（不引入 JSON）

`Z42_CONFIG`（L3）与 `Z42_APP_CONFIG`（L4）都只读 TOML 的 `[runtime]` 表，共用同一个加载器。

**不做 JSON、也不为 JSON 预留抽象**：unify-run-modes 已经裁决过把 .NET 风格的 JSON 侧车**收编成 TOML**（D5），z42 全仓配置格式已统一（`z42.toml` manifest / `~/.z42/config.toml` / `.runtimeconfig.toml`）。唯一的关照是：路径以 `.json` 结尾时给一行**迁移提示**错误，而不是让用户对着一个被静默忽略的文件 debug。

---

## 最终优先级链（高 → 低）

```
L1 CLI        z42vm --set k=v / --mode                    ← 本 change 新增
L2 env        Z42_*                                        ✅ 已有
L3 用户配置    Z42_CONFIG → [runtime]                       ✅ 已有（不再被侧车抢占）
L4 应用配置    Z42_APP_CONFIG → <app>.runtimeconfig.toml    ← 本 change 生成 + 独立通道
              （build 从 manifest [profile.*] 烤入）
L5 默认        KNOWN_KNOBS default                          ✅ 已有
```

L3/L4 都是"文件层"、格式与解析器相同，区别只在**谁写的**：L3 是用户，L4 是 build。工程 profile 因此天然落在用户配置之下、默认之上——与直觉一致。

---

## What This Does NOT Do（明确划走）

- **不引入 JSON 配置格式**：只给 `.json` 路径一行迁移提示错误。
- **不做运行时可变旋钮**：配置在 boot 后冻结（`OnceLock`），脚本只读；热改 GC 参数走专门 API（`Std.GC`）。
- **不实现 `runtimeconfig.template.toml` 合并**：只预留入口，且对已存在的手写侧车报错而非覆盖。
- **不新增旋钮**：只给现有 21 个补 schema 元数据（+ 3 个元旋钮 `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG` 及其登记）。
- **不改 zbc / zpkg 格式**：设置走侧车文件，不烤进产物字节（延续 unify-run-modes 的方向 B）。
- **不动 REPL / apphost 的启动路径**（除侧车通道换名）。

---

## 六阶段迭代（每阶段独立可 commit + 可全绿）

| 阶段 | 内容 | 子系统 | 风险 |
|---|---|---|---|
| **P0** | `KnobSpec` 扩 schema（`value`/`build`/`requires`/`platforms`/`tier`/`sources`/`aliases`）+ 21 旋钮填表 + 可用性求值器 + 单测 | runtime | 最低（纯元数据）|
| **P1** | 解析层记录 provenance（`ResolvedKnob`）+ 分层诊断严重度 + `--strict-config` | runtime | 低 |
| **P2** | CLI 层：`--set k=v` 接入链顶；alias 解析；`--mode` 冲突检测；未知 key 建议 | runtime | 中（碰 `main.rs` 启动路径）|
| **P3** | 查询表面：`--list-knobs` / `--show-config`（text + json）；`--info` 改调同一渲染器 | runtime | 低 |
| **P4** | 脚本表面：6 个 builtin（追加）+ `Std.Runtime.RuntimeConfig` + 双文件层加载器 | runtime + stdlib | 中（碰 BuiltinId 表）|
| **P5** | 侧车：`Z42_APP_CONFIG` 独立通道（VM + launcher）+ z42c driver 生成 `dist/<name>.runtimeconfig.toml` + launcher 停止注入 `Z42_MODE` | runtime + toolchain + **compiler** | **中高**（碰自举，需不动点验证）|

> P5 是唯一碰自举的阶段，放最后；前五阶段全部可独立落地。

---

## Scope（允许改动的文件）

### runtime（P0–P5）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/config/knobs.rs` | MODIFY | `KnobSpec` 扩 7 字段 + 填表 |
| `src/runtime/src/config/availability.rs` | NEW | 可用性 + 可接受层求值 + 诊断渲染 |
| `src/runtime/src/config/resolve.rs` | NEW | provenance 分层解析（自 `config.rs` 迁出）|
| `src/runtime/src/config/source.rs` | NEW | 双文件层（L3/L4）的 TOML 加载与逐 key 叠加 |
| `src/runtime/src/config/render.rs` | NEW | 三个查询命令共用渲染器（text + json）|
| `src/runtime/src/config.rs` | MODIFY | hub：`RuntimeConfig` 本体 + 全局单例 + `Layer`/`ResolvedKnob` |
| `src/runtime/src/config_tests.rs` | MODIFY | 现有 47 测试保持 + 新增 |
| `src/runtime/src/main.rs` | MODIFY | `--set` / `--list-knobs` / `--show-config` / `--strict-config`；`--mode` 冲突检测 |
| `src/runtime/src/corelib/config.rs` | NEW | 6 个 `__cfg_*` builtin |
| `src/runtime/src/corelib/mod.rs` | MODIFY | **追加**注册（保 BuiltinId 稳定）|

### stdlib（P4）
`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42` | NEW（`Std.Runtime.RuntimeConfig` 只读表面）

### toolchain / compiler（P5）
| 文件 | 变更 |
|------|------|
| `src/toolchain/launcher/core/launcher.z42` | MODIFY（侧车走 `Z42_APP_CONFIG`；删 `Z42_MODE` 注入）|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY（产 zpkg 时同产 `dist/<name>.runtimeconfig.toml`）|
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY（若需：`[profile.*]` 全旋钮解析，现只取 mode）|

### docs（归档前必须落地）
| 文件 | 变更 |
|------|------|
| `docs/book/src/runtime/runtime-settings.md` | NEW（优先级链 + 旋钮 SoT + 可用性矩阵 + 诊断规则 + 侧车生成与传递，配 mermaid）|
| `docs/book/src/SUMMARY.md` | MODIFY（挂新页）|
| `docs/book/src/stdlib/…` | MODIFY（`Std.Runtime.RuntimeConfig` 表面）|
| `docs/design/runtime/launcher.md` | MODIFY（侧车通道 + 停止 `Z42_MODE` 注入）|
| `docs/features.md` | MODIFY（设置优先级表 + 可用性矩阵）|

---

## 已定决策（User 裁决 2026-09-05）

| # | 决策 | 选定 |
|---|---|---|
| U1 | 工程 profile 落层 | **直接写入 `runtimeconfig.toml` 侧车**（build 生成），作为独立的「应用配置层」L4；`Z42_APP_CONFIG` 独立通道，与用户 `Z42_CONFIG` 逐 key 叠加 |
| U2 | `--set` key 形式 | **只认完整 key**（`toml_key`）；需要短写法的旋钮在 `KnobSpec` 里显式声明 `aliases` |
| U3 | 旋钮的输入层 | 每个旋钮显式声明 `sources: LayerMask`——**不是所有旋钮都支持 CLI**；元旋钮只接受 CLI/env |
| U4 | `Z42_STRESS_ITERS` | 标 `DebugOnly + Internal` 留在表内（保持可发现性，默认视图隐藏）|
| U5 | 运行配置格式 | **只有 TOML**，不引入 JSON、不留 JSON 抽象；`.json` 路径给一行迁移提示错误 |

## 未决
无。spec：[design.md](design.md) / [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md) / [tasks.md](tasks.md)。待 User 批准进 IMPL。
