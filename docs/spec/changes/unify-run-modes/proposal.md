# Proposal: 运行模式统一——统一启动入口 + 统一运行时设置（unify-run-modes）

> 状态：🔴 DRAFT（待 User 审批）| 创建：2026-07-26 | 类型：`vm` + `toolchain`（新运行入口 + 设置解析）→ 完整流程
> 子系统：`toolchain`（launcher 分发器 + 源码运行壳）‖ `runtime`（设置解析 + 内存执行入口）‖ `stdlib`（`z42.scripting` 复用/迁移）‖ `compiler`（`[profile.*]` 解析）
> 设计 SoT（落地时同步）：`docs/design/runtime/launcher.md`（分发器）、`docs/design/compiler/project.md`（`[runtime]` 段 + profile.mode 消费）、新增 `docs/design/runtime/runtime-settings.md`（设置优先级链 + 旋钮 SoT）
> 前置：REPL 底座（`add-z42-repl` ✅ 已在 main：`z42.scripting` Form B + `__load_bytecode_in_memory` + z42i + launcher repl 路由）、进程内编译 API（`extract-compile-pipeline-api` ✅ `PackageCompile`）

---

## Why

z42 目前有三种运行形态，各走各的入口、各用各的设置来源，缺一条统一心智：

1. **跑编译产物** `.zpkg`/`.zbc`：launcher `_cmdRun` → `z42vm`（host + embed）
2. **REPL**：launcher `_forwardRepl` → z42i → `z42.scripting`（host + embed）
3. **跑源码**（像 python 直接跑 `.z42`）：**尚不存在**

同时"运行时设置"散在三处（CLI flag / 环境变量 / 未被消费的 `[profile.*].mode`），无单一优先级链、无单一 SoT。

本 change 做两件统一：
- **A. 统一启动入口**：一个前门 `z42 <target>` 按 target 分类派发到三模式，并补上"跑源码"（单文件 + 工程）。
- **B. 统一运行时设置**：建立分层优先级链（CLI > env > 配置文件 > profile > 默认），以 VM 的 `KNOWN_KNOBS` 为唯一旋钮 SoT，配置文件统一走 TOML。

关键洞察：**"跑源码" 不是新造轮子，而是 REPL 已铺好的进程内 `Compile→bytes→load→invoke` 底座换一个前门**。Embed 与 Host 的真正分界不是"能否编译"（Embed 要 REPL ⇒ Embed 已内嵌编译器），而是"能否从磁盘解析工程"。

---

## 模式支持矩阵（本 change 确立的规格）

| 运行模式 | Host | Embed | 第2层"产字节"来源 |
|---|:---:|:---:|---|
| 跑编译产物 `.zpkg`/`.zbc` | ✓ | ✓ | 无（直接 load）|
| REPL | ✓ | ✓ | 编 snippet（循环）|
| 源码运行 · 单文件 | ✓ | ✗ | 合成 manifest → 编译 |
| 源码运行 · 工程 | ✓ | ✗ | 读 manifest → 编译 |

> Embed = 第1层执行芯 + REPL 那一支（编内存 snippet）；砍掉"从磁盘读工程"的两支前门。Embed 支持 REPL 的硬前提是固件能吃下整个编译器链接体积（既有成本，非本 change 新增）。
>
> **依赖来源统一为注入接口（D6）**：编译核心不硬编码"扫文件系统"，而是通过抽象**依赖 provider 接口**取依赖 zpkg——host 实现 = 主动扫 `Z42_LIBS` 目录注入 stdlib；embed 实现 = 固件/用户显式注入。**同一机制、可插拔来源**。Host 与 Embed 的差异收敛为"provider 的一个实现"，而非两套代码路径。

---

## 架构：三门一芯

```
                        ┌─────────────── toolchain（薄壳）───────────────┐
前门:   z42 app.zpkg    z42 run foo.z42    z42 run <projdir>    z42 repl
             │           └─合成 manifest─┘  └─读 manifest─┘        │(readline+元指令)
             │                    └──────────┬──────────┘          │
             │                               ▼                     ▼
             │              ┌──────── stdlib-tier: z42.scripting（核心）────────┐
             │              │  ScriptState · CompileFile/Project · Eval · Engine │
             │              │  依赖 → z42c pipeline + z42.ir（已在 stdlib）      │
             │              └──────────────────┬─────────────────────────────────┘
             │                                 ▼  产 bytes（经增量 cache，按 hash）
第1层 执行:  └──────── VM: load(bytes/zpkg) → invoke(entry) ──────────────────────┘
                       (__load_bytecode_in_memory / __invoke_static，host+embed 都有)
```

