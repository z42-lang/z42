# Tasks: adopt-inline-env-knobs

> 状态：🟢 IMPL 完成（P0–P2 + 文档） | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/runtime-settings/spec.md)

## P0 — 进 RuntimeConfig（runtime/config）
- [x] 0.1 `RuntimeConfig` 加 8 字段：`jit_threshold: u32` / `osr_threshold: u32` /
      `jit_debug_promote: bool` / `no_fusion: bool` / `no_typed_fusion: bool` /
      `fusion_debug: bool` / `stackalloc: Option<String>` / `repl_native: Option<PathBuf>`
- [x] 0.2 解析：两个 threshold 复用 `parse_u32_knob(name, default)`（clamp ≥1，沿用现行
      `unwrap_or(d).max(1)` 语义）；四个 bool 用 `parse_bool_knob`；`stackalloc` 原样存串
      （消费点的 match 不动）；`repl_native` 存路径
- [x] 0.3 登记表：8 条 `sources` 由 `ENV_ONLY` 改 `LayerMask::ALL`；四个 `ValueKind::Flag` → `Bool`；
      `consumed_by` 去掉 "(inline env read)" 后缀
- [x] 0.4 单测：各旋钮四层可设；Flag→Bool 后 `=0/false` 真的关；`Z42_STACKALLOC=of`（typo）
      → 诊断 + 落默认；两个 threshold 的 clamp 与默认不变
- [x] 0.5 GREEN

## P1 — 消费点改读 runtime_config（runtime）
- [x] 1.1 `jit/mod.rs`：两个 threshold
- [x] 1.2 `jit/translate/mod.rs`：`jit_debug_promote`
- [x] 1.3 `metadata/superinstr.rs`：三个 fusion 旋钮（注意 `no_typed_fusion` 是**反向**——
      `typing_enabled = !no_typed_fusion`）
- [x] 1.4 `interp/stack_alloc.rs`：`mode()` 的 env 读换成 config 读，**保留 `AtomicU32` 缓存**
- [x] 1.5 `corelib/repl_native.rs`：`repl_native`
- [x] 1.6 GREEN + 手工验证 `--set jit-threshold=5` / `--set no-fusion=false` 真生效

## P2 — 防腐门收紧
- [x] 2.1 `inline_env_knobs_are_honest_about_their_layers` 反过来：断言**没有**旋钮再带
      "(inline env read)"，除非它标了 ENV_ONLY
- [x] 2.2 源码扫描门保持（它抓的是"读了但没登记"，与本项正交）
- [x] 2.3 GREEN

## 文档
- [x] `docs/book/src/runtime/runtime-settings.md`：删掉「尚未收编 → ENV_ONLY」那段，
      改写为「元旋钮 + 测试脚手架之外，全部四层可设」

## 未决
无。


## 落地记录（2026-09-05）

**反向防腐门当场抓到三个提案没料到的同类缺口**：`Z42_LIBS` / `Z42_PATH` /
`Z42_CRASH_DIR` 声称四层可设，实际全都只读 env——`--set libs=/x`、
`[runtime].path`、`[runtime].crash-dir` 至今**静默无效**。四个消费点
（`startup.rs` 三处 + `signal_handler.rs` 一处 + `main.rs` 的"用户是否已指定 libs"判断）
一并改读 `runtime_config()`。顺带修掉 `resolve_module_paths` 里硬编码的 `:` 分隔符
（Windows 上错），改用 `RuntimeConfig.module_path`（已按平台拆好）。

**门本身也被自己抓了一次**：`scan_env_literals` 递归时用 `strip_prefix(dir)` 而非
`strip_prefix(root)`，`config/parse.rs` 被记成 `parse.rs`，按目录豁免就失效了。

门现在是**精确的**：只把 `env::var(` / `env::var_os(` 那一行上的字面量算作"读"
（排除 `set_var`/`remove_var` 的写、排除当参数传的 key 如
`reject_flag_conflict(.., "Z42_MODE", ..)`），并放行元旋钮（`main()` 在**装配**层时
读它们，那时还没有解析结果可查）。

**手工验证**：`--set jit-threshold=5` → `[cli]`（此前报 cannot be set from [cli]）；
`--set no-fusion=false` → `false`（Flag 语义下会是"开"）；`[runtime]` 里设
`stackalloc`/`osr-threshold` → `[user-config]`；`Z42_STACKALLOC=of` →
`expected one of: on, off, 0, heap, stats`（此前静默当"开"）。

**GREEN**：runtime cargo 1145/0；release 无新告警（11 = main 基线）；e2e 566 +
cross-zpkg 17 + multi-exe 2；lines 33 known / 0 new-grown。

## 剩余
- `runtimeconfig.template.toml` 手写模板合并（独立项）
- iOS/Android/wasm 的侧车分发（`sidecar-reaches-published-apps` 的剩余项）
