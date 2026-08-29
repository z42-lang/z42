# Cross-Platform 测试架构

> **更新（unify-test-pipeline-z42b，2026-08-29）—— 两层执行模型 + bundle 缝**：本节是当前
> 权威的顶层框架，下文旧描述按此校准。
>
> **两层职责切分：**
> - **z42b = 单目标执行器**：给一个目标（项目 or bundle）+ 一个 rid，负责「编译→部署→运行」这
>   **一份**。host（无 rid）走 `builder_test` 进程内反射；on-device（`--rid`）走部署 + 嵌入 agent。
>   统一为 `z42b test [--rid]`。
> - **xtask = 语料/fleet 编排器**：repo 结构知识（发现 `src/tests/**` + `libraries/<lib>/tests/**`、
>   编译全量 → .zbc、分片、聚合、CI 门禁）留在 harness。z42b 眼里只有「一个项目」，散装 fixture 语料
>   天然多工程 → 不能塞进 z42b。
> - **缝 = bundle manifest** `{cases:[golden|unit …]}`：xtask 编完语料吐 bundle，`z42b test --rid`
>   消费（`agent._runBundleReport` 已支持：golden 隔离 VM + unit 共享 VM）。无需新协议。
>
> **on-device 载荷归属**：test-agent 住**按需下载的能力 workload** `workload/test/agent`（平台无关，
> `z42 workload install test`），非某平台、也不进 SDK 恒在核心。平台专属嵌入 host 壳仍住各
> `workload/<plat>/`——共享 agent 随 test workload 下载、平台 host 随平台 workload 下载，边界清晰。
> test workload 是 **payload-only 形状**（无 per-RID runtime pack、`host:["*"]`；install CLI 免改，
> 打包侧后续扩展）。
>
> **分阶段（尊重 wire-z42b-host-build 时序）**：
> ① 载荷归位 + 架构定稿（已落，xtask 仍编译+驱动嵌入运行）；
> ②a **host compile-then-test 首刀（已落，`add-z42b-compile-then-test`）**：`z42b test <z42.toml>`
> （或无 target 默认 `z42.toml`）经注入编译器现编工程到 dist 再反射跑 `[Test]`，host 侧对工程/产物
> 统一入口——消除调用方「先 build 再 test」两步；
> ②b z42b 接管平台部署运行 + test workload 打包发布 + **各平台 backend 委托 `z42b test --rid`、退 bespoke**
> （`xtask_test_platform.z42` 的 `IPlatformBackend` 收缩为薄壳）——待做；
> ③ z42b in-process 编译成熟后，「编译一个项目」也走 z42b（语料级编译仍留 xtask）。
> 详见 change `unify-test-pipeline-z42b` 的 design.md（D1–D6）+ `add-z42b-compile-then-test`。

> **更新（retire-test-runner，2026-06-30）**：本文多处把 runner 描述为 Rust **library**
> （`src/toolchain/test-runner/src/lib.rs`，各平台绑库不 fork）。该 Rust runner 已删除——
> host 上 runner 现为 z42b（`z42.builder.zpkg` 经 `z42vm`，见 [testing.md](testing.md)）。
> 平台侧（wasm/iOS/Android）on-device 测试承载形态待 wire-z42b-host-build 阶段重新设计；
> 下文的 runner-as-Rust-library 部分按此理解。
>
> 状态：Design Draft（2026-05-09）。落地分散到 [rewrite-z42-test-runner-compile-time](../../spec/archive/2026-05-12-rewrite-z42-test-runner-compile-time/)（lib API） + [add-platform-wasm](../../spec/archive/2026-05-12-add-platform-wasm/) / [add-platform-android](../../spec/archive/2026-05-12-add-platform-android/) / [add-platform-ios](../../spec/archive/2026-05-12-add-platform-ios/) 各自的 Testing 子段。
>
> 本文是 [cross-platform.md](../runtime/cross-platform.md)（VM build 矩阵）与 [testing.md](testing.md)（测试框架架构）的桥接：**同一份测试集如何在 host / wasm / iOS / Android 一致地跑，并把失败精确报告回 CI**。
>
> 本文的 runner 假设是 **Rust library**。runner 自身向 z42 的迁移路径（自举对齐）见 [test-runner-bootstrap.md](test-runner-bootstrap.md)，与本文解耦。

