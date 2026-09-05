# z42 测试框架（Testing）

> **运行器现为 z42b（retire-test-runner，2026-06-30）**：`[Test]`/`[Benchmark]` 由
> 用 z42 自身写的 `Std.Test.Runner`（z42.test 库）执行，承载于 `z42.builder.zpkg`
> （z42b），经 `z42vm` 运行——取代了原 Rust `z42-test-runner`（`src/toolchain/
> test-runner/` 已删）。发现走 `Std.Test.ModuleLoader.Load`（加载进活 VM + 返回 TIDX），
> 执行走 `__invoke_static` 按 FQN 调用 free-function 测试，退出码即结果。
>
> 本文下方涉及 **Rust runner 专属的 CLI / 多格式输出（TAP / JUnit / JSON、`--filter`
> / `--list` / `--platform`）** 的小节描述的是被取代的实现：z42b 当前只产 pretty 文本 +
> 退出码（xtask 按退出码聚合），结构化格式列为后续工作。skip 平台/特性判定逻辑同样
> 移到 z42 侧（`Std.Test.Runner._skipApplies`）。

## 设计目标

参 Rust libtest / Go testing / xUnit 的成熟模式，让 z42 提供：

1. **编译时测试发现**：编译器扫描 `[Test]` 等 attribute → zbc 中的 `TIDX` section；运行时**不**扫整个 method table
2. **结构化 assertion**：`Assert.eq` / `Assert.throws<E>` / `Assert.near` 等替代 stdout 字面量比对
3. **条件跳过**：`[Skip(platform: "ios", feature: "jit", ...)]` 按平台 / 特性自动决定是否运行
4. **多层组织**：单元 / stdlib API / VM × 脚本工程级集成 各自归位
5. **跨平台一致**：同一份 `.zbc` 在 desktop / wasm / iOS / Android 跑相同结果（详见 [cross-platform-testing.md](cross-platform-testing.md)）

## 架构总览（R 系列）

```
   .z42 源码                   C# 编译器                     zbc 二进制
   ─────────                   ─────────                     ───────────
   [Test]                      Lex / Parse / TypeCheck      TIDX section:
   fn test_x()         ──►     AttributeBinder         ──►    [
                               收集 z42.test.* attrs           {method_id, kind=Test, ...},
   [Skip(platform:             写 IrModule.TestIndex           {method_id, kind=Setup, ...},
     "ios", reason: ...)]      [R4: 校验签名]                  ...
                                                              ]
                            ────────────────────────────────────────────────
                            ▼

   Rust 端 (R1)
       LoadedArtifact.test_index                       z42.test 库 (R2)
        ↑ load_artifact 读 TIDX                       ↳ Test/Skip/Setup/...
        ↓                                              ↳ Assert.* / TestIO.*
                                                       ↳ Bencher.iter
   z42b（z42.builder.zpkg，反射式 runner）──────────►
       ↳ Std.Test.ModuleLoader.Load(artifact) → 加载进活 VM + 返回 TIDX 条目
       ↳ Std.Test.Runner.RunModule：遍历条目
       ↳ 同命名空间 [Setup] → 反射调用 [Test]/[Benchmark] free function → [Teardown]
       ↳ catch TestFailure / SkipSignal；[ShouldThrow<E>] 链匹配
       ↳ pretty 输出 + 退出码（0=全过 / 1=有失败）
```

> **runner 即 z42b**（retire-test-runner）：测试运行器是用 z42 自身写的
> `Std.Test.Runner`，由 `z42.builder.zpkg`（z42b）承载、经 `z42vm` 运行——取代了原
> 先用 Rust 写的 `z42-test-runner`。它用反射（`Std.Test.ModuleLoader.Load` +
> `__invoke_static` 按 FQN 调用 free function）发现并执行 `[Test]`/`[Benchmark]`，
> 退出码即结果。执行模式（interp / jit）由承载 z42b 的 `z42vm --mode <mode>` 决定：
> runner 与被加载的测试函数在同一 VM、同一模式下执行（jit 覆盖即 `z42vm --mode jit`
> 运行 z42b，无需 per-test fork）。

R1 已落地 (commits ea54554 / bb2df98 / 5180d21)：编译时发现 + 6 个 attribute (`[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` / `[Ignore]` / `[Skip]`) + zbc TIDX v=2 section。

**待落地**：

- **R2** — z42.test 库扩展（Assert API + TestIO.captureStdout + Bencher.iter + native helpers）
- **R3** — runner（现为 z42b：`Std.Test.Runner` + pretty 输出 + Setup-Teardown 调度 + Bencher 执行）
- **R4** — 编译期 attribute 校验（Z0911-Z0915）含 `[ShouldThrow<E>]` + `[TestCase(args)]`
- **R5** — stdlib 各库 `tests/` 补本地原生测试（不大规模迁移现有 golden）

> **TAP / JUnit / JSON 输出与 `--filter` 等富格式**是原 Rust runner 的能力；z42b 当前
> 只产 pretty 文本 + 退出码（xtask 按退出码聚合即可）。结构化输出格式列为后续工作
> （见 retire-test-runner 归档的 Deferred）。

---

## 执行模式：in-process vs subprocess、interp vs jit（parallelize-and-jit-stdlib-tests, 2026-06-21）

runner 有两条执行路径，互不相同的两件事（不要混淆）：

| 维度 | in-process（R3b 默认） | subprocess（fallback） |
|------|----------------------|----------------------|
| 实现 | `bootstrap.rs` 建 VmContext，`runner.rs` 直调 `interp::run_outcome` | `exec.rs` fork `z42vm <zbc> <method>` 每 test 一个进程 |
| Setup/Teardown | ✅ 跑（共享 VmContext 链） | ❌ 不跑（独立进程，无共享态） |
| 触发 | 默认 | `--jobs N>1`（VmContext `!Send`）或 `--legacy-subprocess` 或 `--mode jit` |

**`--mode interp|jit`（exec mode）与上面的进程模型是正交但耦合的**：

- **interp** → in-process（`runner.rs` 硬编码 `interp::run_outcome`，从不读
  `LoadedRunner.vm`）。
- **jit** → **强制 subprocess**。根因：in-process runner 直调 interpreter，没有
  driving JIT 的路径（要支持得让 `runner.rs` 走 jit dispatch，是更大的改动）。
  而 fork 的 `z42vm --mode jit` 已有完整 jit 路径 + transitive eager-load
  （`main.rs` 5.1d），直接复用。代价：jit 下 Setup/Teardown 不跑。
- `--mode` 也透传给 subprocess 路径 fork 的 z42vm（`exec.rs` / `parallel.rs`），
  否则 z42vm CLI 默认（jit）会与 runner 的 interp 意图不一致。

> **为何不做 in-process jit**：runner.rs 的 `interp::run_outcome` 是测试执行的
> 唯一入口，jit 化需要把 Setup/Test/Teardown 三处都改走 jit dispatch。当前阶段
> ROI 不足——subprocess jit 已能覆盖"stdlib 在 JIT 下是否与 interp 分歧"这一目标
> （CI `stdlib-jit-consistency` job）。若将来 in-process jit 有需求，再实现。

> **xtask `--jobs` ≠ runner `--jobs`**：xtask 的 `test stdlib --jobs N` 是
> **unit 级并行**（N 个 unit 同时 compile+run，每个 runner 仍 in-process 串行 →
> 保留 Setup/Teardown）；runner 自身的 `--jobs N` 是 test 级并行（强制 subprocess）。
> xtask 现在用前者，不再给 runner 传后者。

---

## 测试目录组织（约定，2026-05-05 dotnet/runtime-style）

> **⛔ 已迁移 + 冻结（2026-07-16，review §4.6）**：本节与下方「添加新测试时的归属规则」的
> **当前、去 C# 化**版本已迁到 live 文档 [`docs/workflow/testing/README.md` §测试文件归属](../../workflow/testing/README.md#测试文件归属放哪--加新用例往哪放)。**以 live 文档为准**——下方原文含 C# 时代残留
> （`z42.Tests` xUnit / `dotnet test` / `z42-test-runner` / `z42.Tests/Fixtures/{parse,errors}`
> 均已不存在），仅作历史留存，不再更新。

