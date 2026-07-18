# Design: 重建 bench 结构化输出

## Architecture

```
z42b bench --format json <artifact>
        │  builder_cli.z42: AddOption("format", …) → r.GetOption("format")
        ▼
builder_test.z42: _runModule(r, "bench") → Runner.RunModule(path, format)
        │
        ▼
Std.Test.Runner.RunModule(path, format)          [Runner.z42]
        │  ModuleLoader.Load → TestEntry[]
        │  for each [Test]/[Benchmark] entry:
        │     ┌─ pretty: 现有路径（直接 Console.WriteLine PASS/FAIL/SKIP）
        │     └─ json:   TestIO.captureStdout(() => invoke) → 捕获文本
        │               ├─ benchmark → BenchStats.parse(文本) → bench_stats   [BenchStats.z42]
        │               └─ 收集 TestResult{name,status,is_benchmark,reason?,bench_stats?}
        ▼
json 模式末尾：TestReport.toJson(results) → 单个 JSON 对象 → Console.WriteLine   [TestReport.z42]
```

## Decisions

### Decision 1: 捕获 stdout 解析 vs 让 benchmark 返回 Bencher

**问题：** Runner 经 `__invoke_static` 调 benchmark，只拿到函数返回值（void→null），拿不到
benchmark 内部构造的 `Bencher` stats。如何取得结构化 stats？

**选项：**
- A — **捕获 stdout + 解析 `bench[...]` 行**（旧 Rust runner 同款契约）。只动 stdlib+toolchain。
- B — **改 `[Benchmark]` desugar 让函数返回 Bencher**，`__invoke_static` 直接拿 stats 对象。
  结构化数据结构化流动，更根因；但**要动 z42c 编译器**。

**决定：选 A。** 理由：①B 需改 `compiler` 子系统，而该锁被 `split-irgen-class` 占用 → 越界
（parallel-development.md 跨子系统占全部锁，任一被占即排队）；②A 是把旧 runner 早已确立的
「printSummary 格式即 runner 契约」在 z42 里重实现，不是新发明的启发式——benchmark 的**全部**
输出就是那一行，解析它是确定性的、不是「猜」；③A 完全留在本变更的 stdlib+toolchain 锁内。

> 与 philosophy「修复从根因/不降级 sentinel」的关系：A 的 `parse` malformed 时**返回 null**
> 而非 sentinel，消费端（Runner）据 null 决定「该 benchmark 无 stats」，不下游猜测。契约的
> 「根因正确」由 `Bencher.printSummary` 单点产出 + `BenchStats.parse` 单点消费保证——格式改动
> 两端同提交（doc 注释已明写）。B 是「更根因」但受锁阻断，记为 Deferred（见下）。

### Decision 2: JSON 手写 vs 依赖 z42.json

**问题：** `z42.test` 的 `[dependencies]` 为空（foundational 库，被所有其它库测试依赖）。

**决定：** **手写 JSON 序列化**（`TestReport.z42` 内一个最小转义器 + 字段拼接）。引 z42.json 会
给 foundational 库加依赖、可能引环，且 z42.test 设计原则本就是「不依赖 native 库 / 纯脚本」
（README「设计选择」）。schema 扁平、字段固定，手写成本低、可控。

### Decision 3: JSON schema —— 精简旧 Rust schema

**问题：** 旧 Rust `TestResult` 有 `duration_ms`/`failure_location`/`stack_trace`；z42 in-process
runner 无逐测计时、失败位置信息也不成熟。

**决定：** **精简**。z42 报告 schema：

```json
{
  "tool": "z42b",
  "module": "<path>",
  "summary": { "total": N, "passed": P, "failed": F, "skipped": S },
  "results": [
    { "name": "<FQN>", "status": "passed|failed|skipped", "is_benchmark": false },
    { "name": "<FQN>", "status": "failed", "is_benchmark": false, "reason": "<Type>: <msg>" },
    { "name": "<FQN>", "status": "passed", "is_benchmark": true,
      "bench_stats": { "label": "…", "min_ns": n, "median_ns": n, "max_ns": n, "samples": n } }
  ]
}
```

省去 `duration_ms`/`failure_location`/`stack_trace`（无可靠来源，宁缺勿造）；保留旧核心
（`is_benchmark` + `bench_stats` + summary）。`total_ns` 不入（`printSummary` 行不含 total，
不改该行格式 → 无来源）。pre-1.0 无消费方，精简不破坏兼容（philosophy 不为旧版本兼容）。

### Decision 5: 规避两个 nightly-z42c bug —— Runner 内联装配 + 仅 1-arg TestReport 助手

**实施期用 nightly z42c 编 z42.test 时撞上两个 z42c 前端 bug**（二分探针 A–S 定位；均属被占的
compiler 子系统 → 本变更规避不修）：

1. **数组类型参数误解析**：`toJson(string, TestResult[])` 报 `E0401: unknown type in `new`: ]`
   ——`T[]` 参数被误当 `new T[...]`（`T` 为用户类、叠加类前含 `[]` 注释 / 跨文件类字段时触发）。
   `TestEntry[]`（Runner 既有）幸免（跨文件、仅基元字段）。
2. **多参 brace-body 方法 TSIG 丢导出**：`report(module,total,passed,failed,skipped,body)`（6 参、
   含 int、体内有 `{`/`[`/`}` 字符串字面量）**编译通过但静默不进 TSIG 导出**（跨包加载即
   `undefined function TestReport.report`）；`resultJson`（1 参）/`esc`（1 参）稳定导出。