---

## 设计目标

1. **Single source of truth** — `src/tests/` 与 `src/libraries/<lib>/tests/` 的所有用例都能在每个平台跑（除显式 skip 的）；**不为某平台 fork 测试集**
2. **零重新编译** — `.zbc` 在 host CI 编译一次，分发给各平台 CI 作为输入；`.zbc` 二进制平台无关
3. **Pass 判定原生化** — assert-only 用例 = 进程退出 0；golden 用例 = stdout 匹配。两种判定都是平台原生概念，无需自定义协议
4. **Library-first runner** — z42-test-runner 是 Rust **library**，CLI 只是其薄壳；每个平台直接绑库不 fork

## 非目标

- 平台间 stdout 字面 byte-identical（换行符 / locale 允许差异，golden 比对走 trim + line-by-line）
- 在每个平台都能跑**所有**用例（filesystem / threading / native FFI 等用例可显式 `[SkipPlatform]`）
- iOS 真机 CI（v0.1 simulator-only；真机走 manual smoke）

---

## 架构总览

```
┌──────────────── Host CI（Linux / macOS）─────────────────┐
│                                                            │
│  src/tests/*.z42                                            │
│  src/libraries/<lib>/tests/*.z42                            │
│              │                                              │
│              ▼ z42c (C# 编译器) — 一次编译                  │
│  artifacts/test-zbcs/                                       │
│      ├── tests/<category>/<name>.zbc                        │
│      └── libraries/<lib>/<name>.zbc                         │
│              │                                              │
│              ▼ tar 打包 (artifacts/test-zbcs.tar)           │
│              │                                              │
│              ▼ 上传 GitHub Actions artifact                 │
└────────────────────────┬───────────────────────────────────┘
                         │
        ┌────────────────┼─────────────────────┐
        ▼                ▼                     ▼
   ┌────────┐      ┌──────────┐         ┌──────────┐
   │  WASM  │      │ Android  │         │   iOS    │
   │  CI    │      │   CI     │         │   CI     │
   └───┬────┘      └────┬─────┘         └────┬─────┘
       │                │                    │
       │ vitest         │ JUnit              │ XCTest
       │ + wasm-bindgen │ + JNI              │ + Swift FFI
       │                │                    │
       ▼                ▼                    ▼
   z42-test-runner (Rust library, 同一份 src/toolchain/test-runner/)
       │                │                    │
       ▼                ▼                    ▼
   z42_vm interp   z42_vm interp        z42_vm interp / aot
   (interp-only)   (interp-only)        (interp-only)
       │                │                    │
       ▼                ▼                    ▼
   Pass / Fail / Skip + captured stdout/stderr  ──► 各平台原生 test report
```

**核心抽象：** test-runner 不感知平台。各平台只负责
- 把 `.zbc` 字节读出来递给 runner
- 把 runner 返回的 `TestResult` 翻译成本平台 test framework 的 pass/fail
- 串接平台特有的 stdout/sink（捕获到字符串而非真打印）

---

## test-runner library API

按 [rewrite-z42-test-runner-compile-time](../../spec/archive/2026-05-12-rewrite-z42-test-runner-compile-time/) spec 落地。最小可绑定 surface：