- **第1层（执行芯）到处相同**：VM 加载一个包 + invoke 入口。Host / Embed 共用。
- **第2层（产字节）是唯一差异层**：编译产物=无；单文件=合成 manifest；工程=读 manifest；REPL=编 snippet。
- **Embed = 第1层 + REPL 支**，不含"从磁盘读工程"两支。

---

## 单文件 vs 工程：合成 manifest 统一

两者最终都产出同一个 `CompileInputs`，只是 manifest 从哪来不同——**给单文件合成一个隐式 `ProjectManifest`**，走与工程完全相同的 `ManifestLoader → PackageCompile` 链：

**单文件 `foo.z42` → 隐式合成 manifest**
```
name         = 文件名 stem（foo）
kind         = exe
entry        = Main（约定；要求文件含 void Main()）
sources      = [foo.z42]           ← 仅此一个文件
dependencies = 仅 stdlib（prelude + 该文件 using 到的 z42.* 包，从 Z42_LIBS 解析）
profile      = 默认（mode 由设置优先级链决定）
```

**工程 `<dir>` → 读磁盘真 manifest**
```
<dir>/*.z42.toml（或 z42.workspace.toml + -p 选成员）
→ 真 ProjectManifest（本地依赖 + sources glob + [profile.*].mode + [build]）
```

于是只有**一个 `SourceRunEngine`，两个前端**（合成 / 加载 manifest），后半段全复用现有编译+执行链。

**三条边界规则（避免模糊态）：**
1. **单文件 = 只能依赖 stdlib，不能依赖本地兄弟源文件。** 要多文件/本地依赖 → 升级成工程（写 `z42.toml`）。类比 `rustc foo.rs` vs `cargo`，中间无"散文件无 manifest"模式。
2. **工程 = 必须有 manifest。** `z42 run <dir>` 找不到 manifest → 明确报错，不猜。
3. **入口约定 = 要求 `void Main()`。** 不引入 top-level-statements 新语法（避免踩 bootstrap-seed 分阶段纪律）；"顶层语句脚本"留作独立语言特性 change。

---

## 前门分类器（launcher `_cmdRun` 扩展）

```
z42 <target> [-- args]     分类:
  *.zpkg / *.zbc   → 模式① 加载产物直接跑        (host + embed)
  *.z42 (文件)      → 模式② 单文件源码运行          (host)
  <目录> / 省略     → 找 manifest:
      *.z42.toml         → 工程源码运行
      z42.workspace.toml → 需 -p 选成员（或 default-members）
      都没有             → 报错（不猜"散文件"）
  省略 target + 无工程   → 模式③ REPL             (host + embed)
```

- 保留 `z42 foo.z42` 裸简写（像 `python foo.py`），与现有 `z42 app.zpkg` 裸简写对称。
- `z42 run foo.z42` 显式形式同样接受。

---

## 运行时设置统一：分层优先级 + 单一旋钮 SoT

**主张：不是 env 与配置文件二选一，而是建立单一优先级链，两者都是同一组旋钮的输入。**

**优先级（高→低）：**
```
CLI flag  >  环境变量  >  运行配置文件  >  工程 profile  >  SDK 全局默认
--mode       Z42_*        [runtime] 段     [profile.*]     KNOWN_KNOBS default
```

