# Tasks: fix-benchmark-bencher-arg-trampoline

> 状态：🟢 已完成 | 创建：2026-07-20 | 完成：2026-07-20 | 类型：fix（最小模式）
> 子系统：`compiler`（短占，User 授权预抢 split-irgen-class 锁，隔离 worktree）

**变更说明：** 把 `BenchmarkDesugar` pass 移植到 z42c——`[Benchmark] void f(Bencher b)`
（form-2 arg 形）AST-desugar 成 demoted `f$impl(Bencher b)` + 零参 `[Benchmark]` wrapper。

**原因：** 该 pass 原是 C# bootstrap 编译器实现（add-benchmark-bencher-arg-trampoline
2026-05-31），随 `f8ff73d5「删除 C# 编译器」`一起删除，**z42c 从未移植** → form-2 benchmark
自 C#→z42 编译器切换起一直运行期失败（`MethodInfo.Invoke: expects 1 argument, got 0`——
z42b 反射 runner 零参调用 [Benchmark]，而 TIDX 仍指向 1 参函数）。add-stdlib-bench-baseline
实施期发现 3 个 pre-existing form-2 基准失败，本 change 根治。

**文档影响：** `bench/README` 更新（form-2 现可用，删 stale「失败」注记）；z42.test README
既有「form-2 编译期 desugar」描述现已准确，无需改。pipeline passes 不单列 book（同 AttributeSynth
约定）。

- [x] 1.1 NEW `src/compiler/z42c.semantics/src/BenchmarkDesugar.z42`：`Run(cu)` 扫描顶层
      `[Benchmark]` 自由函数（单 Bencher 参）→ demote（renamed `$impl`、剥属性）+ 合成零参 wrapper
      （`Bencher b = new Bencher(); f$impl(b); b.printSummary("f");`，复用 md.RetType 免 respell void）
- [x] 1.2 4 处挂钩 `AttributeSynth.Run(X)` → `AttributeSynth.Run(BenchmarkDesugar.Run(X))`
      （IncrementalDriver.z42 ×1 + IrDump.z42 ×3）——parse 后、typecheck 前的既有 seam
- [x] 1.3 NEW `tests/desugar/bench_desugar_tests.z42`（+ 单元 toml）：6 [Test]（form-2 展开为
      impl+wrapper / wrapper 零参带 [Benchmark] / impl 无属性 / 零参·非-Bencher·无属性三类 pass-through）
- [x] 1.4 验证：
      - `test compiler`：**22 单元全绿（含 6 新 desugar 单测）+ 自举不动点 7/7 gen1==gen2 byte-identical**
        （z42c 源无 form-2 benchmark → Run 恒返回原 cu → 自编译零扰动）
      - `bench stdlib z42.math`：`bench_abs_loop(Bencher b)` PASS（samples=100 = 默认 Bencher）
      - `bench stdlib z42.test`：`bench_add_argform` / `bench_integer_sum_loop`（form-2）PASS
      - 3 个 pre-existing form-2 失败清零；`bench stdlib` 全绿
- [x] 1.5 文档：bench/README form-2 注记更新
- [x] 1.6 完整 gate 以 CI 为权威（冷环境）

## 备注
- **踩坑（自食其果·两次）**：`impl` 是 z42 保留关键字（`impl Trait for Type`），误用作局部变量名
  （BenchmarkDesugar.z42 + 测试各一次）→ 被本人前一 change `fix-parse-diags-dropped` 的关键字占名
  诊断当场抓出（"expected variable name"）。改名 `implName`/`implFn`。诊断上浮修复即时收益。
- `$impl` 后缀：`$` 在 z42 用户标识符非法 → 与用户代码零碰撞；z42c 内部 mangle 已用 `$`，
  含 `$` 的自由函数名正常流经 SymbolCollector/IrGen/z42b（实测 form-2 端到端 PASS）。