```rust
// src/toolchain/test-runner/src/lib.rs

/// 执行一个 .zbc 中的所有 [Test] / [Benchmark]。
pub fn run_zbc(bytes: &[u8], opts: RunOptions) -> SuiteResult;

/// 执行单个 [Test]（按 method id 或 fully-qualified name）。
pub fn run_one(bytes: &[u8], target: &str, opts: RunOptions) -> TestResult;

pub struct RunOptions {
    pub filter:   Option<String>,    // substring / regex
    pub tags:     Vec<String>,
    pub format:   OutputFormat,      // Pretty / Tap / Json
    pub bench:    bool,
    pub platform: PlatformInfo,      // ★ 用于 [SkipPlatform] 判定
}

pub struct PlatformInfo {
    pub name:         &'static str,       // "host" / "wasm" / "android" / "ios"
    pub mode:         ExecMode,           // Interp / Jit / Aot
    pub capabilities: CapabilitySet,      // bitset of supported capabilities
}

/// Closed enum — adding a new capability requires updating both the enum
/// and the C# AttributeBinder's whitelist (E0917 unknown-capability check).
bitflags! {
    pub struct CapabilitySet: u16 {
        const JIT        = 0b0000_0001;
        const AOT        = 0b0000_0010;
        const THREADING  = 0b0000_0100;
        const FILESYSTEM = 0b0000_1000;
        const NETWORK    = 0b0001_0000;
        const DLOPEN     = 0b0010_0000;
        const SUBPROCESS = 0b0100_0000;
    }
}

// 各平台编译期常量
pub const HOST_CAPS:    CapabilitySet = CapabilitySet::all();
pub const WASM_CAPS:    CapabilitySet = CapabilitySet::empty();   // 暂全无；Worker+SAB 落地后开 THREADING
pub const ANDROID_CAPS: CapabilitySet = CapabilitySet::THREADING.union(CapabilitySet::FILESYSTEM)
                                                                  .union(CapabilitySet::NETWORK);
pub const IOS_CAPS:     CapabilitySet = CapabilitySet::THREADING.union(CapabilitySet::FILESYSTEM)
                                                                  .union(CapabilitySet::NETWORK);
// JIT / AOT / DLOPEN / SUBPROCESS 在 mobile / wasm 全 false（policy / sandbox）

pub enum TestResult {
    Pass    { name: String, duration_ns: u64 },
    Fail    { name: String, message: String, stdout: String, stderr: String },
    Skip    { name: String, reason: String },
}

pub struct SuiteResult {
    pub results: Vec<TestResult>,
    pub formatted: String,           // 完整 Pretty/Tap/Json 文本，平台直接转交本地 reporter
}
```

平台只需 `extern "C"` 导出 `run_zbc` 等价（wasm-bindgen / JNI / C ABI）。

---

## 各平台绑定模式

### WASM（[add-platform-wasm](../../spec/archive/2026-05-12-add-platform-wasm/)）

```js
// platform/wasm/test/runner.test.js  (vitest)
import { Z42TestRunner } from '@z42/runtime';
import { readFileSync, readdirSync } from 'fs';

describe('z42 cross-platform tests on wasm', () => {
  const zbcDir = process.env.Z42_TEST_ZBCS;  // host CI 解包后的目录
  for (const file of readdirSync(zbcDir).filter(f => f.endsWith('.zbc'))) {
    test(file, async () => {
      const bytes = readFileSync(`${zbcDir}/${file}`);
      const result = await Z42TestRunner.runZbc(bytes, { platform: 'wasm' });
      expect(result.allPassed).toBe(true);
    });
  }
});
```

- 浏览器场景用 playwright + IndexedDB / fetch 加载 `.zbc`
- Node.js 场景直接 fs（更适合 CI）
- stdout 在 wasm 下走 `Z42TestIO.captureStdout` → 字符串（无 stdio）

### Android（[add-platform-android](../../spec/archive/2026-05-12-add-platform-android/)）

```kotlin
// platform/android/z42-runtime/src/androidTest/.../Z42TestRunnerTest.kt
@RunWith(Parameterized::class)
class Z42TestRunnerTest(private val zbcAsset: String) {

    @Test fun runOnAndroid() {
        val bytes = context.assets.open("zbcs/$zbcAsset").readBytes()
        val result = Z42TestRunner.runZbc(bytes, platform = "android")
        assertTrue(result.allPassed, result.formatted)
    }

    companion object {
        @Parameterized.Parameters
        @JvmStatic fun cases() = context.assets.list("zbcs")
    }
}
```

- `.zbc` 作为 instrumented test asset 打包
- `Z42TestRunner.runZbc` 是 JNI 包装，内部调用 Rust `run_zbc(bytes, opts)`
- emulator / device 上跑（CI 用 macOS runner 提供 x86_64 emulator 加速）

### iOS（[add-platform-ios](../../spec/archive/2026-05-12-add-platform-ios/)）

