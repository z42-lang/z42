# Proposal: 运行模式统一——统一启动入口 + 统一运行时设置（unify-run-modes）

> 状态：🔴 DRAFT（待 User 审批）| 创建：2026-07-26 | 更新：2026-07-27（简化：取消单文件源码运行）| 类型：`vm` + `toolchain` → 完整流程
> 子系统：`runtime`（设置解析）‖ `toolchain`（launcher 前门 + build+run 编排）‖ `compiler`（`[profile.*]` 解析）
> 设计 SoT（落地时同步）：新增 `docs/design/runtime/runtime-settings.md`（设置优先级链 + 旋钮 SoT）、`docs/design/runtime/launcher.md`（前门分类）、`docs/design/compiler/project.md`（`[runtime]` 段 + profile.mode 消费）
> 前置：REPL 底座（`add-z42-repl` ✅ 已在 main）、现有 `z42 build` 编排

---

## Why

z42 目前的运行形态各走各的入口、各用各的设置来源，缺一条统一心智：

1. **跑编译产物** `.zpkg`/`.zbc`：launcher `_cmdRun` → `z42vm`（host + embed）
2. **REPL**：launcher `_forwardRepl` → z42i → `z42.scripting`（host + embed）
3. **跑源码工程**：**尚不存在**（要先 `z42 build` 再手动跑产物）

同时"运行时设置"散在三处（CLI flag / 环境变量 / 未被消费的 `[profile.*].mode`），无单一优先级链、无单一 SoT。

本 change 做两件统一：
- **A. 统一启动入口**：一个前门 `z42 run <target>` 按 target 分类派发；补上"跑源码工程"= **build（增量）+ run 的编排**（像 `cargo run`）。
- **B. 统一运行时设置**：建立分层优先级链（CLI > env > 配置文件 > profile > 默认），以 VM 的 `KNOWN_KNOBS` 为唯一旋钮 SoT，配置文件统一走 TOML。

> **设计简化（2026-07-27）**：早期方案含"单文件源码运行 + 进程内编译底座 + 合成 manifest + 依赖注入接口"。经讨论取消——z42 的代码单元是**包**（同包多文件天然互相可见、无需 import），单文件无法优雅生长（要多文件就得写 manifest 成为工程），故**只支持工程源码运行**。而工程源码运行 = build 到盘 + 跑产物，纯粹是两个现成能力的编排，**不需要独立的源码编译执行子系统**。本 change 因此由「设置统一」主导 + 一个薄前门便利命令。

---

## 模式支持矩阵

| 运行模式 | Host | Embed | 实现 |
|---|:---:|:---:|---|
| 跑编译产物 `.zpkg`/`.zbc` | ✓ | ✓ | 现有 `_cmdRun` → z42vm |
| REPL | ✓ | ✓ | 现有 z42i + z42.scripting |
| 跑源码工程（带 manifest 的目录）| ✓ | ✗ | **build（增量）+ 跑产物**（本 change 新增编排）|

> 无单文件模式：多文件互相引用 = 一个包（manifest 的 `sources` 决定），单文件天然只是"一个源文件的退化包"，无法生长，价值窄；快速实验交给 REPL。
> Embed 只跑编译产物 + REPL；不跑源码工程（无需磁盘工程解析）。

---

## 架构：一个前门，一个执行芯

```
前门(分类):   z42 run app.zpkg      z42 run <projdir>        z42 repl
                    │                 └─build(增量)─┐          │(readline+元指令)
                    │                        产 .zpkg│         │
执行芯:         ────┴──────────── z42vm: load(zpkg) → invoke(entry) ─── z42.scripting(REPL)
```

- **执行芯到处相同**：`z42vm load(zpkg/zbc) → invoke(entry)`。Host / Embed 共用。
- **跑源码工程 = build + run 编排**：`z42 run <dir>` 检测到源码工程 → 调**现有 `z42 build`**（增量缓存，已新鲜则跳过）→ 把产出的 `.zpkg` 喂给现有"跑编译产物"路径。不新写编译执行逻辑、不进程内、不碰 z42.scripting。
- **REPL 独立**：`z42 repl` 仍走 z42i + z42.scripting（本 change 不动其内核）。

