# Runtime Benchmarks (criterion)

## 职责

z42 VM 内部模块的微基准。用 [criterion](https://bheisler.github.io/criterion.rs/) 框架，自动 warmup + 多次 iter + 统计中位数与 95% 置信区间。

`cargo bench` 不会被 `cargo build` 或 `cargo test` 触发，仅在显式 `cargo bench` 时编译。

## 现有 bench 文件

| 文件 | 内容 |
|------|------|
| `smoke_bench.rs` | criterion 框架 sanity check（纯 Rust 基线） |
| `gc_cycle_bench.rs` | GC 环回收：`cycle_heavy_100`（大量小环）/ `shallow_tree_1k`（纯 mark）/ `large_array_10k`（大数组 mark） |

## 运行

```bash
cd src/runtime
cargo bench                          # 跑全部 bench
cargo bench --bench gc_cycle_bench   # 跑单个 bench 文件
cargo bench -- --quick               # 快速模式（每个 bench 总耗时上限缩短）
```

## 结果位置

- 文本输出：终端
- HTML 报告：`src/runtime/target/criterion/<bench-name>/index.html`（含分布图）

## 设计约定

- 每个 bench 文件 ≤ 200 行
- 用 `criterion::black_box` 防止 LLVM 优化消除被测代码
- 名称用 `<area>/<scenario>_<size>` 风格（如 `interp/arith_loop_1k`、`gc/alloc_small`）
- 同一 bench 文件内的 group 用 `criterion_group!` 聚合
- 不在 bench 中跑 IO（避免抖动）；如需 .zbc 输入，用 `include_bytes!`

## 与 baseline 的对比

criterion 原生 `--save-baseline` / `--baseline` 做同 runner 对照（CI 见 `.github/workflows/bench-pr.yml`）；端到端场景与 baseline diff 用 `xtask bench --diff`（`xtask bench -h`）。

## 待办
- `interp` dispatch / call / 算术循环 bench
- `.zbc` 解码吞吐 bench