```swift
// platform/ios/Tests/Z42RuntimeTests/Z42TestRunnerTests.swift
class Z42TestRunnerTests: XCTestCase {
    func testAllZbcs() throws {
        let bundle = Bundle.module
        for url in try bundle.urls(forResourcesWithExtension: "zbc")! {
            let bytes = try Data(contentsOf: url)
            let result = try Z42TestRunner.runZbc(bytes, platform: "ios")
            XCTAssertTrue(result.allPassed, result.formatted)
        }
    }
}
```

- `.zbc` 作为 SwiftPM resource bundle
- Swift facade → C bridge → Rust `run_zbc`
- iOS App Store policy 禁 JIT → `interp-only` features；AOT 占位（M9 落地后真启）

---

## 测试选择性（skip 机制）

两层机制，先 capability 后 platform：

| 层级 | Attribute | 用途 |
|---|---|---|
| **能力门控**（首选） | `[Feature("threading")]` | 测试需要某项**能力**才能运行；任何不具备该能力的平台自动 skip |
| **平台 escape hatch** | `[SkipPlatform("ios")]` | 测试因平台特定 bug / 限制需跳过，**与能力无关** |

### 既有：`[Skip]` attribute

`testing.md` R1 已支持 `[Skip(reason: ...)]`（无条件）+ TIDX `skip_reason` 字段。

### 能力注册表（Capability Registry）

平台能力是**封闭枚举**，避免拼写错误 / 漂移。当前定义：

| Capability | 含义 | host | wasm | android | ios |
|---|---|:---:|:---:|:---:|:---:|
| `jit` | Cranelift JIT codegen | ✅ | ❌ 沙箱禁动态 codegen | ✅ M9+ | ❌ App Store policy |
| `aot` | AOT 编译产物（M9+） | ⚠️ 占位 | ❌ | ⚠️ 占位 | ⚠️ 占位 |
| `threading` | 多线程执行（pthread / Worker / GCD） | ✅ | ⚠️ 仅 Worker+SAB | ✅ | ✅ |
| `filesystem` | 完整 POSIX 文件 I/O | ✅ | ❌ 默认无 | ⚠️ scoped storage | ⚠️ sandbox |
| `network` | TCP socket / HTTP client | ✅ | ⚠️ fetch / WebSocket | ✅ | ✅ |
| `dlopen` | 动态加载 native library（FFI） | ✅ | ❌ | ⚠️ NDK only | ❌ App Store |
| `subprocess` | spawn 子进程 | ✅ | ❌ | ❌ | ❌ App Store |

平台能力集在编译期由 cargo features 决定，运行时通过 `PlatformInfo.capabilities` 暴露给 runner。

> **多线程具体语义**：z42 的并发模型属 L3 范围（参 [features.md](../features.md)），尚未落地。`threading` capability 现在主要给 stdlib `Std.Threading` 类（占位）+ 未来 `async`/`Task` 测试预留接口。落地之前所有用例不带 `[Feature("threading")]`，capability 默认对所有平台 false。

### 既有：`[Feature]` attribute（扩用法）

```z42
[Test]
[Feature("jit")]
void test_jit_inlining() { ... }       // 仅 host 跑（其他平台 capabilities 不含 jit）

[Test]
[Feature("threading")]                  // 未来 L3 落地后启用
void test_parallel_map() { ... }

[Test]
[Feature("filesystem")]
void test_read_file() { ... }            // host + 带 filesystem 的移动平台 sandbox 路径

[Test]
[Feature("threading", "filesystem")]    // AND 语义：两项都需要
void test_concurrent_log_write() { ... }
```

runner 检查：`RunOptions.platform.capabilities.is_superset_of(test.required_features)` 不满足 → status=Skip。

### `[SkipPlatform]` —— 平台特定 escape hatch

仅当**问题与能力无关**（例如 wasm runtime 某版本 bug、特定平台数值精度差异）时使用：

```z42
[Test]
[SkipPlatform("wasm", reason: "wasm-bindgen 0.2.x bug #1234")]
void test_some_specific_case() { ... }
```

编译器在 TIDX 写 `skip_platforms: List<String>`（R1 已有 `platform: String` 字段，扩为列表）。

**优先级原则**：能用 `[Feature]` 表达就**不要**用 `[SkipPlatform]`。`[Feature]` 是声明式（"此测试需要 X 能力"），`[SkipPlatform]` 是命令式（"在 Y 平台跳过"）—— 前者会随平台能力升级自动启用，后者需手动维护。

