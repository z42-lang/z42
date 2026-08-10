# Design: 清单驱动的嵌入式 test-host（归一到 [Test] + bundle）

> 承接 add-embedded-app-run(#95) 的嵌入基座;把"跑测试用例"做成**清单驱动 + 可寻址命名用例 +
> 打包一个/全部 + 跑一个/全部**,并**归一到单一 Runner 路径**(User 定调)。desktop 先落地,
> wasm/ios/android 复用(同 agent + 同 `z42_host_run_app`)。

## 三种语料 + 归属

| 语料 | 形态 | on-device 跑? |
|------|------|:--:|
| golden e2e(`src/tests`) | 整程序 source.z42 + expected_output.txt(或 Assert 自验);cross-zpkg=多 zbc/多包 | ✅ VM **行为**测试 |
| [Test] 单元(`<lib>/tests`、z42c 单测) | `[Test]` 方法 | ✅ |
| Rust VM 单测(`src/runtime/*_tests.rs`) | 原生 Rust(测 VM **内部**) | ❌ 留 host `xtask test runtime` |

on-device 语料 = golden + [Test];Rust 内部测试是独立 host 层,不进 bundle。

## Decision A: 归一 —— golden 也变 [Test](User 定调)

**每个 golden 用例在 build 期生成一个 `[Test]` 包装**,让 agent **只有一条 `Std.Test.Runner` 路径**:

```
// 生成物（golden case <name> 的 wrapper，与其源码同模块编译）
[Test] void __golden__<name>() {
    string captured = Std.Test.TestIO.CaptureStdout(() => { <ns>.Main(); });
    Assert.Equal(<expected-inlined>, captured.TrimEnd());   // 无 expected → 仅跑 Main（Assert 自验）
}
```

- golden 的 Main + 期望输出在**同模块内**,调用直接、stdout 经 `Std.Test.TestIO` 捕获、`Assert.Equal` 比对。
- 好处:agent 单一模型(Runner),无 golden 专用判定路;报告格式天然统一。
- 代价:build 期一个 golden→[Test] **wrapper 生成步**(读 expected_output.txt → 内联 → 拼 wrapper 源 → 与 golden 源一起编)。

> 依赖:`Std.Test.TestIO` 的 stdout 捕获(builtin `__test_io_install_stdout_sink` 已存在)。
> 无 expected_output.txt 的 golden = Assert 自验证 → wrapper 只调 Main(抛异常即 fail)。

## Decision B: 用例 = 清单命名单元(点1)

- **可寻址单位是名字**(不是 zbc);一个用例可能编成**多个 zbc**(cross-zpkg)。build 解析
  "名字 → 它的 zbc 集 + kind + expected"。
- **结构**:沿用现有 `src/tests/manifest-targets` 的 `z42.toml [[test]] harness=` 模型,推广为整个
  语料的描述方式;现有 `src/tests` goldens 由一份**生成清单**归类(逐步可加最小声明)。
- **打包单个用例**:`--case <name>` → 只 build+打包该 target 的 zbc(集)→ 跑。平时单验省时。

## Decision C: bundle + filter(点2)

- **build → test-bundle** = 清单 `[{name, zbc:[...]}]`(归一后全是 unit kind)+ 全部 zbc + stdlib zpkg。
- **平台无关,编一次发所有平台**(激活之前暂缓的资产共享)。
- **agent 读清单**:`--filter <name/模式>` 跑子集,无则全部。归一后 agent 对每个 zbc 一律 `RunModule`。

## Architecture / 数据流

```
src/tests(golden) ──build期──> golden→[Test] wrapper ──┐
<lib>/tests([Test]) ───────────────────────────────────┤─ z42c 编 ─> zbc(集) ─┐
                                                        │                       ├─> test-bundle
                                                        └─ 生成清单 name→zbc ───┘   （+ stdlib zpkg）
                                                                                        │ 打包进 app（嵌入）
   xtask test embedded [--case <name>] [--filter <pat>]                                 ▼
        → agent（z42.testagent）读清单 → 逐 zbc Runner.RunModule（filter）→ 汇总 JSON
        → desktop: testhost(嵌入 VM) / mobile: 同 agent + z42_host_run_app + 各平台 asset bundle
```

## 命令面(重构 #96)

- `xtask test embedded --case <name>`:打包+跑单个用例(省时单验)。
- `xtask test embedded`:打包全部 + 跑全部。
- `xtask test embedded --filter <pat>`:打包全部、跑子集。
- Rust 内部测试仍走 `xtask test runtime`(host cargo,不嵌入)。

## Implementation Notes(分步)

1. **golden→[Test] wrapper 生成器**(build 期):枚举 golden case → 读 ns(源)+ expected → 生成
   wrapper 源 → 与 golden 源同编为一个含 `[Test]` 的模块。先验证 `TestIO.CaptureStdout` 能捕获
   golden 的 print(可能需给 Std.Test 补一个 `CaptureStdout(Action)` 便捷 API)。
2. **清单生成 + bundle**:枚举全语料(golden-wrapped + [Test] units)→ 编 zbc → 写清单
   `name→zbc(集)` → 组 bundle(zbc + stdlib)。`--case` 只做单个。
3. **agent 加清单+filter**:读 bundle 清单 → 逐 zbc `RunModule`(按 filter)→ 汇总一个 JSON。
4. **xtask test embedded** 重构:`--case`/`--filter` → build bundle → desktop testhost 跑 → 汇总。
5. mobile(后续 change):bundle 作 asset 打进 app;同 agent + `z42_host_run_app`。

## Testing Strategy
- 归一验证:一个 golden(有 expected)+ 一个 [Test] 单元,经 wrapper/清单/bundle → agent → 汇总
  JSON,pass/fail 正确;`--case`/`--filter` 选择正确。
- GREEN 以 CI 为权威(本地嵌入链已在 #95 实测;本 change 的 codegen+bundle 需健康工具链验证)。

## Deferred
- mobile 壳 + asset bundle 打包(复用清单+bundle+agent)。
- 全语料清单化的收尾(逐步给 src/tests 补声明 / 或纯生成)。
- persistent agent 命令通道 + 结果汇总成单 GitHub Check。