**四项动作：**
1. **`KNOWN_KNOBS`（`src/runtime/src/config.rs`）升格为设置 Schema 唯一 SoT。** 补齐 2 个漏网旋钮（`Z42_JIT_PROFILE` @ `jit/lazy.rs`、`Z42_TARGET` 预留），修 GC minor threshold 描述（"64 KiB" vs 实际比率 0.75 不一致）。每旋钮声明：名字/类型/默认/`consumed_by`/**对应 TOML key + env 名**。env / TOML / CLI 三输入映射到同一旋钮，`RuntimeConfig` 是唯一解析出口。
2. **配置文件统一 TOML，收编 JSON 侧车。** 新增 `[runtime]` 段，物理上按模式取自不同文件、但同一解析器：
   - 模式① 编译产物：`app.runtimeconfig.toml` 侧车（**取代现 `.runtimeconfig.json`**，与全仓 Std.Toml 栈一致）。
   - 模式②/③：`<name>.z42.toml` 的 `[runtime]` + `[profile.*].mode`。
   - 全局兜底：`~/.z42/config.toml` 扩 `[runtime]` 段（把 launcher 那处手写单行解析换成 Std.Toml，消掉第二套解析器）。
3. **打通 `[profile.*].mode` 消费链。** z42c 落地 `[profile.*]` 解析（`Main.z42` 现延后项），运行路径也读它——"这个包默认用什么执行模式"由 manifest 决定，而非只能 `--mode` 覆盖。
4. **env 保持一等输入，不废弃。** `Z42_LIBS`/`Z42_HOME`/`Z42_PORTABLE_VM` 是三层共用定位/依赖总线，CI/容器重度依赖；优先级高于配置文件，只是纳入统一链、语义明确。

---

## 已定决策（本轮讨论确认）

| # | 决策 | 选定 | 理由 |
|---|---|---|---|
| D1 | 源码运行执行形态 | **进程内**（复用 `__load_bytecode_in_memory→invoke`，不 fork z42vm）| 让"源码运行=REPL 换前门"成立；Embed 复用同一条 |
| D2 | 源码运行编译产物缓存 | **按 hash 增量缓存**，与现有增量编译一致（`.cache`/`cache_dir`）| cache 命中直接 load ≈ 编译产物速度；miss 才现编 |
| D3 | 编译-求值内核归属 | **(A) `z42.scripting` 作共享 stdlib-tier 核心**；`z42c.*` pipeline 仍是依赖库 | 三模式共享内核、Embed 可用；不做 (B) 编译器整体 stdlib 化（留长期独立方向）|
| D4 | 单文件运行 | **合成隐式 manifest**，走与工程相同的编译链 | 单一 `SourceRunEngine`，与工程运行完全一致 |
| D5 | 运行时设置 | **统一 TOML + 分层优先级**，KNOWN_KNOBS 为旋钮 SoT，env 不废弃 | 单一 SoT + 单一优先级链，收编 JSON 侧车 |
| D6 | 依赖来源 | **抽象依赖 provider 注入接口**；host=扫 `Z42_LIBS` 注入，embed=固件/用户显式注入 | 同一机制可插拔来源；host/embed 差异收敛为一个 provider 实现 |
| D7 | 单文件缓存位置 | **随文件 `.cache/`**（非全局 `~/.z42/.cache`）| 与工程增量缓存布局一致 |
| D8 | `z42.scripting` 物理迁移 | **暂不迁移**；等编译器 pipeline 整体 stdlib 化 (B) 时一起动 | 它依赖 `z42c.*`（仍在 src/compiler/），单迁会造成 stdlib 反向依赖 compiler 树，无意义 |

> D1+D2 调和：**"进程内" 指执行不 fork，非"零落盘"**——落盘的是增量缓存产物（复用增量编译的前提），执行仍进程内 load/invoke。

---

## What This Does NOT Do（明确划走）

- **(B) 编译器 pipeline 整体 stdlib 化**：本 change 只把 `z42.scripting` 作共享核心，`z42c.pipeline/syntax/semantics` 仍是 `z42c.*`。编译器整体 stdlib 化留独立长期方向（延续 z42.ir 收敛）。
- **`z42.scripting` 物理迁 `src/libraries/`（D8）**：本 change 不迁——它依赖仍在 `src/compiler/` 的 `z42c.*`，单迁会造成 stdlib 反向依赖 compiler 树。随 (B) 一起动。
- **top-level-statements 脚本语法**：单文件仍要求 `void Main()`；顶层语句作独立语言特性 change。
- **散文件无 manifest 运行**：不支持；要么单文件，要么工程（带 manifest）。
- **Embed 上的源码运行**：host-only；Embed 仅编译产物 + REPL。
- **源码运行的 mobile/WASM 支持**：随 host 先行，mobile defer。

---

## 五阶段迭代（每阶段独立可 commit + 可全绿）

| 阶段 | 内容 | 子系统锁 | 风险 |
|---|---|---|---|
| **P0** | 设置 SoT 收敛：补齐 KNOWN_KNOBS 漏网旋钮、修描述不一致、`RuntimeConfig` 加 `[runtime]` TOML 输入 + 优先级链 | `runtime` | 最低（不改入口行为，纯地基）|
| **P1** | 侧车 JSON→TOML：launcher 读 `.runtimeconfig.toml`（Std.Toml）；`~/.z42/config.toml` 换 Std.Toml；`z42 publish` 侧车产出同步换 | `toolchain` | 低 |
| **P2** | `[profile.*]` 打通：z42c 解析 profile 段，运行路径消费 `mode` | `compiler` | 中（碰自举，需不动点验证）|
| **P3** | 统一分发器 `RunEngine`：①③ 收敛到单一前门 + 单一设置解析 + 依赖定位 | `toolchain` | 中 |
| **P4** | 模式② 源码运行：`SourceRunEngine`（合成/加载 manifest）+ 进程内 load/invoke，接在 P3 + P0 上 | `toolchain`(+`stdlib` 若迁 z42.scripting) | 中 |

---

## Scope（允许改动的文件 · 待 IMPL 起步时按锁细化）

> IMPL 起步前逐阶段查 ACTIVE.md：`compiler`（现被 `nested-types-followup` 占）、`stdlib`（现被 `converge-z42c-onto-z42-project` 占）；`runtime`/`toolchain` 现空闲。P0 可先起（runtime 空闲）。

### runtime（P0）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/config.rs` | MODIFY | 补漏网旋钮进 KNOWN_KNOBS；修描述不一致；`RuntimeConfig` 加 `[runtime]` TOML 输入源 + 优先级合并 |
| `src/runtime/src/main.rs` | MODIFY | 设置解析接入优先级链；（P4）接受内存 bytes 入口 |
| `src/runtime/src/jit/lazy.rs` | MODIFY | `Z42_JIT_PROFILE` 纳入 RuntimeConfig（去 straggler）|

### toolchain（P1/P3/P4）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/toolchain/launcher/core/launcher.z42` | MODIFY | `_cmdRun` 分类器扩展；`.runtimeconfig.toml`（Std.Toml）；`config.toml` 换 Std.Toml |
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | 前门分类派发（.z42 / 目录 / 省略）|
| `src/toolchain/launcher/core/*run_engine*.z42` | NEW | 统一 `RunEngine`（设置解析 + 定位 + 派发）|
| `src/toolchain/*/SourceRunEngine.z42` | NEW | 合成/加载 manifest → 进程内编译执行（P4）|

### stdlib（P4，不做物理迁移 — D8）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/toolchain/scripting/src/Script.z42` | MODIFY | 加 `CompileFile` / `CompileProject` 入口（源码运行复用）|
| `src/toolchain/scripting/src/*Provider*.z42` | NEW | 依赖 provider 注入接口（D6）：host 扫 `Z42_LIBS` / embed 显式注入 |