### 默认行为

所有 `src/tests/` + `src/libraries/<lib>/tests/` 的用例在每个平台都跑（除 `[Feature]` / `[SkipPlatform]` / `[Skip]` 显式 opt-out 外）。

---

## 跨平台 parity 期望

| 测试类别 | 用到的 capability | host | wasm | android | ios |
|---|---|:---:|:---:|:---:|:---:|
| 基础语法 / 类型 / 控制流 | — | ✅ | ✅ | ✅ | ✅ |
| 类 / 接口 / 继承 / 泛型 | — | ✅ | ✅ | ✅ | ✅ |
| 异常 / try-catch | — | ✅ | ✅ | ✅ | ✅ |
| Closure / Lambda | — | ✅ | ✅ | ✅ | ✅ |
| GC / 弱引用 | — | ✅ | ⚠️ 时序差异 | ✅ | ✅ |
| ref/out/in（dir-mode + interp_only） | — | ✅ interp | ✅ interp | ✅ interp | ✅ interp |
| JIT 专项（如 jit_transitive） | `jit` | ✅ | skip | ✅ M9+ | skip |
| Multithreading（L3+） | `threading` | ✅ | ⚠️ Worker+SAB | ✅ | ✅ |
| Filesystem stdlib | `filesystem` | ✅ | skip | ✅ scoped | ✅ sandbox |
| Network stdlib | `network` | ✅ | ⚠️ fetch only | ✅ | ✅ |
| Native FFI（dlopen 路径） | `dlopen` | ✅ | skip | ⚠️ NDK | skip |
| Subprocess spawn | `subprocess` | ✅ | skip | skip | skip |
| Cross-zpkg 多包 | — | ✅ | 暂不跑 | 暂不跑 | 暂不跑 |

`skip` = 测试自动跳过（缺所需 capability）；`⚠️` = 子集可跑或语义略有差异。

> 比例预估：当前 157 case → 各平台预计跑 ~140 个（jit / native FFI / filesystem 共 ~15-17 个 capability-gated；threading 类 L3 起逐步增加）。

---

## CI 矩阵

```yaml
jobs:
  host-tests:                       # 既有
    runs-on: ubuntu-latest
    steps:
      - dotnet test
      - ./xtask test e2e        # interp + jit
      - ./xtask test lib
      - tar artifacts/test-zbcs/*.zbc → upload-artifact

  wasm-tests:
    needs: host-tests
    runs-on: ubuntu-latest
    steps:
      - download-artifact test-zbcs
      - cd platform/wasm && pnpm test  # vitest 跑遍所有 .zbc

  android-tests:
    needs: host-tests
    runs-on: macos-latest             # x86_64 emulator 加速
    steps:
      - download-artifact test-zbcs → assets/zbcs/
      - ./gradlew connectedAndroidTest

  ios-tests:
    needs: host-tests
    runs-on: macos-latest             # Xcode + simulator
    steps:
      - download-artifact test-zbcs → resources/
      - xcodebuild test -destination 'platform=iOS Simulator,...'
```

**合并门禁**：host-tests 必通；wasm/android/ios 任一失败标记 PR 红，但允许 manual override（v0.1 各平台稳定性未证明前的 escape hatch）。

---

## 设计决策

### Decision 1: Library-first vs subprocess-per-platform

**问题：** 每个平台直接绑 test-runner library，还是各自维护一个进程级 wrapper？

**选项：**
- **A：library-first（采纳）** —— test-runner 是 Rust crate，`pub fn run_zbc(...)`；wasm/android/ios 用 wasm-bindgen / JNI / C ABI 各自绑
- **B：subprocess-per-platform** —— 每平台跑一个 z42vm-test 进程，用文件 / pipe 输出 TestResult

**采纳 A**：
- 移动平台 subprocess 启动慢且受限（iOS 完全禁子进程；Android 受 sandbox 限）
- 库调用零额外开销，wasm 唯一可行路径（无进程概念）
- CLI 仍存在，host 用；移动平台直接绑库
- 代价：需要把 runner 的逻辑彻底解耦于 stdio（用 `String` 而非 `println!`）—— 与 `Std.Test.TestIO.captureStdout` 已有方向一致

