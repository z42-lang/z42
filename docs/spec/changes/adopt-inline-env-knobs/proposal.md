# Proposal: 收编 8 个内联 env 旋钮，让它们可从 CLI / 配置文件设置（adopt-inline-env-knobs）

> 状态：🔴 DRAFT | 创建：2026-09-05 | 类型：`vm` → 完整流程
> 前置：[`complete-runtime-settings`](../complete-runtime-settings/)（#443 已合并）
> SoT：`docs/book/src/runtime/runtime-settings.md`

---

## Why

`complete-runtime-settings` 建了「登记表是唯一 SoT + 五层链」，但有 8 个旋钮**只在
env 层可设**——它们在各自 `consumed_by` 处直接 `std::env::var`，CLI 与配置文件层
根本到不了那行代码。当时如实标了 `LayerMask::ENV_ONLY`（标成四层全收会让
`--list-knobs` 声称能 `--set` 而实际静默无效，比不登记更坏），并把收编列为剩余项。

| 旋钮 | 现读法 | 位置 |
|---|---|---|
| `Z42_JIT_THRESHOLD` | `var().parse::<u32>()`，默认 2，`.max(1)` | `jit/mod.rs:92` |
| `Z42_OSR_THRESHOLD` | 同上，默认 10000 | `jit/mod.rs:100` |
| `Z42_JIT_DEBUG_PROMOTE` | `var_os().is_some()` | `jit/translate/mod.rs:250` |
| `Z42_NO_FUSION` | `var().is_ok()` | `metadata/superinstr.rs:122` |
| `Z42_NO_TYPED_FUSION` | `var().is_err()`（反向）| `metadata/superinstr.rs:126` |
| `Z42_FUSION_DEBUG` | `var().is_ok()` | `metadata/superinstr.rs:130` |
| `Z42_STACKALLOC` | `var()` + 五值 match，缓存在 `AtomicU32` | `interp/stack_alloc.rs:176` |
| `Z42_REPL_NATIVE` | `var_os()` 路径 | `corelib/repl_native.rs:144` |

**用户可见后果**：`z42vm --set jit-threshold=5 app.zpkg` 会明确报「cannot be set from
[cli]」——诚实，但用户想做的事做不了。工程也没法在 `[profile.release]` 里固化
`jit-threshold`，尽管那正是这类调优旋钮最该待的地方。

**顺带的性能事实**：`std::env::var` 每次调用要加锁 + 查环境块 + 分配 `String`。
`runtime_config()` 是 `OnceLock` 的一次 acquire load + 字段读。收编是**净收益**，
不是为了统一性付性能税——`superinstr.rs` 那三个是**每次融合识别都读**（`fuse_blocks`
每个方法调一次），`repl_native.rs` 那个在 dlopen 候选枚举里读。

---

## What

### A. 8 个旋钮进 `RuntimeConfig`，`sources` 放开为四层全收

各自 `consumed_by` 处改读 `runtime_config().<field>`。已有的本地缓存
（`stack_alloc.rs` 的 `AtomicU32 MODE`）**保留**——它省的是重复 match，不是省 env 读。

### B. 四个 `Flag` 旋钮转成真 `Bool`（**有意的语义收紧**）

`Z42_NO_FUSION` / `Z42_NO_TYPED_FUSION` / `Z42_FUSION_DEBUG` / `Z42_JIT_DEBUG_PROMOTE`
现在是「存在即启用」——`Z42_NO_FUSION=0` **仍然关闭 fusion**。这是 shell flag 的惯例，
但它**活不过配置文件**：一旦这些旋钮能写进 `[runtime]`，`no-fusion = false` 必须表示
「不要关 fusion」，否则就是个陷阱。

故收编时一并转成 `ValueKind::Bool`：`0/false/off/no` = 关，`1/true/on/yes` = 开，
其它值 = 类型非法（诊断 + 用默认），与 `Z42_GC_TRACE` / `Z42_JIT_PROFILE` 一致。

**破坏面**：只有「显式把这些 env 设成 falsey 字符串却期望它生效」的用法会变。
全仓探查确认 `scripts/` / `.github/` / `docs/`（除归档 spec 的散文提及）**零消费方**，
且四个都是 `tier: Internal` 的调试开关。

### C. `Z42_STACKALLOC` 的 `Enum` 取值收敛

现状：`off` / `0` / `heap` → 关，`stats` → 统计，**其它任何值**（含 `on`、含拼错的
`of`）→ 开。登记表已按这个写了 `Enum(["on","off","0","heap","stats"])`，但解析器的
「其它一律开」意味着 typo 静默变成「开」。收编后由解析层的 Enum 校验兜住：表外的值
→ 诊断 + 落默认（开）。行为对合法值不变，对 typo 从静默变明说。

---

## What This Does NOT Do

- **不改这些旋钮的默认值与语义**（除 B/C 两条明说的收紧）。
- **不动 `Z42_STRESS_ITERS`**：它仍是 `ENV_ONLY` + `DebugOnly` + Internal 的测试脚手架，
  进 CLI 表面等于向用户暗示它是个正经旋钮。
- **不碰三个元旋钮**（`Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG`）的
  `CLI_ENV` 限制——写进配置文件会自指。
- 不做 `runtimeconfig.template.toml` 合并（独立项）。

---

## 三阶段

| 阶段 | 内容 | 风险 |
|---|---|---|
| **P0** | `RuntimeConfig` 加 8 个字段 + 解析；登记表 `sources` 放开、四个 Flag→Bool | 低 |
| **P1** | 8 个消费点改读 `runtime_config()`；保留本地缓存 | 中（碰 jit/interp/metadata，但都是读取点替换）|
| **P2** | 防腐门收紧：源码扫描门现在还允许「内联 env 读」，改为**只允许**表内标了 ENV_ONLY 的那些 | 低 |

## Scope
`src/runtime/src/config.rs` · `config/knob_table.rs` · `config/parse.rs` · `config_tests.rs` ·
`jit/mod.rs` · `jit/translate/mod.rs` · `metadata/superinstr.rs` · `interp/stack_alloc.rs` ·
`corelib/repl_native.rs` · `docs/book/src/runtime/runtime-settings.md`

## 未决
无。
