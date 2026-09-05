# Runtime Benchmarks (criterion)

## 职责

z42 VM 内部模块的微基准。用 [criterion](https://bheisler.github.io/criterion.rs/) 框架，自动 warmup + 多次 iter + 统计中位数与 95% 置信区间。

`cargo bench` 不会被 `cargo build` 或 `cargo test` 触发，仅在显式 `cargo bench` / `just bench-rust` 时编译。

## 现有 bench 文件

| 文件 | 内容 | 状态 |
|------|------|------|
| `smoke_bench.rs` | criterion 框架 sanity check（纯 Rust 基线） | ✅ P1.A |
| `interp_bench.rs` | interp dispatch / call / 算术循环 | ⏳ P1.B/C |
| `gc_bench.rs` | alloc / collect / write barrier | ⏳ P1.B/C |
| `decoder_bench.rs` | .zbc 解码吞吐 | ⏳ P1.B/C |

## 运行

```bash
just bench-rust              # 跑全部 bench
cargo bench --bench smoke_bench   # 跑单个 bench 文件
cargo bench -- --quick       # 快速模式（每个 bench 总耗时上限缩短）
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

## 与 baseline 的对比（P1.D 加）

P1.D 引入：
- criterion 原生 `--save-baseline` / `--baseline`（同-runner 对照，见 `.github/workflows/bench-pr.yml` Part C）
- `z42 xtask.zpkg bench --diff` 与 baseline diff
- CI PR 阶段 quick 子集 + diff 性能门禁

详见 [docs/spec/changes/add-benchmark-framework/](../../../spec/changes/add-benchmark-framework/)。