### Decision 2: zbc 是否每平台重新编译

**问题：** host CI 编译一份 .zbc 分发，还是每平台 CI 各自编译？

**选项：**
- **A：host 编译一次（采纳）** —— `.zbc` 平台无关；编译只在 host
- **B：各平台 cross-compile** —— 每平台 CI 复制一份编译流程

**采纳 A**：
- `.zbc` 设计就是平台无关字节码，跨平台编译没意义
- 减少 CI 时间（C# 编译器只跑一次）
- 让 .zbc 与 z42c 版本绑定明确（host commit hash 即版本）
- 代价：上下游 CI job 强依赖 → 用 GitHub Actions `needs:` + `upload-artifact` / `download-artifact` 解决

### Decision 3: SkipPlatform 写在源还是 runner

**问题：** 平台过滤的归属。

**选项：**
- **A：源代码级 attribute（采纳）** —— `[SkipPlatform("wasm")]` 编译期写 TIDX，runner 读取
- **B：runner 配置级** —— 维护一份 `wasm-skip-list.toml`，runner 启动时按文件过滤

**采纳 A**：
- 与代码同居 → grep 即知；改测试时改 attribute 同步
- TIDX 已有 `platform` 字段（R1 落地），扩为列表零格式 churn
- 单一真相来源；CI runner 不需维护额外清单
- 代价：需要给 [SkipPlatform] 加 attribute validator（E0916+ 错误码）—— 现有 R4.A 框架可扩

### Decision 4: Capability 表示 —— 字符串 vs 封闭枚举

**问题：** `[Feature("jit")]` 的参数是任意字符串还是封闭枚举？

**选项：**
- **A：开放字符串** —— 任何字符串都接受；runner 与 PlatformInfo.capabilities 做字符串集合比较
- **B：封闭枚举（采纳）** —— 编译期把 `"jit"` / `"threading"` 等映射到 `CapabilitySet` 位；attribute validator 拒绝未注册名称（E0917 unknown-capability）

**采纳 B**：
- 拼写错误（`[Feature("threadnig")]`）会变"永远 skip"的隐形 bug；封闭枚举编译期拒绝
- Runtime 用 bitset 比对比字符串 set 快（runner 启动 O(1)）
- 强制开发者通过 PR 把新 capability 加进表 → 触发讨论"这能力意味着什么、各平台是否提供"
- 代价：每加新 capability 改 enum 定义 + C# 端 whitelist + 各平台常量；接受这个 friction，因为 capability 数量本就少（10 内）

**新 capability 的添加流程：**
1. `src/runtime/src/test_runner/capabilities.rs` 加 enum variant
2. 各 `<PLATFORM>_CAPS` 常量决定该平台是否提供
3. C# 端 `CapabilityRegistry.cs` whitelist 加入名称（attribute validator 用）
4. `cross-platform-testing.md` capability 矩阵表加一行

---

## xtask 三阶段平台测试管线（已落地：add-platform-test-pipeline, 2026-06-15）

上文（Phase 4/5）原计划三平台各自在自己的 `build.sh` / `test.sh` 里跑测试。实践中
这导致三套各自为政的 bash，且"编 fixture + 收 stdlib"在三处重复（最易漂移）。
2026-06-15 统一到**接口驱动的 z42 框架**（`scripts/test/xtask_test_platform.z42` + 三个
backend 文件），三步可单独调用、共享逻辑只写一份。

### 三阶段（每步可单独跑，亦可串联）

| 阶段 | 共享（框架一份） | 平台特定（backend） |
|------|----------------|--------------------|
| **① build project** | — | desktop→`cargo rustc` staticlib(libz42.a)；wasm→`wasm-pack`；iOS→cargo×targets+`xcframework`；Android→`cargo-ndk`+gradle AAR |
| **② build test assets** | 编 R1–R7 fixtures→.zbc（z42c）+ 收 stdlib zpkg；落点参数化 | 落点不同（desktop 不拷 stdlib，search_paths 直指 _libsDir；wasm 加 `files.json`）|
| **③ run tests** | 统一 JUnit 报告落点 | runner 不同：desktop→`cc r1_r7.c`+链 libz42.a+跑；wasm→Playwright；iOS→`swift test`/Simulator；Android→gradle `connectedAndroidTest` |