z42 测试组织对标 [dotnet/runtime](https://github.com/dotnet/runtime/tree/main/src) 的成熟模式 ——「被测对象在哪，测试就在哪」+「中央 VM 测试集按特性分类」。

```
src/
├── compiler/z42.Tests/                # 编译器单元测试 (C# xUnit)
├── runtime/
│   ├── src/<mod>_tests.rs             # VM Rust 单元测试（per .claude/rules/runtime-rust.md）
│   └── tests/                         # cargo 框架强约定的 Rust 集成测试
│       ├── zbc_compat.rs              # C# → Rust zbc 解码契约
│       ├── native_*.rs                # native interop / pin / opcode-trap e2e
│       ├── manifest_schema_validation.rs
│       └── data/                      # 测试 fixtures
├── libraries/<lib>/tests/             # stdlib 库本地测试（拍平，无 golden/ 中间层）
│   ├── <NN>_<name>/                   # golden 用例（source.z42 + expected_output.txt）
│   └── *.z42                          # [Test] 格式（z42-test-runner 调度）
└── tests/                             # ★ 中央 VM e2e 测试根（dotnet/runtime/src/tests/ 对标）
    ├── README.md
    ├── basic/         # 基础（hello / fibonacci / arrays / namespace / assert dogfood）
    ├── exceptions/    # try/catch/finally, stack trace, exception subclass
    ├── generics/      # generic function / class / constraints / instantiation / interface dispatch
    ├── inheritance/   # virtual / abstract / multilevel
    ├── interfaces/    # multi-interface / property / IComparer / interface event
    ├── delegates/     # delegate / multicast / event / nested
    ├── closures/      # lambda / closure / local fn
    ├── gc/            # GC cycle / collect / weak ref / weak subscription
    ├── types/         # enum / struct / record / typeof / is/as / nullable / numeric aliases / char
    ├── control_flow/  # switch / do-while / null-coalesce / null-conditional / loop control
    ├── operators/     # bitwise / increment / logical / comparison / overload
    ├── refs/          # ref / out / in
    ├── classes/       # class / namespace / access / static / auto-property / ctor / indexer
    ├── strings/       # string builtin / methods / static methods / edge cases
    ├── parse/         # 仅 ZASM-match（无 .zbc / 无 stdout 比对）
    ├── errors/        # 编译失败用例（expected_error.txt）
    └── cross-zpkg/    # 多 zpkg 端到端（target / ext / main 三方）
```

### 各层职责

| 目录 | 形态 | 谁运行 | 用例风格 |
|------|------|-------|---------|
| `src/compiler/z42.Tests/` | C# xUnit project (`.csproj`) | `dotnet test` | 编译器单元测试 (Lexer/Parser/TypeCheck/IRGen) + walks `src/tests/` + `src/libraries/<lib>/tests/` 跑 GoldenTests |
| `src/runtime/src/<mod>_tests.rs` | Rust 单元测试 | `cargo test` | VM 模块单测（GC / interp / decoder ...） |
| `src/runtime/tests/*.rs` | Rust 集成测试 | `cargo test` | 跨语言契约 + native interop e2e（cargo 框架硬约定位置） |
| `src/libraries/<lib>/tests/<name>/` | `.z42` source + expected_output.txt | `./xtask test e2e` + xUnit GoldenTests | 库 API 行为校验（与 [Test] 文件共居）|
| `src/libraries/<lib>/tests/*.z42` | 单文件，含 `[Test]` 注解 | z42-test-runner | 库 [Test] 单元测试 |
| `src/tests/<category>/<name>/` | `.z42` source + expected_output.txt | `./xtask test e2e` + xUnit GoldenTests | VM e2e 按特性分类 |
| `src/compiler/z42.Tests/Fixtures/parse/<name>/` | `.z42` + expected.zasm | xUnit GoldenTests::ParseTests | IR/ZASM 匹配（无 VM 执行）|
| `src/compiler/z42.Tests/Fixtures/errors/<name>/` | `.z42` + expected_error.txt | xUnit GoldenTests::ErrorTests | 编译失败诊断匹配 |
| `src/tests/cross-zpkg/<name>/` | target/ext/main 三 toml 工程 | `./xtask test e2e --dir cross-zpkg` | 多 zpkg 链接 + 跨包 IR/TSIG 解析 |

### 添加新测试时的归属规则

按以下顺序判断（先到先得）：

1. **库 API 行为？** → `src/libraries/<lib>/tests/<name>/`（与库源码同居）
2. **编译失败用例？** → `src/compiler/z42.Tests/Fixtures/errors/<name>/`（用 `expected_error.txt`）
3. **仅 ZASM 匹配（不需要 VM 执行）？** → `src/compiler/z42.Tests/Fixtures/parse/<name>/`
4. **跨多个 zpkg？** → `src/tests/cross-zpkg/<name>/`
5. **其他 VM/编译器特性？** → `src/tests/<category>/<name>/`
   - 不确定类别时归 `basic/`，后续可重新分类

### 用例文件约定

| 文件 | 何时存在 | 含义 |
|------|---------|------|
| `source.z42` | 必须 | z42 源码 |
| `source.zbc` | 可执行测试 | 由 `./xtask build test` 生成；**不与 `source.z42` 同处**——run-golden 编译产物按组件镜像 src 布局到 `artifacts/build/`：`src/tests/X`→`artifacts/build/tests/X`，`src/libraries/<lib>/tests/X`→`artifacts/build/libraries/<lib>/tests/X`（gitignored，不污染 src；与 stdlib/z42c 包构建落点一致）。唯一例外 `src/tests/zbc-format/*/source.zbc` 是 check-in 的字节基线，就地重写（`git diff` = 格式漂移） |
| `source.zasm` | 可选 | 调试用 ZASM 文本 |
| `expected_output.txt` | run 用例 | stdout 期望（**空文件 = 用 `Assert.*` 自验，删除即可**）|
| `expected_error.txt` | error 用例 | 编译诊断期望 |
| `expected.zasm` | parse 用例 | IR ZASM 期望 |
| `features.toml` | 可选 | LanguageFeatures override |
| `interp_only` | 可选 marker | JIT 模式跳过 |

### `expected_output.txt` 处置（2026-05-05）

- **非空文件**保留，`./xtask test e2e` 用于 stdout 比对（103 个用例）
- **空文件**已删除（16 个），那些用例完全靠内置 `Assert.Equal` 自验：成功 = 跑通无 stdout 输出
- 测试 runner（xtask test e2e / GoldenTests.cs / ./xtask test dist）在文件缺失时把期望视为空字符串
- 等 R3 z42-test-runner 落地后，由独立 spec 评估是否把 stdout 比对全部转为 [Test]+Assert

---

## R4.B Generic Attribute Syntax `[Name<TypeArg>]`（2026-04-30）

R4.A 落地了 6 个 z42.test attribute 的解析与签名校验；R4.B 增补**单类型参数泛型 attribute**语法，唯一即时用例是 `[ShouldThrow<E>]`（z42.test 库自检"应抛 E 类型异常"的负向路径）。

### 语法

```z42
[Test]
[ShouldThrow<TestFailure>]
void test_assert_fail_throws() {
    Assert.Fail("expected to fail");
}
```

- 单类型参数；多参 `[X<A, B>]` 和嵌套 `[X<List<int>>]` 报 parser 错（`E0202`）
- Parser 接受任意 `[Name<T>]` 写法，**哪些 attribute 允许 type arg** 由语义校验（E0913）决定
- 类型参数允许短名（`TestFailure`），与 `class X : Exception` 一致；TIDX 写源码原文，运行时按需规范化

### 编译期写入 TIDX 流程

```
parser collects [ShouldThrow<E>]
  ↓ TestAttribute.TypeArg = "E"
TestAttributeValidator (E0913 / E0914 checks)
  ├─ TypeArg 必填（不能裸 [ShouldThrow]）
  ├─ 类型必须存在于 SemanticModel.Classes
  ├─ 类型必须继承 Exception（沿 BaseClassName 链回溯）
  └─ ShouldThrow 必须配 [Test] / [Benchmark]（修饰符语义）
  ↓
IrGen 写入：TestEntry.ExpectedThrowTypeIdx = pool.intern("E") + 1
            TestFlags.ShouldThrow 位置位
  ↓ TIDX section v=2 字段已在 R1.C 预留
ZbcReader → Rust loader → resolve_test_index_strings
  → TestEntry.expected_throw_type = Some("E")
```

### Runtime 比对（A2，2026-04-30）

z42-test-runner 读 TIDX `expected_throw_type` 比对实际抛出：

- **未抛**（exit 0）→ Failed `expected to throw <E>, but no exception was thrown`
- **类型匹配**（FQ 相等 OR 短名相等，对 chain 中任一 entry 匹配即算）→ Passed
- **类型不匹配** → Failed `expected to throw <E>, got <X>`

类型提取：从 stderr 的 `Error: uncaught exception: ` 后取 `[A-Za-z0-9_.]+`，覆盖 `<TYPE>: <msg>` 与 `<TYPE>{field=...}` 两种 z42vm 输出格式。

### Inheritance 比对（A3，2026-04-30）

`[ShouldThrow<Base>]` 也匹配 Base 的子类（编译期展开方案，运行时无需类型反射）：

- **C# IrGen 端**：`BuildShouldThrowChain(typeArg)` 遍历 `SemanticModel.Classes`，把 `typeArg` + 所有从 `typeArg` 派生的类的短名拼成 `;`-delimited 字符串写入 TIDX `expected_throw_type` 槽。例如 `[ShouldThrow<Exception>]` 在 z42.test dogfood 的 CU 里展开为 `"Exception;TestFailure;SkipSignal"`。
- **Runner 端**：split `expected_throw_type` 后任一 entry 命中即 Pass；同样的 `type_matches`（FQ vs 短名）规则
- **覆盖范围**：仅当前 CU 的 `SemanticModel.Classes` 可见类（含 `using` 引入的 import）；不在 import 链路上的 zpkg 依赖不会枚举（这些场景下 fallback 到直接匹配）
- **零格式 bump**：TIDX layout 不变；`expected_throw_type` 字段语义从"单个类型名"扩展为"类型名或 `;`-delimited list"

### 当前不做的

- ⏸️ 跨非 import zpkg 依赖的 inheritance（要求 runner 知道完整类型层次，需做 LazyLoader 集成 → 由 R3 完整版承担）
- ⏸️ 多类型参数 `[X<A, B>]` / 嵌套 `[X<List<Y>>]` / dotted name `<Std.E>`

---

## Runner 输出格式（R3a，2026-04-30）

`z42-test-runner --format <pretty|tap|json|junit>` 四选一。`--filter <SUBSTR>` 按方法名 substring 筛选；多个 `--format` 等价于最后一个。

> **stdout 纯净保证**（add-junit-xml-formatter, 2026-05-31）：runner 捕获
> 每个 test body 的 stdout (Console.WriteLine) 并 re-emit 到 **stderr**，
> 所以 machine formatter (json / junit / tap) 的 report 永远独占 stdout，
> 不被 test 自身输出污染。`z42-test-runner suite.zbc --format junit >
> report.xml` 总是产出合法 XML。test 输出 (含 benchmark 的 `bench[...]`
> 行) 在 stderr 仍可见。in-process 与 subprocess 模式行为一致 (subprocess
> 本就 pipe 子进程 stdout)。

### 默认 format 选择

- TTY 上 stdout → `pretty`（人类可读，带颜色）
- 非 TTY（管道、重定向、CI）→ `tap`（机器消费默认）
- 显式 `--format X` 强制覆盖

### Pretty

R3a 保留原 R3 minimal 输出语义，仅在收集所有结果之后再渲染。和 A2/A3 阶段视觉等价。

### TAP 13 ([testanything.org](https://testanything.org/tap-version-13-specification.html))

```
TAP version 13
1..8
ok 1 - Z42TestDogfood.test_assert_equal_pass
ok 2 - Z42TestDogfood.test_skip # SKIP platform=ios
not ok 3 - Z42TestDogfood.test_fail
  ---
  message: 'expected `Foo`, got `Bar`'
  ...
```

YAML diagnostic 块仅在 `not ok` 后输出（`ok` 不带 reason）；skip 用 `# SKIP <reason>` 指令。

### JSON

自定义 schema，可扩字段（保持向后兼容）：

```json
{
  "tool": "z42-test-runner",
  "version": "0.1.0",
  "module": "z42.test_dogfood.zbc",
  "summary": {
    "total": 8, "passed": 8, "failed": 0, "skipped": 0,
    "duration_ms": 48
  },
  "tests": [
    { "name": "Z42TestDogfood.test_assert_equal_pass",
      "status": "passed", "duration_ms": 6 },
    { "name": "Z42TestDogfood.test_skip",
      "status": "skipped", "duration_ms": 0,
      "reason": "platform=ios" },
    { "name": "Z42TestDogfood.test_fail",
      "status": "failed", "duration_ms": 7,
      "reason": "expected `Foo`, got `Bar`" }
  ]
}
```

`reason` 字段对 `passed` 测试不输出（serde `skip_serializing_if`）。后续 R3b 加 `setup_duration_ms` / `teardown_duration_ms` 时不破坏现有消费者。

### JUnit XML（add-junit-xml-formatter, 2026-05-31）

`--format junit` 输出事实标准 JUnit XML，被 Jenkins (`junit` step)、
GitLab CI (`artifacts:reports:junit`)、CircleCI、GitHub test-reporter 原生
ingest（失败高亮 / 历史趋势 / flaky 检测）。一次 `.zbc` run = 一个 module =
一个 `<testsuite>`，包在 `<testsuites>` root 里：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="3" failures="1" skipped="1" time="0.012">
  <testsuite name="MyMod" tests="3" failures="1" skipped="1" time="0.012">
    <testcase name="MyMod.test_pass" classname="MyMod" time="0.006"/>
    <testcase name="MyMod.test_skip" classname="MyMod" time="0.000">
      <skipped message="skipped on ios: WebGL bug"/>
    </testcase>
    <testcase name="MyMod.test_fail" classname="MyMod" time="0.000">
      <failure message="values not equal (expected 3, actual 2)">values not equal (expected 3, actual 2)
  at MyMod.test_fail (my_test.z42:42)
  at Std.Test.Assert.Equal (Assert.z42:38)</failure>
    </testcase>
  </testsuite>
</testsuites>
```

要点：
- `classname` = module 名（CI 用它做 grouping）
- `time` 秒数 3 位小数（duration_ms / 1000）
- failure `message` 属性 = reason 首行；body = 完整 reason + stack_trace（若有），全部 XML-escaped
- skipped 的 reason 进 `message` 属性
- XML 转义 hand-rolled（attr: `& < > " '`；text: `& < >`），不引入 XML 库 — 与 tap.rs yaml_escape 同风格
- benchmark entry 作普通 testcase（JUnit 无 benchmark 概念）；其 `bench_stats` 仍在 `--format json` 里

CI 集成示例：
```yaml
# GitLab CI
test:
  script: z42-test-runner suite.zbc --format junit > report.xml
  artifacts:
    reports:
      junit: report.xml
```

### --filter

substring match 而非 regex；不引入 `regex` 依赖。`test.method_name.contains(filter)` 为 true 即保留。如未来需 regex，独立 spec 升级。

### --list / --dry-run（add-runner-list-and-dry-run-flags, 2026-05-31）

- `--list` — 打印发现到的 test 名（每行一个），exit 0；不执行 body。
  与 `--filter` 组合可用。**CI sharding 典型用法**：
  ```bash
  z42-test-runner suite.zbc --list \
    | awk "NR % $N == ($JOB % $N)" \
    | xargs -I{} z42-test-runner suite.zbc --filter {}
  ```
- `--dry-run` — 走完 discovery + filter + skip evaluation，但不调 test body。
  通过 test → `Passed { duration_ms: 0 }`；`[Skip(...)]` 仍正确报 Skipped
  (skip_eval 跑)。验证 filter / platform / feature gating 不付执行成本。
- 两者同时设置 → `--list` 胜出（短路更早）。

### 退出码

- `0` — 全部通过 / 仅 skipped
- `1` — 任一 failed
- `2` — runner 内部错误（路径解析、I/O 等）
- `3` — 0 tests discovered（含被 filter 排空）

---

## 增量测试 `./xtask test changed`（R3c，2026-04-30）

`./xtask test changed` 把 `git diff` 的变更文件映射到受影响测试集合，跳过无关测试加快本地反馈。

### Base ref 解析

按优先级：

1. 环境变量 `Z42_TEST_CHANGED_BASE`（CI 友好，例如 `origin/main`）
2. 命令行第一参数（`./xtask test changed main`）
3. 默认 `HEAD`（工作区 + staged 修改）

untracked 文件（`git ls-files --others --exclude-standard`）也纳入变更集。

### 文件 → 测试映射

| 变更路径 | 触发命令 |
|---------|---------|
| `src/libraries/<lib>/src/**` | `./xtask test lib <lib>` + `./xtask test e2e` |
| `src/libraries/<lib>/tests/**` | `./xtask test lib <lib>` |
| `src/libraries/<lib>/<lib>.toml` | `./xtask test lib <lib>` |
| `src/runtime/src/**`、`src/runtime/Cargo.toml` | `cargo test runtime` + `./xtask test e2e` |
| `src/runtime/tests/**` | `cargo test --manifest-path src/runtime/Cargo.toml` |
| `src/tests/cross-zpkg/**` | `./xtask test e2e --dir cross-zpkg` |
| `src/tests/**`（其他） | `./xtask test e2e` |
| `src/compiler/**` | `./xtask test compiler` + `./xtask test e2e` |
| `src/toolchain/**` | `cargo test test-runner` + `./xtask test lib` |
| `src/toolchain/xtask` 的 vm / regen 命令 | `./xtask test e2e` |
| `src/toolchain/xtask` 的 lib 命令 | `./xtask test lib` |
| `src/toolchain/xtask` 的 cross-zpkg 命令 | `./xtask test e2e --dir cross-zpkg` |
| `scripts/xtask*.z42`、`*.workspace.toml`、`src/runtime/build.rs` | 全套 `./xtask test` |
| `*.md`、`docs/**`、`.claude/**`、`README*` | 不触发 |
| 其他 `src/**` 或未识别的根级文件 | 全套 `./xtask test`（防御性） |

去重后按"先编译后 VM 后 stdlib 后 cross"的隐式顺序串行执行；任一命令失败即停（透传退出码）。

### 限制（R3c 范围）

- 目录级粗粒度映射；不读 IR / 类级反向依赖图（需独立 spec）
- 不缓存上次结果（每次重新跑选中的命令）
- 单 base ref；不支持区间或多 ref
- 不监听文件变更（无 watch 模式）

### 用例

```bash
# 改了 z42.math 一个文件 → 只跑 z42.math + VM goldens
./xtask test changed

# PR 检查：只跑 main 与当前 branch 之间的差异影响范围
Z42_TEST_CHANGED_BASE=origin/main ./xtask test changed

# 看计划不执行（pre-commit hook 友好）
./xtask test changed --dry-run
```
- ⏸️ TypeArg 升级为 TypeExpr（当前 `string?` 足够）
- ⏸️ user-defined attributes（z42 当前白名单：z42.test.* + Native 两个 family）

### 错误码

- **E0913** `ShouldThrowTypeInvalid`：3 种触发（缺 type arg / 类型不存在 / 不继承 Exception）+ 1 种"非 ShouldThrow attribute 上有 type arg"
- **E0914** `SkipReasonMissing`（沿用）：扩展为 `[Skip] / [Ignore] / [ShouldThrow]` 三者任一缺 [Test]/[Benchmark] 都报错

---

## TestIO（R2 完整版，2026-05-05）

`Std.Test.TestIO` 三个静态方法，让测试代码捕获被测代码的 console 输出。lambda + delegate 已落地后兑现。

```z42
public static class TestIO {
    public static string captureStdout(Action body);
    public static string captureStderr(Action body);
    public static CaptureResult captureBoth(Action body);
}

public class CaptureResult {
    public string Stdout { get; }
    public string Stderr { get; }
}
```

### 实现要点

- **Stack 语义**：`__test_io_install_*_sink` push 一个 `Vec<u8>`，`__test_io_take_*_buffer` pop。嵌套合法（内层 push/pop 不影响外层 buffer）。
- **异常透传**：捕获过程中 body 抛 → take_buffer 在 catch 块里调一次确保 sink pop，再 `throw e;` 重抛。每个 `captureStdout` 调用前后 sink stack 深度守恒。
- **stderr 先决条件**：z42.io 加 `Std.IO.ConsoleError` 类（`WriteLine` / `Write`，binding `__eprintln` / `__eprint`），让 z42 用户首次能写 stderr。`captureBoth` 同时 install 两个 sink；channel 之间不混合。

### 错误模式

> 写测试时记得：z42 lambda **快照捕获值类型**（[examples/closure_capture.z42](../../examples/closure_capture.z42)）。要把 capture 结果传出 lambda body 必须用引用类型（class/array），不能直接对外部 int / string 局部变量赋值。dogfood 用 `IntCell` / `StrCell` 等 wrapper class 演示这个模式。

---

## Bencher（R2 完整版，2026-05-05）

`Std.Test.Bencher` 给代码段做 wall-clock 测量。
2026-05-31 起 runner 已支持调度 `[Benchmark]` 方法（见
[add-benchmark-runner-dispatch](../../spec/archive/2026-05-31-add-benchmark-runner-dispatch/)）— 在 body 内构造 `var b = new Bencher(); b.iter(() => ...);` 即可。
两种写法都可用：`[Benchmark] void name() { ... }`（runner 显示为 `bench:name`）或继续在 `[Test]` 内构造（pre-spec 写法仍兼容）。

```z42
public class Bencher {
    public Bencher();                              // warmup=10 / samples=100
    public Bencher(int warmupIters, int sampleIters);
    public void iter(Action body);
    public long MinNs { get; }
    public long MaxNs { get; }
    public long MedianNs { get; }
    public long TotalNs { get; }
    public int  Samples { get; }
    public void printSummary(string label);
}

public static class BenchHelpers {
    public static object blackBox(object value);   // identity；JIT 端预留 hook
}
```

### Native helpers

- `__time_now_mono_ns` — `OnceLock<Instant>` epoch + `Instant::now().elapsed().as_nanos()`，单调性由 `std::time::Instant` 保证
- `__bench_black_box` — interp 端 `args[0].clone()`；future JIT 端可挂钩防止 dead-code elimination

### Stats in JSON output（capture-benchmark-stats-in-testresult, 2026-05-31）

Runner 全模式 (in-process / `--legacy-subprocess` / `--jobs N>1`) 把
`Bencher.printSummary(label)` 的输出 line 解析为 `TestResult.bench_stats`
结构化字段，CI 工具 (perf-regression dashboards / 基线 diff scripts) 无需
grep 即可直接消费：

```json
{
  "name": "Z42TestBenchDemo.bench_addition_demo",
  "status": "passed",
  "duration_ms": 60,
  "is_benchmark": true,
  "bench_stats": {
    "label": "addition_demo",
    "min_ns": 3875,
    "median_ns": 3958,
    "max_ns": 4666,
    "samples": 5,
    "total_ns": 0
  }
}
```

字段对应 `Bencher` 实例的 `MinNs / MedianNs / MaxNs / Samples`. `total_ns`
当前 reserved 0 (Bencher.printSummary 不打 total)；future Bencher format
upgrade 加 `total=Nns` 后 parser patch 自动捕获 (单元测试 `bench_stats_*`
会 catch 格式不匹配).

**Parser invariant**: 字段顺序 + `ns` 单位固定 (`min=Xns median=Yns
max=Zns samples=N`). 修改 `Bencher.printSummary` 输出必须同步更新
`exec::extract_bench_stats_from_stdout` (commit 同时 ship).

**In-process 路径** (`--jobs 1`, default) 由
[bench-stats-in-process-capture (2026-05-31)](../../spec/archive/2026-05-31-bench-stats-in-process-capture/)
落地：runner 在每个 `[Benchmark]` 调用前 push 一个 thread-local
stdout sink (`z42::corelib::io::push_stdout_sink`), 调 body, pop, 解析
+ re-emit 到 process stdout (用户 terminal 仍看到原始输出)。所以 in-process 与
subprocess 模式 `bench_stats` 行为一致。机制复用现有
`STDOUT_SINKS` 栈 (originally built for `TestIO.captureStdout`); nested
captures (user 在 benchmark body 内再调 `TestIO.captureStdout`) 仍按栈
语义正确隔离。

### Runner [Benchmark] 调度（add-benchmark-runner-dispatch, 2026-05-31）

#### 用法（usage）

```z42
[Benchmark]
void bench_addition() {
    var b = new Bencher();              // warmup=10, samples=100 (defaults)
    b.iter(() => 1 + 2 + 3);
    b.printSummary("addition");         // → bench[addition] min=… median=… max=… samples=100
}
```

Runner 执行后 pretty 输出形如:

```
  ✓ bench:bench_addition  (12ms)
```

JSON 输出 `is_benchmark: true` field, 便于消费者按 group 过滤. TAP 与 [Test] 同形态（不区分；用 name 前缀辨识）.

`[Benchmark]` 与 `[Skip(...)]` / `[Timeout(...)]` 等其它 attribute 自由组合（同 `[Test]`）.

#### Signature contract（两种形态）

**形态 1 — zero-arg（add-benchmark-runner-dispatch, 2026-05-31）**：上面的
`void f()`，作者在 body 内自构 Bencher。

**形态 2 — Bencher-arg（add-benchmark-bencher-arg-trampoline, 2026-05-31）**：

```z42
[Benchmark]
void bench_add(Bencher b) {     // runner 给你一个现成的 Bencher
    b.iter(() => 1 + 2 + 3);    // 只管测量，无 boilerplate
}
```

编译器在 TypeCheck 前把它 desugar 成形态 1：

```z42
void bench_add$impl(Bencher b) { /* 原 body */ }   // 降级 helper，剥掉 attribute
[Benchmark] void bench_add() {                      // 合成 wrapper
    var b = new Bencher();
    bench_add$impl(b);
    b.printSummary("bench_add");                    // label = 原方法名
}
```

合成 wrapper 与手写 zero-arg benchmark **完全同形**，所以 validator /
runtime / runner 全部无改动即可处理它。`bench_stats.label` = 原方法名。
`$` 是非法标识符字符 → `$impl` 后缀 collision-proof。

`void f(Bencher b)` 用 `new Bencher()` 默认参数（warmup=10/samples=100）；
需自定义 warmup/samples 用形态 1 的 `new Bencher(W, S)`。

**仍报 E0912 的签名**（desugar 不触发 → validator 兜底）：多参数、单个
非-Bencher 参数（`void f(int x)`）、非 void 返回、泛型。**class-method
benchmark 暂不支持**（top-level only for v1；class-method Bencher-arg 仍
E0912，同 pre-spec）。

#### 设计思路（design rationale）

完整决策见
[design.md](../../spec/archive/2026-05-31-add-benchmark-runner-dispatch/design.md)。

| 维度 | 选择 | 拒绝的备选 + 理由 |
|------|------|--------------------|
| Signature shape | 支持 `void f()` + `void f(Bencher b)` 两形态 | 初版只 zero-arg（runner 缺 Bencher 构造设施）；Bencher-arg 经 add-benchmark-bencher-arg-trampoline 用 AST-desugar 补回 |
| Execution path | 与 [Test] 同路径 (in-process / subprocess / parallel 全部复用) | 单独路径会重复 Skip/Timeout/Setup-Teardown 逻辑; 共享路径让 [Benchmark] 自动继承所有现有特性 |
| Output 区分 | pretty `bench:` 前缀 + JSON `is_benchmark: true` field | 新 TestStatus 变体 (e.g. `Benchmarked`) 会破坏所有现有 status-grouping 消费者; 加 flag 是 backward-compatible |
| 公开 API 直接 break | 不引入 versioned alias | 零现存用户; 引入 alias 是不必要复杂性 |

#### Bencher-arg 实现：为什么 AST-desugar（add-benchmark-bencher-arg-trampoline, 2026-05-31）

完整决策见 [design.md](../../spec/archive/2026-05-31-add-benchmark-bencher-arg-trampoline/design.md)。

| 维度 | 选择 | 拒绝的备选 + 理由 |
|------|------|--------------------|
| 实现层 | AST-level desugar（pre-TypeCheck，`BenchmarkDesugar.Run`） | (a) runtime ObjNew API：把 Bencher 构造 + 字段读暴露给 Rust runner，耦合 interp 内部、要在 interp loop 外复刻 ctor-chain；(b) compiler IR-synthesis：IrGen 直接 emit trampoline IR，需 codegen 期跨包解析 Bencher ctor + printSummary，易错 |
| 为何 AST 最干净 | 合成的 `new Bencher()` / `printSummary` 走**正常**管线解析 — 用户的 `Bencher b` 参数本就证明它们在 scope；validator / runtime / runner **零改动**（desugar 只产出它们已能处理的 zero-arg 形态）| — |
| 注入点 | 单 chokepoint `PipelineCore.CheckAndGenerate` + `CheckOnly`（single-file + package 两路都经此） | 多处散注入易漏 |
| 命名 | wrapper 保留原名（用户可见 clean），body 移到 `$impl` | `$` 非法标识符 → collision-proof；wrapper 同名让 TIDX/JSON/pretty 显示干净的原方法名 |
| validator | **不改** | desugar 在 validate 前把 Bencher-arg 转 zero-arg，validator 永远只见 zero-arg；这把改动半径压到一个新文件 + 两行 pipeline 插入 |

---

## R2 实施期间发现的 z42 限制（2026-05-05，及后续修复）

R2 完整版实施时碰到的语言/反射 bug，多数同会话内已修复：

| 路径 | 现象 | 状态 |
|---|---|---|
| `e is X` cross-module（X 是导入类的短名）| 当 `e` 静态类型 = Exception、运行时 = Std.TestFailure → IsInstance 返回 false | ✅ 修：FunctionEmitterExprs `BoundIsPattern` 与 `BinaryOp.Is/As` 改用 `QualifyClassName`（commit 7858f30）|
| `Object.GetType()` on Exception 子类 | VCall 找不到方法（vtable 跨多层继承时未传递）| ✅ 修：exec_instr.rs VCall 加 lazy hierarchy walk fallback（同上 commit）|
| `throw;` bare rethrow | parser 无此语法 | ✅ 修：StmtParser 接受 `throw;`，TypeChecker 维护 catch-var 栈，desugar 到 `throw <currentCatchVar>;`（本批次）|
| `: this(args)` ctor delegation | parser 无此语法 | ✅ 修：TopLevelParser 接受 `:this(...)`，AST `FunctionDecl.ThisCtorArgs`，FunctionEmitter 委托时 emit 链接 ctor call + skip base + skip field-init（本批次）|
| Lambda 值类型快照捕获 | 设计选择，非 bug | 📋 保留：详见 [closure_capture.z42](../../examples/closure_capture.z42)。需要 mutable 状态用 wrapper class |
| Generic-E `is` (`is E` where E is type-param) | IR-side IsInstance 接受编译期硬编码 class_name | ⏸️ 未修：等需求驱动 |
| Generic-extern T inference (in-CU) | extern 函数 `T f<T>(T x)` 在同 CU 内调用 `f(42)` 无法推断 T | ✅ 修：SymbolCollector.Classes 在收 method 签名时激活 method.TypeParams（本批次）；TypeChecker.Calls 静态方法路径加 SubstituteGenericParams + SubstituteGenericReturn |
| Generic-extern T inference (cross-zpkg) | 跨 zpkg 调用还差 TSIG `ExportedMethodDef` 加 method-level TypeParams 字段 | ⏸️ 未修：BenchHelpers.blackBox(object) 临时形态；独立 spec 处理 TSIG bump |
| Method-level explicit generic call `Foo.bar<int>(42)` | parser 与 `<` 比较运算符冲突 | ⏸️ 未修：依赖参数推断（in-CU 已可用） |

`Assert.Throws(string typeName, Action)` 已恢复（依赖 IsInstance + GetType 修复），同时保留 `Assert.ThrowsAny(Action)` 处理"不在乎具体类型"场景。

---

## Benchmark 与 Test 分离原则

z42 把 benchmark 与 correctness test **在职责与驱动上分离**：两者由不同的命令驱动、
用不同的判定语义、走不同的 CI 门禁。它们的**源码位置**则同处 `src/tests/` 之下 ——
`src/tests/perf/` 是 benchmark 子树，与 correctness 类别平级但被发现逻辑显式排除。

> **2026-09-05 布局变更（move-bench-into-tests）**：顶层 `bench/` 目录已删除，内容并入
> `src/tests/perf/`。**这不是对分离原则的推翻，只是取消了"再开一个顶层目录"这个物理选择** ——
> 动机是不额外维护一个顶层目录。下面「架构红线」一节记录了原决定与改动理由。

### 当前 bench 布局

```
src/tests/perf/                          # benchmark 子树（xtask bench 独占驱动）
├── baseline-schema.json                 # 结果文档的 JSON Schema (Draft 2020-12, v2)
├── scenarios/                           # z42 端到端场景 (.z42)，01..11
│   ├── 01_fibonacci.z42 … 11_type_test_chain.z42
│   └──                                  # 首行 `// tier: gate|full` 决定是否进 PR 门禁
├── probe/capabilities.z42               # 被测 VM 能力探针 → 结果的 profile 字段
└── testdata/*.json                      # 判红 fixture（4 个），bench-pr.yml 每次跑断言退出码

src/libraries/<lib>/bench/*_bench.z42    # micro：[Benchmark] + Std.Test.Bencher，z42b 派发
                                         # （`.bench.` infix 是 zpkg 命名硬约束，与本节无关）
src/runtime/benches/                     # Rust criterion (cargo 框架强约定位置)
├── README.md  smoke_bench.rs  gc_cycle_bench.rs

artifacts/bench/                         # 结果输出（artifacts/ 整体 gitignored）

./xtask bench                     # hyperfine 调度 11 个场景 → artifacts/bench/e2e.json
./xtask bench --tier gate         # 只跑 PR 门禁那 6 条（CI 用的就是这条）
./xtask bench --mode both         # 每场景各测 interp 与 jit（各一条 profile 结果）
./xtask bench --ab …              # 同-runner A/B 回归门禁 → artifacts/bench/ab.json
./xtask bench --diff --baseline P # 与一份历史结果比对（阈值见 book 判红语义页）
```

### 为什么职责上分离（与 correctness 类别不同处）

| 维度 | `src/tests/<cat>/` (correctness) | `src/tests/perf/` (perf) |
|------|---------------------------|-----------------|
| 驱动 | `xtask test e2e`（golden：跑 z42vm、diff stdout） | `xtask bench`（hyperfine 测时） |
| 发现 | dir-mode `<cat>/<name>/source.z42` + flat `<cat>/*.z42` | 显式排除，见下 |
| 编译 profile | Debug 即可 | **必须 Release** |
| 输出 | 通过 / 失败 + 诊断 | metrics JSON + 置信区间 |
| 噪声容忍 | 必须确定性 | 可重跑 / warmup / 多 sample |
| CI 门禁 | hard fail | 见 book 判红语义页（e2e / micro 硬门禁，均走「可疑即复测」；criterion 信息性）|

**发现逻辑的显式排除**（两个枚举器各一处，都以 `perf` 为名单项）：

- `_isNonRunnableCat`（`scripts/common/xtask_golden.z42`）—— golden 运行器 + embedded corpus 共用
- `_isNonRegenCat`（`scripts/build/xtask_test_assets.z42`）—— `xtask build test` 资产编译

perf 的文件都比 golden 深一层（`perf/scenarios/*.z42` 而非 `perf/*.z42`，且无 `source.z42`），
所以两个枚举器**今天**本来就抓不到它。名单项是为了**声明**这个排除，而不是依赖布局巧合 ——
否则日后有人在 `perf/` 顶层放一个 `.z42`，就会静默把一条 benchmark 登记成 golden 测试。

`xtask test changed` 同理：`src/tests/perf/` 映射到 `xtask bench --quick`（前 2 个场景、
runs=3，< 60s，验"场景还能编译运行"），而不是把整个 e2e 套件扫一遍。

### 主流语言调研（2026-04-30）

| 语言 | tests 位置 | bench 位置 | 模式 |
|------|----------|----------|------|
| **Rust** | `tests/` | `benches/` 平行 | 分离（cargo 内置） |
| **C++** | `tests/` | `benchmarks/` | 分离（Google Benchmark 约定） |
| **.NET** | `*.Tests.csproj` | `*.Benchmarks.csproj` | 分离（BDN 必须独立 project） |
| **Java** | `src/test/java/` | `src/jmh/java/` | 分离（JMH 独立 sourceSet） |
| **Haskell** | `test/` | `bench/` | 分离（Cabal `benchmark` stanza） |
| **Python (asv)** | `tests/` | `benchmarks/` | 分离（NumPy/SciPy 用） |
| **Python (pytest-benchmark)** | `tests/test_*.py` | 同文件 fixture | **统一**（少数派） |
| **Go** | `*_test.go` | `*_test.go` 内 `BenchmarkX` | **统一**（极简哲学，唯一） |

这些语言的"分离"首先是**驱动与判定的分离**（独立 runner、独立 baseline/diff 流），
目录平行只是其外在形式；z42 保留前者，放弃后者的顶层目录形式。

### 架构红线

- `src/tests/perf/` 由 `xtask bench` 独占驱动，**不**进 golden 发现（两处名单项，见上）
- `src/runtime/benches/` **不**移动（cargo 框架约定位置）
- correctness 类别 `src/tests/<cat>/` 里**不**放性能场景；性能场景只落 `src/tests/perf/`
- 反向亦然：`src/tests/perf/` 下**不**放 correctness golden（无 `expected_output.txt` 语义）

> **已推翻的红线（2026-09-05）**：原文两条为「`bench/` 顶层目录**不**移到 `src/tests/`」与
> 「`src/tests/` 不放性能场景（即不出现 `perf-*` 子目录）」。改动理由：顶层多一个只有
> `xtask bench` 消费的目录，维护成本（.gitignore 条目、CI paths、三处文档目录树）大于
> 它换来的可见性；而分离原则真正要防的"benchmark 被当成 correctness test 跑"，由发现
> 逻辑里的显式排除保证，比靠目录位置隔离更直接、也可被测试覆盖。同期一并删除的还有
> `bench/README.md`（内容并入 book 判红语义页）与 `bench/scripts/compare-modes.sh`
> （其能力已被 `xtask bench --mode both` 覆盖）。

---

## Attribute 系统（R1 已落地 6 个）

每个被 `z42.test.*` attribute 标注的函数会进入 zbc 的 TIDX section。语义校验在 R4。

| Attribute | 形式 | 语义（R3 runner 行为） |
|-----------|------|---------------------|
| `[Test]` | 无参 | 标记普通测试方法 |
| `[Benchmark]` | 无参 | 标记基准方法（runner 用不同调度） |
| `[Setup]` | 无参 | 每个 `[Test]` 前调用 |
| `[Teardown]` | 无参 | 每个 `[Test]` 后调用（即使失败） |
| `[Ignore]` | 无参 | 永久忽略（runner 不列入统计） |
| `[Skip(reason: "...", platform: "...", feature: "...")]` | 命名参数 | 跳过：reason 必填；platform 限定平台时跳过；feature 缺失时跳过 |

`[Skip]` 三个命名参数都可选（除 reason 外），可任意组合：

```z42
[Test]
[Skip(reason: "blocked by issue #123")]                                  // 总是跳
void test_known_broken() { }

[Test]
[Skip(platform: "ios", reason: "JIT not supported on iOS")]              // iOS 上跳
void test_jit_only() { }

[Test]
[Skip(feature: "multithreading", reason: "single-threaded build")]       // 缺特性时跳
void test_concurrent() { }

[Test]
[Skip(platform: "wasm", feature: "filesystem", reason: "wasm sandbox")]  // 多重条件
void test_fs_io() { }
```

R4 计划新增的 attribute（**目前 parser 不识别**）：

- `[ShouldThrow<E>]` — 期望函数抛 `E` 类型异常（需先实现 attribute 泛型语法）
- `[TestCase(args)]` — 参数化测试，可重复多次（需先实现 typed args 语法）

### `[Timeout(milliseconds: N)]` — Per-test wallclock budget (2026-05-30)

由 [add-test-timeout-attribute](../../spec/changes/add-test-timeout-attribute/) 引入，
首个接受**整数字面量** named-arg 的 test attribute：

```z42
[Test]
[Timeout(milliseconds: 60_000)]
void test_secp256k1_roundtrip() { ... }
```

要点：
- 必须与 `[Test]` 或 `[Benchmark]` 同时出现；不可重复
- `milliseconds:` 值必须 `> 0` 且 `≤ i32::MaxValue`；否则 **E0917**
- runner 把请求 budget 与 `TIMEOUT_HARD_CEILING_SECS = 2 × DEFAULT_TIMEOUT_SECS = 600s` 比较，
  超出时 clamp 到 ceiling 并打一行 `note:` 警告（保护 hang detector，
  防止 `60_000_000` 这种 typo 完全禁用超时机制）
- 无 `[Timeout]` 时 runner 使用 `DEFAULT_TIMEOUT_SECS = 300`，origin 标为
  `"runner default"`；超时失败 reason 字段会显示 budget 来源便于分辨
- TIDX section 在 v=3 起每条 entry 追加 `timeout_ms: i32`（`0` = no override）

> **AttributeArg discriminator**：为承载整数 named-arg，parser 把
> `TestAttribute.NamedArgs` 从 `Dictionary<string, string>` 升级为
> `Dictionary<string, AttributeArg>`，其中 `AttributeArg` 是
> `AttributeArgString | AttributeArgInt` sealed-record 判别联合。未来
> 加 `AttributeArgIdent` / `AttributeArgFloat` 形态时只需新增 record +
> parser 分支 + 消费侧 pattern。

### `[Skip(platform:)] / [Skip(feature:)]` — Conditional skip semantics (2026-05-30)

由 [add-test-skip-platform-feature-eval](../../spec/archive/2026-05-30-add-test-skip-platform-feature-eval/) 引入。R1.C 起 compiler 就把
`platform: / feature: / reason:` 三段写入 TIDX，但**直到本 spec 落地之前 runner 都
无条件跳过**任何带 `[Skip]` flag 的 test —— 把 "iOS-only 测试" 在所有平台都跳掉。
本 spec 把"按条件跳过"的语义补齐。

#### 用法（usage）

```z42
// 仅在 iOS host 上跳过；linux / macos / windows 上正常跑
[Test]
[Skip(platform: "ios", reason: "WebGL backend unimplemented on iOS")]
void test_render_pipeline() { ... }

// 在不支持 multithreading 的 host 上跳过（wasm 单线程 → 跳；native → 跑）
[Test]
[Skip(feature: "multithreading", reason: "needs worker pool")]
void test_concurrent_index() { ... }

// 复合：在 iOS **或** filesystem 不可用时跳过（OR 语义）
[Test]
[Skip(platform: "ios", feature: "filesystem", reason: "ios sandbox + browser fallback")]
void test_load_from_disk() { ... }

// 无条件跳过（保留 R1.A 旧用法）
[Test]
[Skip(reason: "tracker #4711 — fix in next sprint")]
void test_known_broken() { ... }
```

#### 支持的 platform 值（精确字符串比较，case-sensitive）

`"linux" | "macos" | "windows" | "android" | "ios" | "wasm" | "freebsd"`

值来自 Rust `std::env::consts::OS`，与 stdlib `Std.Platform.OS()` 同源。
不在列表中的字符串永远不匹配（写 `[Skip(platform: "atari")]` 不会在任何
host 上跳过 —— 用于占位 / future-OS 测试）。

#### 支持的 feature 值（v1 minimal 注册表）

| Feature 名 | Available 时机 | 说明 |
|-----------|----------------|------|
| `interp`  | 始终 true | interp 解释器始终编译进 z42vm |
| `jit`     | 始终 true | JIT 也编译进；执行模式选择是 per-method 而非 build-time |
| `multithreading` | 非 wasm 时 true | wasm 单线程 sandbox |
| `filesystem` | 非 wasm 时 true | wasm 沙箱无 host fs |

**未知 feature 名 → deny-by-default 跳过** + stderr `note: unknown feature
"X" — treating as unavailable`（一次跑里同名只 warn 一次）。把 typo
（`"multi-threading"` → 应为 `"multithreading"`）当成"我们这环境也不支持"
处理，比 fail-open 静默吞 typo 安全。

#### CLI / env 覆盖（验证用）

```bash
# Linux host 上验证 iOS 跳过路径
z42-test-runner suite.zbc --platform ios

# env var 等价（CLI 优先于 env）
Z42_TEST_PLATFORM=ios z42-test-runner suite.zbc
```

> feature 没有对应 CLI override；编译期 cfg 决定。需要测试"feature
> unavailable" 路径时，写一个 unknown feature 名（自动 deny）即可。

#### 设计思路（design rationale）

完整决策记录见
[`design.md`](../../spec/archive/2026-05-30-add-test-skip-platform-feature-eval/design.md)。
关键选择简述：

| 维度 | 选择 | 拒绝的备选 + 理由 |
|------|------|--------------------|
| Platform 来源 | runner Rust 端直读 `std::env::consts::OS` | 不通过 z42 bootstrap 调 `Std.Platform.OS()` — 引入额外 VM call 依赖且 stdlib 未链接时挂；两者本就源于同一 Rust const，无信息差 |
| Compound (`platform: + feature:`) | OR — 任一成立就跳 | AND 会让 "在 iOS 但 JIT 可用" 环境意外跑过去；OR 对齐 pytest `@skipif(c1 or c2)` 直觉 |
| Unknown feature | Deny-by-default + warn | Fail-open 静默吞 typo（`multi-threading` → 该跳没跳挂掉）；硬 error 让测试代码 typo 阻塞整个 run，破坏 "runner 是工具" 期望 |
| Feature 初始集 | 4 个 (`interp/jit/multithreading/filesystem`) | 与 `examples/test_demo.z42` 已用案例对齐 + 常见诉求；其他（async / gc-precise / network）按需扩 |
| Reason 字符串 | 触发条件 + 用户 reason 拼接（`"skipped on ios: WebGL bug"`） | 仅显示 user reason 会让排查者必须查源码反推"为什么这次跳了"；触发条件直接显示是 debugging 体验关键 |
| `SkipEnv` 通过参数传 | 显式参数，不进 thread-local / global | 单元测试可自由构造任意 env 做矩阵参数化；clarity over magic |

#### 实施 (implementation)

- 核心逻辑：`Std.Test.Runner._skipApplies(entry)`（[`src/libraries/z42.test/src/Runner.z42`](../../../src/libraries/z42.test/src/Runner.z42)）
  —— 无条件 skip / `[Skip(platform:)]` 按 host OS / `[Skip(feature:)]` 保守跳过（原 Rust `skip_eval.rs` 已删）
  （`runner.rs` in-process / `exec.rs` subprocess / `parallel.rs` parallel-subprocess）
  共享同一决策权威
- 覆盖：z42.test `tests/skip_platform_demo.z42` 端到端验证平台/特性 skip 判定
  （原 Rust `skip_eval_tests.rs` 18 用例矩阵已随 runner 删除；判定逻辑现在
  `Std.Test.Runner._skipApplies`）
- E2E demo：[`src/libraries/z42.test/tests/skip_platform_demo.z42`](../../../src/libraries/z42.test/tests/skip_platform_demo.z42)
  9 用例（7 platform + 1 永不匹配 + 1 unknown feature）— 由 stdlib test
  wave 跑过验证 end-to-end 行为

### Failure source location in runner output (2026-05-30)

由 [surface-test-failure-source-location](../../spec/archive/2026-05-30-surface-test-failure-source-location/)
引入。Runtime 自 2026-05-10 起就在每次 throw 时往 `Std.Exception.StackTrace`
字段填充 `(file:line[:col])` 的多帧栈（见 `src/runtime/src/exception/mod.rs:186-224
populate_stack_trace`），但 runner `format_value` 之前**只读 Message 字段**，
所有 stack 信息直接丢弃。用户看到的失败仅有 `"TestFailure: values not equal"`，
没法定位到出错的 Assert 调用在哪一行。本 spec 把已有的 stack 信息接通到所有
三种 formatter 的输出里。

#### 用法（usage）

测试代码无需任何改动。失败时 runner 自动展示：

> Note: 2026-05-31 起 `(file:line)` 是默认形态。Pre-spec 因
> [fix-line-entry-file-population](../../spec/archive/2026-05-31-fix-line-entry-file-population/)
> 未落地时 LineEntry.file 通常为 null，frame 退化为 `(line N, col M)`
> 无 file，IDE jump-to-source 失效。fix 后所有 frame 都自带源文件路径。

**Pretty (TTY)**:

```
  ✗ MyTests.test_arithmetic  (my_test.z42:42)
      TestFailure: values not equal (expected 3, actual 2)
      stack:
        at MyTests.test_arithmetic (my_test.z42:42)
        at Std.Test.Assert.Equal (Assert.z42:38)
        at Std.Test.AssertCore.checkEqual (AssertCore.z42:17)
```

**TAP 13**:

```
not ok 3 - MyTests.test_arithmetic
  ---
  message: 'TestFailure: values not equal (expected 3, actual 2)'
  location: 'my_test.z42:42'
  stack: |
      at MyTests.test_arithmetic (my_test.z42:42)
      at Std.Test.Assert.Equal (Assert.z42:38)
      at Std.Test.AssertCore.checkEqual (AssertCore.z42:17)
  ...
```

**JSON**:

```json
{
  "name": "MyTests.test_arithmetic",
  "status": "failed",
  "duration_ms": 7,
  "reason": "TestFailure: values not equal (expected 3, actual 2)",
  "failure_location": "my_test.z42:42",
  "stack_trace": "  at MyTests.test_arithmetic (my_test.z42:42)\n  at Std.Test.Assert.Equal (Assert.z42:38)\n  at Std.Test.AssertCore.checkEqual (AssertCore.z42:17)"
}
```

`failure_location` 适合 IDE / CI 工具直接消费，做 jump-to-source 快捷
跳转。`reason` 字段保持向前兼容（pre-2026-05-30 CI 脚本继续工作）。

> **JIT path 已覆盖**（更正 2026-05-31）：JIT 模式**已**在 throw 处钩入
> `populate_stack_trace` —— 见 `src/runtime/src/jit/helpers/control.rs::jit_throw`
> (2026-05-10 jit-stack-trace + span-column-propagate)。JIT 与 interp
> 共用同一 `VmContext.call_stack`（unify-frame-chain），codegen 在每个
> call site / throw site 预先 stamp `(line, col)` 常量。golden test
> `src/tests/exceptions/stack_trace_field.z42`（断言 trace 含
> `Demo.Inner/Outer/Main`）在 `./xtask test e2e` 的 **jit pass** 下通过，证明
> JIT-executed throw 的 StackTrace 正确填充。**早先版本此处误标"未覆盖"
> —— 实为已实现，本次更正。**
>
> **Subprocess (`--jobs N>1` 或 `--legacy-subprocess`) 现支持** stack —
> 2026-05-31 起父进程解析 z42vm stderr 里的
> `Error: uncaught exception:` 后续 `  at <Func> (<file>:<line>)` 行
> （`parse-subprocess-failure-location-from-stderr`），与 in-process
> 路径同等展示 `failure_location` + `stack_trace`。

#### 设计思路（design rationale）

完整决策记录见
[`design.md`](../../spec/archive/2026-05-30-surface-test-failure-source-location/design.md)。
关键选择：

| 维度 | 选择 | 拒绝的备选 + 理由 |
|------|------|--------------------|
| Reason vs 独立字段 | location / stack 独立成 `TestResult` 字段 | 拼进 `reason` 会破坏既有 CI 脚本的 grep 解析；独立字段让 JSON consumer 直接做 IDE jump-to-source 无需 regex |
| Framework-frame filter (primary 提取) | `Std.Test.*` prefix OR `.Assert.` substring → 跳过 | regex / 完整 trie 过 engineered；简单 startsWith / contains 覆盖 99% case，少量误判（`MyApp.AssertUtils` 误归 framework）可接受换取无依赖 + 易理解 |
| Full stack 不过滤 | 即使全是 framework 帧也完整保留 | Assert 内部 bug 的诊断仍要看完整栈；`primary_location` 给主路径，full stack 给 deep-debug |
| Pretty 默认展开 stack | 无 `--no-stack` flag | v1 红测试 = 用户主动想看 detail；噪声主要来自全绿 run（那里没 fail output） |
| JSON 字段名 `failure_location` | 而非 `location` | 区分 "throw site" 与未来可能加的 "test method declaration site"；前缀 `failure_` 自描述 |
| YAML literal block `\|` for stack | 而非单行 yaml_escape | 多行栈一行行 escape 拼成 `"l1 l2 l3"` 失去结构；literal block 是 TAP 13 + YAML 1.2 原生多行表达 |
| Stack 解析无 regex 依赖 | 手写 `splitn` / `strip_prefix` | 增加 regex crate dep 不值；producer 是 z42-internal，shape 稳定可控 |

#### 实施 (implementation)

- 核心逻辑：`Std.Test.Runner._runOne`（[`src/libraries/z42.test/src/Runner.z42`](../../../src/libraries/z42.test/src/Runner.z42)）catch 异常 → `ex.Message` / `ex.GetType().FullName`；栈轨迹由 z42vm 经 `ctx.pending_thrown` 原样透出（原 Rust `runner.rs` 已删）
- 数据通道：`Outcome::Failed { reason, location, stack_trace }` →
  `TestResult { reason, failure_location, stack_trace }` →
  pretty / tap / json formatter
- 覆盖：z42.test `tests/failure_location_demo.z42` 端到端验证失败位置 + 栈轨迹（原 Rust `runner_tests.rs` 已删）（empty
  input / all-framework / mixed / col-suffix-stripping / no-parens-locus /
  line-only fallback / unicode paths / malformed line skip）
- TAP / JSON formatter 测试：`format/tap.rs::tap_format_with_location_and_stack_includes_new_fields`
  与 `format/json.rs::json_serialization_round_trip` 覆盖输出 byte shape
- E2E demo：[`src/libraries/z42.test/tests/failure_location_demo.z42`](../../../src/libraries/z42.test/tests/failure_location_demo.z42)
  catch 一个 Assert.Equal 失败、断言其 StackTrace 字段非空 + 包含 Assert 帧
  + 包含 test 方法名 → 验证 runtime 仍在跑 + user-frame 捕获正确

### Std.Test.Assert API quick reference (2026-05-30, extended)

由 [extend-assert-numeric-and-collection-helpers](../../spec/archive/2026-05-31-extend-assert-numeric-and-collection-helpers/)
扩充。完整方法列表分组：

| 分组 | 方法 |
|------|------|
| Equality | `Equal(o, o)`, `NotEqual(o, o)` |
| Boolean | `True(b)`, `False(b)` |
| Null | `Null(o?)`, `NotNull(o?)` |
| String | `Contains(string, string)` |
| Numeric ordering | `Greater`, `Less`, `GreaterOrEqual`, `LessOrEqual` × `{long, double}` |
| Numeric range | `InRange(actual, min, max)` × `{long, double}` (inclusive bounds) |
| Array containment | `ArrayContains(o, o[])`, `ArrayDoesNotContain(o, o[])` |
| Array emptiness | `ArrayIsEmpty(o[])`, `ArrayIsNotEmpty(o[])` |
| Exception | `Throws(typeName, action)`, `ThrowsAny(action)`, `DoesNotThrow(action)` |
| Float approx | `EqualApprox(actual, expected, eps)` |
| Control | `Fail(msg)`, `Skip(reason)` |

#### 用法（usage）

```z42
using Std.Test;

[Test]
void test_port_in_range() {
    var port = ServerFactory.NewPort();
    Assert.Greater(port, 0);                  // strict ordering
    Assert.InRange(port, 1024, 65535);        // inclusive range
}

[Test]
void test_response_envelope() {
    object[] headers = response.GetHeaders();
    Assert.ArrayIsNotEmpty(headers);
    Assert.ArrayContains("Content-Type", headers);
    Assert.ArrayDoesNotContain("X-Internal-Trace", headers);
}

[Test]
void test_pi_approximation() {
    Assert.EqualApprox(MyMath.Pi(), 3.14159, 1.0e-4);  // tolerant
    Assert.Greater(MyMath.Pi(), 3.0);                   // strict
}
```

#### 设计思路（design rationale）

完整决策见 [design.md](../../spec/archive/2026-05-31-extend-assert-numeric-and-collection-helpers/design.md)。
关键选择：

| 维度 | 选择 | 拒绝的备选 + 理由 |
|------|------|--------------------|
| 命名风格 | `Greater` (短) vs xUnit `GreaterThan` | 与 stdlib 既有 `NotEqual` / `Contains` 短形一致；4 字符 × 多调用点 = 显著降噪 |
| 参数顺序 | `(actual, expected)` for ordering helpers | 与 `Equal(expected, actual)` *故意*不对称：equality 对称，ordering 有方向 — 顺序就是被断言的不等式（`Greater(port, 0)` 读为 "port 大于 0"）|
| `InRange` 边界 | 包含 (`min <= x <= max`) | xUnit 同；半开区间不直觉；想要 exclusive 用 Greater + Less 两条 |
| double overload | 不复用 `EqualApprox` 公差 | strict ordering vs tolerant comparison 是不同 assertion；混合两者会让 `Greater` 语义模糊 |
| NaN 处理 | 显式 `if (x != x)` guard 抛 TestFailure | IEEE-754 让 `NaN <= 0` 为 false，朴素 ordering 会让 `Greater(NaN, 0)` 静默通过；guard 保证显式失败。**注：z42 当前从 `0.0/0.0` 不产生真 NaN（疑似常量折叠），guard 暂无法从 z42 source 触发，但保留作为 defensive 代码** |
| 数组 helper 仅 `object[]` | 不引入 `List` / `Set` 重载 | z42 Phase 1 无泛型；`object[]` 通过 boxing 覆盖 6/6 观察用例；L2 generics 后扩展 |
| **`Array*` 命名前缀** | 不复用 `Contains` / `IsEmpty` 短名 | z42 DependencyIndex first-wins **不做跨包 overload resolution**（[common-pitfalls.md §1](../../../.claude/rules/common-pitfalls.md#1-资源加载顺序必须显式排序2026-05-17-强化)）。bare `Assert.Contains` 永远先命中 z42.core 的 `(string, string)` overload，z42.test 的 `(object, object[])` overload 永远不可达。前缀消除 collision。L2 加 generics 后可引入真正的 `Contains<T>(T, IList<T>)` 与 `Array*` 并存或 deprecate alias |
| 比对运算符 | `==` (z42 默认) | 与 List.Contains 等 stdlib 集合一致；不需要自定义 Equals dispatch |

#### 实施 (implementation)

- 核心扩展：[`src/libraries/z42.test/src/Assert.z42`](../../../src/libraries/z42.test/src/Assert.z42)
  10 numeric methods (5 family × 2 overload) + 4 array methods
- 单元测试：
  - [`src/libraries/z42.test/tests/assert_numeric_helpers.z42`](../../../src/libraries/z42.test/tests/assert_numeric_helpers.z42)
    22 cases (每方法 pass / fail / boundary 多个)
  - [`src/libraries/z42.test/tests/assert_collection_helpers.z42`](../../../src/libraries/z42.test/tests/assert_collection_helpers.z42)
    12 cases (int + string element 覆盖 + empty edge + regression for pre-spec string Contains)

#### Deferred — upstream gaps observed during this spec

（无）— 此前列的 `bench-bencher-arg-trampoline` 已由
[add-benchmark-bencher-arg-trampoline (2026-05-31)](../../spec/archive/2026-05-31-add-benchmark-bencher-arg-trampoline/)
落地（AST-desugar，见上文 Benchmark 章节）。其余测试框架延后项
（`[TestCase(args)]` 参数化、`TestFailure.Location` 编译期注入）受 L2
语言特性（泛型 / `[CallerLineNumber]` attribute infra）阻塞，登记在
`docs/roadmap.md` Deferred Backlog Index。

---

## TIDX 二进制格式（R1）

详见 [`zbc.md` 的 TIDX 段](../runtime/zbc.md#tidx-test-index可选spec-r1)。

要点：
- Section tag 4 字节 ASCII：`TIDX`
- 当前版本 `v=3`（add-test-timeout-attribute，2026-05-30；v=2 → v=3 追加 trailing `timeout_ms: i32`）
- 仅当模块含至少一个测试 attribute 时由 `ZbcWriter.BuildTidxSection` 写入；
  缺失 = 该 .zbc 无测试
- 字符串引用为 **1-based** 索引到 `module.string_pool`，`0` 表示无值
- C# 类型：[`Z42.IR.TestEntry`](../../src/compiler/z42.IR/TestEntry.cs)
- Rust 类型：[`z42_vm::metadata::TestEntry`](../../src/runtime/src/metadata/test_index.rs)
- 跨语言契约测试：[`src/runtime/tests/zbc_compat.rs::test_demo_tidx_round_trips`](../../src/runtime/tests/zbc_compat.rs)
- 演示文件：[`examples/test_demo.z42`](../../examples/test_demo.z42)

---

## R 系列实施进度（截至 2026-04-30）

| Phase | Spec | Status | Commit |
|-------|------|--------|--------|
| R1.A+B | [add-test-metadata-section](../../spec/archive/2026-04-30-add-test-metadata-section/) | ✅ TestEntry types + zbc TIDX v=1 plumbing | `ea54554` |
| R1.C.1 | 同上 | ✅ TIDX v=2 + skip_platform/feature fields | `bb2df98` |
| R1.C.2-5 | 同上 | ✅ parser 识别 6 attribute + IrGen + 跨语言契约 | `5180d21` |
| R1.D | 同上 | 🟡 docs（本文件 + ir.md 注 + error-codes 占位）+ archive |  |
| R2 | [extend-z42-test-library](../../spec/archive/2026-05-05-extend-z42-test-library/) | ✅ Assert API + TestIO + Setup/Teardown | — |
| R3 | [rewrite-z42-test-runner-compile-time](../../spec/archive/2026-05-12-rewrite-z42-test-runner-compile-time/) | ✅ z42-test-runner lib API | — |
| R4 | [compiler-validate-test-attributes](../../spec/archive/2026-04-30-compiler-validate-test-attributes/) | ✅ E0911/E0912/E0914/E0915 validation | — |
| R5 | [rewrite-goldens-with-test-mechanism](../../spec/archive/2026-04-30-rewrite-goldens-with-test-mechanism/) | ✅ (scope 缩窄, 部分 stdlib goldens migrated) | — |

---

## 编写新测试

### 编译器层（C# xUnit）

```bash
# 加测试到 src/compiler/z42.Tests/<Topic>Tests.cs
# 运行
dotnet test src/compiler/z42.Tests/z42.Tests.csproj
# 或
./xtask test compiler
```

### VM 端到端（z42 golden）

```bash
# 1. 选好类别：src/tests/<category>/<name>/source.z42 (按归属规则)
# 2. 写 src/tests/<category>/<name>/expected_output.txt（可选；空 = 用 Assert.* 自验）
# 3. ./xtask build test 编译 source.zbc
# 4. ./xtask test e2e 验证
```

### Stdlib 库本地（R3 runner 落地后）

```z42
// src/libraries/z42.text/tests/string_basics.z42
import z42.test.{Test, Assert};
import z42.text.StringBuilder;

[Test]
void test_append_concat() {
    let sb = StringBuilder();
    sb.append("a"); sb.append("b");
    Assert.eq(sb.build(), "ab");
}
```

R3 落地后通过 `./xtask test lib z42.text` 运行。

### 工程级集成（src/tests/）

```bash
mkdir -p src/tests/my-integration-test
cd src/tests/my-integration-test
# 选 1：Rust crate
cargo init --bin && # 加 Cargo.toml 到 src/runtime workspace.members
# 选 2：纯脚本
cat > run.sh <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
# ... your test logic
EOF
chmod +x run.sh
```

---

## 全绿（GREEN）标准

任何迭代进归档前，以下命令全过：

```bash
./xtask build      # dotnet + cargo 全部编译通过
./xtask test       # compiler + VM + cross-zpkg 全过
cargo test                # Rust 单测（含 metadata::test_index 12 个）
```

详见 [.claude/rules/workflow.md](../../.claude/rules/workflow.md) 阶段 8。
