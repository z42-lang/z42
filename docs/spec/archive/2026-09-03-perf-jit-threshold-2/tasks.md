# Tasks: JIT tier-up 阈值默认 1 → 2（perf-jit-threshold-2）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：perf（runtime；二轮清单 P1）
**变更说明：** `Z42_JIT_THRESHOLD` 默认由 1（首调即编）改为 2。N=1 会把每个进程里**只跑一次**的函数也全部 Cranelift
编译——对 GREEN 里成百上千次的短命 z42c 进程（每个 golden / stdlib 成员 / 测试编译）是纯税。语义不变（JIT 对语义透明），
仅「哪些函数何时被编」变；显式 env 仍生效。
**原因：** 同机数据——hello-world `--emit-zbc`：N=1 0.80 s（编 561 函数）/ N=2 0.46 s（编 35）/ N=3..10 0.46 s；
z42c.semantics 全量：N=1 12.80 s / N=2 12.83 s / N=5 12.96 s / N=50 13.41 s。z42c 自加载仅 40 ms、DepScan 0.23 s，
JIT 预热是小编译最大单项。
**文档影响：** `jit/mod.rs` 决策注释、`jit/frame.rs` 字段注释、`docs/book/src/runtime/jit-lazy-compile.md` 阈值段。

- [x] 1.1 `jit/mod.rs` `unwrap_or(1)` → `unwrap_or(2)` + 决策注释；`frame.rs` 注释；book 阈值段（含演进史）
- [x] 2. 对比数据：上述编译计时 + `xtask test` 墙钟（近期 GREEN 7:25–7:39）+ `xtask bench --ab --mode both`（长跑场景应无差；03_startup 应更快）
- [x] 3. `xtask test` GREEN + `e2e --mode jit`
- [x] 4. 归档

## 对比数据（2026-09-03，macOS arm64 同机；同一 VM 二进制，仅 `Z42_JIT_THRESHOLD=1` vs 默认 2）

**编译工作负载（本 change 的目标）**

| 场景 | N=1（原默认） | N=2（本 PR） | N=5 | N=50 |
|---|---:|---:|---:|---:|
| hello-world `--emit-zbc`（JIT 编译函数数）| 0.80 s（561）| **0.46 s（35）** | 0.46 s（31）| 0.47 s |
| z42c.semantics 全量 build | 12.80 s | 12.83 s | 12.96 s | 13.41 s |
| 完整 `xtask test`（同机同日）| 7:25 / 7:26 / 7:39 | **5:08** | — | — |

**e2e 运行时场景（hyperfine，warmup 2 / runs 12–20，确认不回归）**

| 场景 | base(N=1) ms | pr(N=2) ms | ratio |
|---|---:|---:|---:|
| 05_polymorphic_dispatch | 570.4 | 568.5 | 0.997 |
| 10_mono_vcall | 1294.8 | 1323.6 | 1.022 |
| 11_type_test_chain | 651.5 | 644.4 | 0.989 |
| 04_c2_p1_arith_loop | 70.5 | 69.8 | 0.991 |
| 06_thread_scaling | 83.8 | 80.8 | 0.965 |
| 02_math_loop | 35.1 | 34.1 | 0.969 |
| 01_fibonacci | 44.7 | 45.1 | 1.008 |
| 03 / 07 / 08（30–65 ms 短场景）| — | — | 0.986–1.010 |

长跑场景全部在 ±2% 内，短场景 8 runs 时曾出现 ±6% 摆动、加到 20 runs 后回落到 ±3.5%——即噪声，无系统性回归。
热循环由 OSR（`Z42_OSR_THRESHOLD`，独立机制）保底，不依赖函数级阈值。