> **desktop（add-desktop-platform-backend, 2026-06-16）**：第 4 个 backend，补桌面 Tier-1 C ABI 的 facade 级 R1–R7（真实外部 C 消费者链 libz42.a，对齐 wasm/iOS/Android）。本地 `./xtask test platform desktop` 7/7 验证 + junit。源 `desktop/tests/r1_r7.c`。

### 接口契约

```
public interface IPlatformBackend {
    string      Name();                  // "wasm" | "ios" | "android"
    int         BuildProject(string root);   // ① 平台原生构建
    AssetLayout Assets(string root);          // ② 落点声明（fixturesDir / stdlibDirs / wantFilesJson）
    int         RunTests(string root);        // ③ 平台 runner
}
```

`_platformAssets`（框架）用 `backend.Assets()` 的落点把"编 fixture + 收 stdlib +
可选 files.json"做一遍——这是消除三处重复的关键。新增平台 = 加一个 backend class +
注册一行。

**CLI**：`./xtask test platform <wasm|ios|android|all> [build|assets|run]`
（无 step = 全流程）。

### R1–R7 平台冒烟契约（单一真相源）

三平台跑**同一组 7 个场景**（测试代码因宿主语言不同分 Playwright/XCTest/JUnit 三份，
但场景 + 期望状态码这份契约统一在此）。注意此 "R1–R7" 是**平台冒烟场景编号**，与
[testing.md](testing.md) 的需求 "R1" 无关。

| 场景 | 内容 | 期望 |
|------|------|------|
| R1 | hello world 冒烟 | stdout 单行 |
| R2 | 坏 zbc | status 10 (BAD_ZBC) |
| R3 | 未知入口 | status 20 (ENTRY_NOT_FOUND)，message 含 FQN |
| R4 | 参数个数错 | status 21 (ARG_MISMATCH) |
| R5 | resolver miss | 在 load/invoke 浮现（status 10 或 30）|
| R6 | 生命周期：重复 init 3× | 3 段输出 |
| R7 | 多行 stdout | 保序（a\nb\nc）|

状态码↔平台异常映射见 [`platform-contract.md`](../../../src/toolchain/workload/platform-contract.md) §错误码。

### CI 聚合 dashboard 方向（地基已铺，传输层延后）

每平台 ③ 把 JUnit 落到统一 `artifacts/test-reports/<platform>/junit.xml`。未来 CI change
让各平台 job 经 **GitHub Checks API**（test-reporter action）把这些报告聚合成 PR 上的
check runs —— GitHub 即"远程同步层"，**不需自建服务器**。CI job + Checks 接线是独立
change（见 roadmap `infra-ci-platform-test-dashboard`）。

### 落地状态（2026-06-16，三平台 CI 全绿）

infra-ci-platform-test-dashboard（CI run 27561709292 @ 9153fd6c）三平台 job 全 success：

- **wasm**：①②③ 端到端（本地 z42vm 7/7 + CI ubuntu Playwright/Chromium）
- **iOS**：①② + ③ 都在 `IosBackend`。③ = **真 iOS Simulator**（`xcodebuild test`，simctl 自动选第一个可用 iPhone；JUnit 由解析 xcodebuild Test Case 行产出，无需 xcbeautify）。本地（Xcode + simulator）与 CI macos-15 一致；本地实测 7/7。`Z42_IOS_DEST` 可覆盖目标。（原 `swift test` host slice 已被 simulator 取代，roadmap `ios-simulator-test` 完成）
- **Android**：①② 经 `test platform android`；③ **CI ubuntu+KVM 真 emulator**（`reactivecircus/android-emulator-runner` + `gradlew connectedAndroidTest`）R1–R7 绿。AndroidBackend 本地 `RunTests` 桥接 `test.sh`（emulator 自管）；完整 z42 化 = roadmap `port-android-emulator-run-to-z42`
- **dashboard**：每平台 job = PR 上一个 GitHub Check（success/failure）；dorny/test-reporter 出 R1–R7 明细
- **CI 部署目标坑（iter1→2 修复）**：iOS cargo build 必须 `IPHONEOS_DEPLOYMENT_TARGET=platform.ios.min_ios`（否则 zlib-ng C 部署目标 10.0 vs rlib 18.5 → `___chkstk_darwin` 链接失败）；Android Kotlin doc 注释禁含字面 `/*`（`<subdir>/*.zpkg` 中 `/*` 触发 Kotlin 嵌套块注释 → unclosed comment）
- 旧 `platforms/*/{build,test}.sh` **全保留**，CI-proven 已达成 → 退场可另开 change（roadmap `retire-platform-build-test-sh`）