---

## 前门分类器（launcher `_cmdRun` 扩展）

```
z42 run <target> [-- args]     分类:
  *.zpkg / *.zbc   → 加载产物直接跑                    (host + embed)
  <目录> / 省略     → 找 manifest（*.z42.toml / z42.workspace.toml）:
      有 → build(增量) + 跑产出 .zpkg                  (host)
      无 → 明确报错（不猜）
z42 repl                       → REPL                  (host + embed)
```

- 保留 `z42 app.zpkg` 裸简写；`z42 run <dir>` 为源码工程便利命令。
- workspace 目录需 `-p` 选成员（或 default-members）。

---

## 运行时设置统一：分层优先级 + 单一旋钮 SoT

**主张：不是 env 与配置文件二选一，而是建立单一优先级链，两者都是同一组旋钮的输入。**

**优先级（高→低）：**
```
CLI flag  >  环境变量  >  运行配置文件  >  工程 profile  >  SDK 全局默认
--mode       Z42_*        [runtime] 段     [profile.*]     KNOWN_KNOBS default
```

**四项动作：**
1. **`KNOWN_KNOBS`（`src/runtime/src/config.rs`）升格为设置 Schema 唯一 SoT。** 补齐漏网旋钮（`Z42_JIT_PROFILE` @ `jit/lazy.rs`、`Z42_TARGET` 预留），修 GC minor threshold 描述失真。每旋钮声明：名字/默认/`consumed_by`/**对应 TOML key + env 名**。env / TOML / CLI 三输入映射到同一旋钮，`RuntimeConfig` 是唯一解析出口。
2. **配置文件统一 TOML，收编 JSON 侧车。** 新增 `[runtime]` 段，物理上按模式取自不同文件、同一解析器：
   - 编译产物：`app.runtimeconfig.toml` 侧车（**取代现 `.runtimeconfig.json`**）。
   - 工程：`<name>.z42.toml` 的 `[runtime]` + `[profile.*].mode`。
   - 全局兜底：`~/.z42/config.toml` 扩 `[runtime]` 段（launcher 那处手写单行解析换 Std.Toml）。
   - **解析放 VM 端**：launcher 与 apphost 两条启动路径都汇入 z42vm，放这里两路径同时覆盖；runtime 已有 `toml = "0.8"`，零新依赖。
3. **打通 `[profile.*].mode` 消费链。** z42c 落地 `[profile.*]` 解析（`Main.z42` 现延后项），运行路径也读它——执行模式由 manifest 决定，而非只能 `--mode` 覆盖。
4. **env 保持一等输入，不废弃。** `Z42_LIBS`/`Z42_HOME`/`Z42_PORTABLE_VM` 是共用定位/依赖总线，CI/容器重度依赖；优先级高于配置文件，纳入统一链。

---

## 已定决策

| # | 决策 | 选定 | 理由 |
|---|---|---|---|
| D1 | 跑源码工程执行形态 | **build 到盘（增量）→ 走现有编译产物运行路径**（非进程内）| 工程本就产盘上 .zpkg，build+run 编排最简，复用一切现成能力 |
| D2 | 源码工程缓存 | **复用现有增量 build 缓存**（`.cache`/`cache_dir`）| 不新造；已新鲜则跳过 build |
| D-build | build 步实现 | **复用现有 `z42 build` 编排**（launcher 拼 `build` + `_cmdRun`）| 不新写编译逻辑 |
| D5 | 运行时设置 | **统一 TOML + 分层优先级**，KNOWN_KNOBS 为旋钮 SoT，env 不废弃 | 单一 SoT + 单一优先级链，收编 JSON 侧车；VM 端解析覆盖双启动路径 |

> 早期 D3/D4/D6/D7/D8（z42.scripting 作源码运行共享核心 / 单文件合成 manifest / 依赖 provider 注入接口 / 单文件缓存 / 物理迁移）随"取消单文件"整体移除。

---

## What This Does NOT Do（明确划走）

- **单文件源码运行**：不支持。只支持带 manifest 的工程。快速实验用 REPL。
- **源码运行的进程内编译底座 / z42.scripting 复用**：不做——工程 build 到盘再跑产物，REPL 的 z42.scripting 保持专用不动。
- **编译器 pipeline stdlib 化**：本 change 不碰，留独立长期方向。
- **Embed 上的源码运行**：host-only；Embed 仅编译产物 + REPL。
- **top-level-statements 脚本语法 / mobile·WASM 源码运行**：不在本 change。

---

## 四阶段迭代（每阶段独立可 commit + 可全绿）

| 阶段 | 内容 | 子系统锁 | 风险 |
|---|---|---|---|
| **P0** | 设置 SoT 收敛：补齐 KNOWN_KNOBS 漏网旋钮、修描述、`RuntimeConfig` 加 `[runtime]` TOML 输入 + 优先级链 | `runtime` | 最低（不改入口行为，纯地基）|
| **P1** | 侧车 JSON→TOML：launcher 读 `.runtimeconfig.toml`（Std.Toml）；`~/.z42/config.toml` 换 Std.Toml；`z42 publish` 侧车产出同步换 | `toolchain` | 低 |
| **P2** | `[profile.*]` 打通：z42c 解析 profile 段，运行路径消费 `mode` | `compiler` | 中（碰自举，需不动点验证）|
| **P3** | 统一前门 `RunEngine`：分类派发 + 设置解析 + `z42 run <dir>` = build（增量）+ run 编排 | `toolchain` | 中 |

---

## Scope（允许改动的文件 · 待 IMPL 起步时按锁细化）

> IMPL 起步前逐阶段查 ACTIVE.md。`runtime`/`toolchain` 现空闲，P0 可先起；`compiler` 现被占，P2 排队。

### runtime（P0）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/config.rs` | MODIFY | 补漏网旋钮进 KNOWN_KNOBS + `toml_key`；修描述失真；`RuntimeConfig` 分层解析 |
| `src/runtime/src/main.rs` | MODIFY | `--info` 枚举 schema；`Z42_CONFIG` 生效路径 |
| `src/runtime/src/jit/lazy.rs` | MODIFY | `Z42_JIT_PROFILE` 纳入 RuntimeConfig（去 straggler）|

### toolchain（P1/P3）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/toolchain/launcher/core/launcher.z42` | MODIFY | `_cmdRun` 分类器；`.runtimeconfig.toml`（Std.Toml）；`config.toml` 换 Std.Toml；`z42 run <dir>` = build+run 编排 |
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | 前门分类派发（目录 / 省略）|

### compiler（P2）
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | 解析 `[profile.*]` 段（现延后项）|
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY（若需）| profile 段解析补完 |

### docs
| 文件 | 变更 |
|------|------|
| `docs/design/runtime/runtime-settings.md` | NEW（设置优先级链 + 旋钮 SoT，配 mermaid）|
| `docs/design/runtime/launcher.md` | MODIFY（前门分类 + build+run 编排）|
| `docs/design/compiler/project.md` | MODIFY（`[runtime]` 段 + profile.mode 消费）|
| `docs/features.md` | MODIFY（设置优先级表）|
| `docs/roadmap.md` | MODIFY（源码工程运行能力状态）|

---

## 协调

- **锁排队**：P2 需 `compiler`（现被 `nested-types-followup` 占）。P0（runtime）/P1·P3（toolchain）现可起。IMPL 分阶段各自排锁，不一次性占多锁。
- REPL 内核（z42.scripting / z42i）本 change 不动，与 `perf-optimize-repl-eval` 等 REPL 相关 change 无 Scope 重叠。

---

## 未决

无。设计已简化定稿（2026-07-27，取消单文件）。P0 的 design/spec（[design.md](design.md) / [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)）不受简化影响，待 User 批准进 IMPL。