**决定：** ① `TestReport` **不暴露多参 envelope 方法、不接收数组参数**——只留 `resultJson(TestResult)`
+ `esc(string)`（皆 1 参，稳定导出）+ 私有 `_statsJson`；② **报告 envelope（`{tool,module,summary,
results}`）由 `Runner.RunModule` 内联装配**——它本就持计数为局部量，遍历**局部** `TestResult[]`
（局部 `new T[]` 不触发 bug1，探针 G/确认）调 `resultJson` 拼 body，再用 `esc(path)` + 字符串拼接
成 envelope。`RunModule`（2 参）实测导出正常。两 bug 物理规避，且 per-result 渲染可复用。记 Deferred
`bench-structured-future-report-envelope`。

### Decision 4: 捕获仅在 json 模式

**问题：** pretty 模式是否也捕获+解析（以填 bench_stats）？

**决定：** **仅 json 模式捕获**。pretty 模式是人眼路径，无需结构化 stats，保持现有路径逐字节
不变（回归零风险）。json 模式才 `TestIO.captureStdout` 包裹每个 entry 调用。这样 pretty 的
GREEN 断言（现有 bench_demo / test_runner golden）完全不受影响。

## Implementation Notes

- **`BenchStats.parse`**：定位 `bench[` → `]` 取 label；其后按 `min=`/`median=`/`max=`/`samples=`
  逐 key 取值（`ns` 后缀在时间字段上剥离）；任一 key 缺失或非整数 → 返回 null。多行时扫最后一个
  合法行（先按行切，从末尾找第一个 parse 成功的）。纯字符串处理，无 native。
- **capture 复用**：`TestIO.captureStdout(() => ModuleLoader.Invoke(e.Qualified))`。注意 setup/
  teardown 在 benchmark 之外运行——json 模式下把 setup→invoke→teardown 都纳入捕获，但只对
  benchmark 的捕获文本跑 `BenchStats.parse`（setup/teardown 不产 bench 行，无害）。
- **Runner 重构**：抽 `_runOne` 为「执行 + 返回 TestResult」；pretty 模式内联打印，json 模式收集。
  为控 `Runner.z42` 行数（现 146），把 JSON 序列化下沉 `TestReport.z42`，parse 下沉 `BenchStats.z42`。
- **文件行数**：三文件均预期 <300 软限；Runner 增量后核对不越 500 硬限。
- **z42b flag**：`test.AddOption("format", "", "output format: pretty|json", "pretty")`，dispatch 侧
  `r.GetOption("format")` 传入。`bench` 同。`_runModule(r, verb)` 签名加 format 读取。

## Testing Strategy

- **单元（`tests/bench_stats.z42`，`[Test]`）**：镜像旧 Rust `bench_stats_*`——canonical 解析、
  无 bench 行→null、malformed→null、多行取末行；再加 `TestReport.toJson` 对固定 results 的 shape
  断言（含转义：reason 带引号/换行）。这些输入**确定性**，不受计时抖动影响。
- **回归保护**：现有 `xtask test stdlib z42.test`（含 dogfood/test_runner golden）证 pretty 路径不变。
- **集成冒烟**：GREEN 时手动 `z42b bench --format json <编译后的 bench_examples.zpkg>`，肉眼验 JSON
  含 `bench_stats`（ns 值非确定，不进 golden）。
- **完整 GREEN**：`xtask test`（cargo build z42vm + e2e + cross-zpkg + stdlib + compiler + vscode-syntax）。

## Deferred / Future Work

### bench-structured-future-report-envelope

- **来源**：本 change 实施期（Decision 5）
- **触发原因**：nightly z42c 两 bug——(1) 数组类型参数 `T[]`（用户类元素）误解析为 `new T[...]`；
  (2) 多参 + brace-body 的 public static 方法编译通过却不进 TSIG 导出。compiler 锁被占，前端修复
  不在本变更范围。
- **前置依赖**：`compiler` 锁释放 + z42c 修上述两处（parser array-param + TSIG 导出）。
- **触发条件**：修好后可把 envelope 收敛回单个 `TestReport.report(module, counts…, TestResult[])`
  自然 API，`Runner` 不再内联装配；并允许其它 stdlib 放心用 `T[]` 参数 + 多参 JSON builder。
- **当前 workaround**：`TestReport` 只留 1-arg `resultJson`/`esc`；envelope 由 `Runner.RunModule`
  内联拼（局部 `TestResult[]` 遍历，局部 `new T[]` 不触发）。
- **复现**：① `toJson(string, TestResult[])` + 类前 `[...]` 注释 → `unknown type in new: ]`；
  ② `report(string,int,int,int,int,string)` 体含 `"{"`/`"]}"` → 编译过但 `undefined function` at load。

### bench-structured-future-return-bencher

- **来源**：本 change design Decision 1
- **触发原因**：Option B（benchmark desugar 返回 Bencher，结构化数据结构化流动）需改 z42c 编译器，
  而 `compiler` 锁被占；且当前 A 路径已满足「结构化输出」需求。
- **前置依赖**：`compiler` 锁释放 + z42c `[Benchmark]` desugar 支持返回值。
- **触发条件**：当 stdout-capture 路径暴露出脆弱性（如 benchmark 中途打印污染解析），或做独立
  `z42.bench` 包（roadmap 0.4.x B1）时一并根治。
- **当前 workaround**：A 路径——`printSummary` 单点产出 + `BenchStats.parse` 单点消费，格式契约
  两端同提交。
