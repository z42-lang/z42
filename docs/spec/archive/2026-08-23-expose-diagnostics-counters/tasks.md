# Tasks: 运行时计数暴露给 z42（Std.Diagnostics.RuntimeStats）

> 状态：🟢 已完成 | 创建：2026-08-23 | 完成：2026-08-23

## 进度概览
- [x] 阶段 1: Rust builtin + 注册
- [x] 阶段 2: z42 stdlib 面
- [x] 阶段 3: CI informational gate
- [x] 阶段 4: 测试 + 文档 + 验证

## 阶段 1: Rust builtin
- [x] 1.1 `corelib/diagnostics.rs`：加 `const STD_RUNTIME_COUNTERS: &str = "Std.Diagnostics.RuntimeCounters"`（FQN 字符串常量）
- [x] 1.2 `corelib/diagnostics.rs`：`builtin_diag_counters` —— snapshot+stats → `alloc_named` 投影 11 字段
- [x] 1.3 `corelib/mod.rs`：`BUILTINS` 末尾 append `("__diag_counters", diagnostics::builtin_diag_counters)`（日期注释，不插中间）

## 阶段 2: z42 stdlib
- [x] 2.1 `z42.diagnostics/src/RuntimeCounters.z42`：11 只读 auto-property 值对象
- [x] 2.2 `z42.diagnostics/src/RuntimeStats.z42`：`static class RuntimeStats` + `[Native] extern Counters()`（**改名避 `Std.Runtime` 冲突**，见 design D7）
- [x] 2.3 `z42.diagnostics/src/README.md`：功能索引 + 核心文件表同步（六段制）

## 阶段 3: CI informational gate
- [x] 3.1 判定 gate 采集：`xtask profile --threads <scn> --mode interp` 输出 `alloc=N`（确定性，跨 GC-mode 一致，本地实证 08_dict_heavy=50071 / 01_fibonacci=30）
- [x] 3.2 `.github/workflows/bench-pr.yml`：加 informational allocations 步（2 场景 × 2 GC-mode 打印 alloc，`|| true` 永不 fail）

## 阶段 4: 测试 + 文档 + 验证
- [x] 4.1 `corelib/diagnostics_tests.rs`：builtin append-only 注册单测（2 pass）
- [x] 4.2 `z42.diagnostics/tests/runtime_counters.z42`：4 个 [Test]（全 11 字段可读 / alloc 增 / 单调 / 异常字段单调）—— 5/5 lib 文件全过
- [x] 4.3 `cargo build --release` + `cargo test --lib diagnostics`（2 pass）
- [x] 4.4 `xtask test`（完整 GREEN gate）—— 全 stage 绿（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）
- [x] 4.5 spec scenarios 逐条覆盖确认（见下）
- [x] 4.6 文档同步：diagnostics.md §5 刷 P1c 落地 + README 六段 + design D7 命名冲突记录
- [x] 4.7 self-host 5/5 gen1==gen2 逐字节 + bootstrap 无越界（z42c 源未变）

## 备注
- **根因排查大坑（design D7）**：类名 `Runtime` 撞 z42.core prelude 的 `Std.Runtime.Runtime`
  → inline `Runtime.Counters().Allocations` 静默误绑返 Null（stored-local 正常）。改名 `RuntimeStats` 修复。
  实测排除了 Rust builtin 分配路径（手建 vs alloc_named 都 Null）→ 确认是命名冲突非编译器 bug。
- 无格式 bump（builtin 运行时解析，`[Native]` 字符串）；本地完整 GREEN 可验。
- 异常计数 interp-only（JIT 不递增，已知 gap）→ test_exception 用 mode-robust `>=`。