## 实施分期建议

按依赖顺序：

1. **Phase 1 — test-runner library 化**（基础）
   spec：[rewrite-z42-test-runner-compile-time](../../spec/archive/2026-05-12-rewrite-z42-test-runner-compile-time/) （已 DRAFT）
   交付：`pub fn run_zbc(...)`，CLI 改成调用库；host 现有 `./xtask test lib` 不变

2. **Phase 2 — Capability 注册表 + `[SkipPlatform]` attribute**（轻量）
   新 spec（小）：
   - `CapabilitySet` enum 在 Rust + C# 端各定义一份（七项初始 capability：jit / aot / threading / filesystem / network / dlopen / subprocess）
   - `[Feature(name)]` validator：name 必须在白名单（E0917 unknown-capability）
   - `[SkipPlatform(name, reason)]` validator：name ∈ {"host", "wasm", "android", "ios"}（E0918）
   - runner `RunOptions.platform.capabilities` 与 `test.required_caps` 做 superset 比对

3. **Phase 3 — host CI 产 zbc artifact bundle**（轻量）
   新 spec（小）：一个 xtask 子命令把所有 `src/tests/**/*.z42` + `src/libraries/<lib>/tests/*.z42` 编译到 `artifacts/test-zbcs/`，CI upload

4. **Phase 4 — wasm 平台 + testing 子段**（独立）
   spec：[add-platform-wasm](../../spec/archive/2026-05-12-add-platform-wasm/)，加 Testing 段（vitest 消费 zbc bundle）

5. **Phase 5 — android + ios 平台**（独立，可并行）
   spec：add-platform-android / add-platform-ios，各自 Testing 段

每阶段独立可交付且不阻塞下一个；Phase 1 是关键路径，其余可并行。

---

## Deferred / Open Questions

- **Threading 语义**：`threading` capability 已声明，但 z42 并发模型（`async` / `Task<T>` / `lock` 等）属 L3 范围、尚未有规范。**Phase 1-5 内不会有任何 `[Feature("threading")]` 测试**。capability 注册先就位，等 L3 并发设计落地（独立 spec）后第一批 threading 测试同时进
- **wasm threading 路径**：浏览器 multi-threading 依赖 SharedArrayBuffer + `COOP/COEP` HTTP headers，不是 wasm-bindgen 默认。等 L3 并发落地时单独评估是否 wasm 跑（可能默认 `WASM_CAPS` 不含 THREADING，仅在 `+sab` build target 上开启）
- **Bencher 跨平台**：`Std.Test.Bencher` 用 `__time_now_mono_ns` builtin。wasm 需用 `performance.now()` 桥；移动平台用 `clock_gettime`。每平台需要一个 native shim。Phase 4/5 内解决。
- **真机 CI**：iOS / Android 真机租用（BrowserStack / Firebase Test Lab）成本与稳定性 trade-off，v0.1 simulator/emulator 即可，真机走 manual smoke
- **stdout 字符编码**：移动平台 NSString / Java String 都是 UTF-16，golden 比对前要 normalize 到 UTF-8 + LF。`Std.Test.TestIO.captureStdout` 已是 UTF-8 string，platform binding 转字符串时遵循
- **测试时长上限**：iOS XCTest 单测默认 60s timeout；某些 GC stress 测试可能超时 → 加 `[Timeout(secs)]` attribute（占位，不在 Phase 1-5 内）
- **wasm 内存上限**：浏览器默认 wasm linear memory ~2GB，但单 test instantiation 应 << 100MB。如有大堆测试需要 `[SkipPlatform("wasm", reason: "memory")]`