### compiler（P2）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | 解析 `[profile.*]` 段（现延后项）|
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY（若需）| profile 段解析补完 |

### docs
| 文件 | 变更 |
|------|------|
| `docs/design/runtime/runtime-settings.md` | NEW（设置优先级链 + 旋钮 SoT，配 mermaid）|
| `docs/design/runtime/launcher.md` | MODIFY（分发器 + 前门分类）|
| `docs/design/compiler/project.md` | MODIFY（`[runtime]` 段 + profile.mode 消费）|
| `docs/design/toolchain/repl.md` | MODIFY（顺带修已知 stale：单目录 Z42_LIBS 实现）|
| `docs/features.md` | MODIFY（设置优先级表）|
| `docs/roadmap.md` | MODIFY（源码运行能力状态）|

---

## 协调（与在飞/相邻 change）

- **`perf-optimize-repl-eval`**（🟡 进行中，占 compiler 工作树 DepScan）：决定源码运行每次编译成本（DepScan 双读 / 跨轮缓存 / 双编译）。本 change 的进程内编译路径**共享其性能收益**；P4 起步前它最好已落地，否则单文件运行也慢。
- **`extract-compile-pipeline-api`**（✅ `PackageCompile`）：本 change 的 `CompileFile`/`CompileProject` 建其上；接口边界以其为准。
- **锁排队**：P2 需 `compiler`（现被占）、P4 迁移需 `stdlib`（现被占）。P0（runtime）可立即起。IMPL 分阶段各自排锁，不一次性占四锁。

---

## 未决

全部已敲定（2026-07-27）：Embed 依赖来源 → D6 注入接口；单文件缓存 → D7 随文件；物理迁移 → D8 暂不迁。DRAFT 规格完整，待 User 批准进 IMPL。
