# 跨平台测试（平台管线 · 能力门控 · CI 拓扑）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `scripts/test/xtask_test_platform.z42`（三阶段框架 + `IPlatformBackend`）、
> `scripts/test/xtask_test_{desktop,wasm,ios,android}.z42`（四个 backend）、
> `scripts/test/xtask_test_embedded_golden.z42`（`_targetExcludes` 能力门控）、
> `src/toolchain/workload/platform-contract.md`（状态码 ↔ 平台异常映射）、
> `.github/workflows/ci.yml`（`test-desktop` / `test-wasm` / `test-ios` / `test-android`）。
>
> VM 自身的构建矩阵（各平台怎么编出来）见 [跨平台构建](../runtime/cross-platform.md)。
> runner 本身的协议见 [测试框架机制](framework.md)。

本页写**同一份测试语料怎么在 desktop / wasm / iOS / Android 上跑成同一个结论**：
平台管线的三个阶段、能力门控按什么判、以及 CI 里这些 job 的真实拓扑与它们为什么长这样。
要改一个平台的测试路径、加一个平台、或者查"为什么这个用例在 wasm 上没跑"时读它。

## 1. 两个不变量

1. **不为某平台 fork 测试集**。`src/tests/**` 与 `src/libraries/<lib>/tests/**` 是唯一语料；
   平台差异只能靠**排除**表达，不能靠另写一份。
2. **`.zbc` 平台无关，只在 host 编一次**。编译只发生在 host，产物分发给各平台消费。
   跨平台重编没有意义（字节码本就平台无关），而且会让"哪个 z42c 编的"变得说不清。

这两条决定了所有下游形态：平台侧只做"读字节 → 交给 VM → 把结果翻译成本平台的 test report"。

## 2. 两条独立的平台测试路径

容易混淆的一点：CI 的平台 job 里跑的其实是**两套不同表面**，折叠在同一次模拟器/浏览器启动里。

| | R1–R7 平台冒烟 | 嵌入语料 |
|---|---|---|
| 测什么 | **嵌入 API 契约**：错误码、句柄生命周期、stdout 保序 | **语言/stdlib 语料**在该平台的执行结果 |
| 驱动 | `xtask test platform <plat> [build\|assets\|run]`（§3） | `xtask test embedded --rid <rid> [--shard k/n]` |
| 语料 | 固定 7 个场景，与仓库语料无关 | `src/tests/**` + stdlib `[Test]`，经 bundle manifest |
| 详见 | 本页 §3 | [嵌入式运行与测试 agent](embedded-app-run.md) |

R1–R7 与嵌入语料**不冗余**——前者测的是原生壳与 C ABI 的契约，后者测的是 VM 在该平台的行为。

## 3. 三阶段平台管线

`xtask test platform <desktop|wasm|ios|android|all> [build|assets|run]`
（省略 step = 全流程）。框架在 `xtask_test_platform.z42`，平台差异全在 backend：

```z42
public interface IPlatformBackend {
    string      Name();                     // "desktop" | "wasm" | "ios" | "android"
    int         BuildProject(string root);  // ① 平台原生构建
    AssetLayout Assets(string root);        // ② 落点声明
    int         RunTests(string root);      // ③ 平台 runner
}

public class AssetLayout {
    public string   fixturesDir;    // fixture .zbc 落点
    public string[] stdlibDirs;     // 可能 >1（iOS 主 bundle + test bundle）
    public bool     wantFilesJson;  // 浏览器 fetch 清单（仅 wasm）
}
```

| 阶段 | 框架做（一份） | backend 做 |
|---|---|---|
| ① build project | — | desktop `cargo rustc` staticlib；wasm `wasm-pack`；iOS cargo × targets + `xcframework`；Android `cargo-ndk` + gradle AAR |
| ② build test assets | 编 R1–R7 fixture → `.zbc` + 收 stdlib zpkg，落点参数化 | 只**声明**落点（`Assets()` 返回 `AssetLayout`） |
| ③ run tests | 统一 JUnit 报告落点 `artifacts/test-reports/<platform>/junit.xml` | desktop 链 `libz42.a` 跑 C 壳；wasm Playwright；iOS `xcodebuild test`；Android `connectedAndroidTest` |

**阶段 ② 的共享是这个设计的全部意义**。三平台各自在 `build.sh` / `test.sh` 里跑测试时，
"编 fixture + 收 stdlib" 在三处重复，是最容易漂移的一块。`_platformAssets` 拿 `backend.Assets()`
的落点把这件事做一遍，backend 只声明"放哪"。**新增一个平台 = 加一个 backend class + 注册一行。**

### 3.1 R1–R7 契约（单一真相源）

三平台跑同一组 7 个场景。测试代码因宿主语言不同分 Playwright / XCTest / JUnit 三份，
但**场景 + 期望状态码这份契约只写在这里**：

| 场景 | 内容 | 期望 |
|---|---|---|
| R1 | hello world 冒烟 | stdout 单行 |
| R2 | 坏 zbc | status 10（`BAD_ZBC`） |
| R3 | 未知入口 | status 20（`ENTRY_NOT_FOUND`），message 含 FQN |
| R4 | 实参个数错 | status 21（`ARG_MISMATCH`） |
| R5 | resolver miss | 在 load 或 invoke 浮现（status 10 或 30） |
| R6 | 生命周期：重复 init 3× | 3 段输出 |
| R7 | 多行 stdout | 保序（`a\nb\nc`） |

状态码到各平台异常类型的映射表在 `src/toolchain/workload/platform-contract.md`
的「错误码 → 平台异常映射」一节。

> 这里的 `R1–R7` 是**平台冒烟场景编号**，与测试框架自身的阶段编号无关，别混。

## 4. 能力门控

### 4.1 判据是运行期真值

一个测试能不能在某平台跑，由两处独立判定：

- **运行期、逐用例**：`[Skip(feature: X)]` → runner 查 `Std.Platform.Capabilities()`
  （见 [测试框架机制 §3.3](framework.md)）。这是 VM 二进制**实际编译进了什么**，不是静态猜测。
- **编排期、逐用例名**：`_targetExcludes(rid, caseName)` 在装 bundle 时整例排除。
  粗粒度（库级 / 前缀级），目的是让平台语料**诚实**——不去跑注定崩的用例。

第二层是本节的主题。

### 4.2 `_targetExcludes` 的两层结构

关键洞察：**mobile 的能力集与 wasm 不同，不能照搬 wasm 的排除表**。

| 能力 | wasm | mobile（iOS sim / Android emu） |
|---|:---:|:---:|
| 线程（pthreads） | ✗ | ✓ |
| 原生扩展（compression 静态链接） | ✗（无 dlopen） | ✓ |
| OS 熵（secure_random） | ✗ | ✓ |
| 系统时钟（`DateTime.UtcNow`） | ✗ | ✓ |
| 可写文件系统 | ✗ | ✓（app 沙箱 tmp/Documents） |
| server/loopback socket + DNS | ✗ | ✗（沙箱禁 bind/listen；CI 无外网） |
| 进程 fork/exec | ✗ | ✗ |
| TTY / console | ✗ | ✗（测试 harness 无 tty） |
| 可变 env / 桌面 OS 身份 | ✗ | ✗ |

于是排除表分三段（`scripts/test/xtask_test_embedded_golden.z42`）：

```
desktop（非 wasm 非 mobile）  → 全跑，直接 return false
SHARED（wasm 与 mobile 都缺）  → z42.net/* · z42.io/process* · z42.io/console* · z42.io/env* ·
                                z42.io/ansi_color · z42.io/operating_system · z42.io/platform ·
                                z42.cli/cli_env_fallback_and_mutex
WASM-ONLY（仅 isWasm）        → z42.threading/* · z42.compression/* · *stream* · z42.io/file* ·
                                z42.io/directory* · z42.io/path_glob* · z42.io/gc_heap_snapshot ·
                                z42.crypto/secure_random* · z42.core/datetime
ANDROID-ONLY（仅 isAndroid）  → z42.io/file_chmod_link_size · z42.io/gc_heap_snapshot
```

三条值得单独记的：

- **`z42.compression` 是唯一的 native-ext facade 库**（brotli/gzip/zstd/zip/lz4/tar 全走
  `[Native(lib="z42_compression")]`）。wasm 无 dlopen → ext 注册表空 → **元数据 resolver 加载即 panic**
  （`unknown builtin __brotli_compress`），不是断言失败。crypto 走静态 `BUILTINS`，wasm 有。
- **`file_chmod_link_size` 只在 Android 排除**：Android app 沙箱拒绝 POSIX 硬链接
  （`File.Link` → `Permission denied`；symlink 却可以），iOS 沙箱能跑。**不要上移到 SHARED。**
- **`gc_heap_snapshot` 在 Android 的排除是内存问题，不是文件系统问题**：
  `GC.WriteHeapSnapshot` 把整个活堆序列化成 V8 JSON 串再读回；嵌入 runner 里 `[Test]` unit
  共享一个 VM，此例在分片内偏后跑时累积堆已很大 → 快照膨胀到数 GB → Android 模拟器的
  low-memory-killer 直接 SIGKILL 整个 app。iOS-sim（macOS 宿主内存充足）跑得过，desktop 非共享 VM。

### 4.3 排除表怎么演进

mobile 那半边的 fs / stream **保留**是一个**能力假设**，靠 tier-2 CI 分片验证：app 沙箱有可写 tmp，
所以 `file_temp` / `directory_temp` / 各种 stream 假定可跑。某例实际需要沙箱拒绝的东西时，
CI dispatch 会把它显成红，**按证据**加排除。

> 全覆盖的原则是「揭真实平台差异、按证据收敛」，不是「先排干净求绿」。
> 反向的例子也有：曾有 4 个 `z42.io` 用例硬编码 `/tmp/...` 写盘路径——iOS-sim（跑在 macOS）与
> desktop 有可写 `/tmp` 所以过，Android 模拟器无可写 `/tmp` 直接 `Read-only file system`。
> 那**不是**能力缺口，是测试自身的可移植性缺陷；改用 `File.CreateTempDir` 后四例全平台通过，
> 它们的临时排除被删掉。分不清这两类，排除表就会越滚越大且再也说不清为什么。

## 5. CI 拓扑

### 5.1 真实 job

| job | 显示名 | tier | 触发 | 跑什么 |
|---|---|---|---|---|
| `test-desktop` | `test-desktop-cabi(linux-x64)` | 1 | `platform` 变更 / schedule / dispatch | `test platform desktop`（R1–R7） |
| `test-wasm` | `test-wasm-browser(linux-x64) shard k` | 2 | **仅** schedule / dispatch | shard 1 跑 R1–R7；每片跑 `test embedded --rid browser-wasm --shard k/3` |
| `test-ios` | `test-ios-sim(macos-arm64) shard k` | 2 | **仅** schedule / dispatch | 每片单次 `xcodebuild test -scheme Z42VM` 同时跑 R1–R7 与嵌入语料 |
| `test-android` | `test-android-emu(linux-x64) shard k` | 2 | **仅** schedule / dispatch | 每片单次 `connectedAndroidTest` 同时跑 R1–R7 与嵌入语料 |

三个 tier-2 job 都是 `matrix.shard: [1,2,3]`，`needs: toolchain-bootstrap`，`fail-fast: false`。

### 5.2 为什么 tier-2 是 nightly-only

浏览器 / 模拟器 / 真机模拟器慢且重（wasm 冷构建约 27 分钟；Android 每片冷启动模拟器），
放进每个 PR 会把日常迭代拖垮。Tier-1 的 `test-desktop`（纯 C ABI，无浏览器/模拟器）仍按
`platform` 变更过滤器进 PR。

### 5.3 为什么分片而不是提 cap

单 job 有**时间墙**（wasm 的 Playwright、Android 的 60 分钟模拟器）。提 cap 迟早撞墙；
分片把语料的**编译 + 跑**摊到 n 个平行 runner，每片只跑 1/n，墙不动而覆盖到 100%。
`--shard k/n` 的并集 = 全集、零重叠（纯 index 取模分区）。

已知代价（可测、已接受）：矩阵每片是独立 runner，**各自重付一遍平台冷构建**。
所以 wasm 的 R1–R7（与语料无关）只在 `shard == 1` 跑，避免 3× 冗余；
而 mobile 的 R1–R7 **折叠进同一次 `xcodebuild test` / `connectedAndroidTest`**，
没法从 scheme 里摘出来，于是每片冗余重跑——相对每片的 boot + 语料成本可忽略，
换来的是「不二次启动模拟器」。

一次遇到墙的具体形态值得记：wasm 单片语料在浏览器内 interp（每例 fresh VmContext + stdlib reload、
单线程）整体已超 10 分钟，撞穿了 Playwright 配置里旧的 620 秒 whole-test timeout，
表现为 **shard 2 确定性红**。解法是提 timeout（25 分钟）+ 抬 job 墙（75 分钟），
而不是提 n——每加一片都要重付一遍冷 `wasm-pack` 构建，提 timeout 零额外 job。

### 5.4 报告聚合

每平台 ③ 把 JUnit 落到 `artifacts/test-reports/<platform>/junit.xml`，CI 用 `dorny/test-reporter`
把它变成 PR 上的一个 GitHub Check（R1–R7 明细逐条可见）。
junit / logcat / crash-diagnostics 这些 artifact 均按 `${{ matrix.shard }}` 命名，避免矩阵内碰撞。
**GitHub Checks 就是"远程同步层"**，不需要自建服务。

## 6. 在哪改

| 要改什么 | 改哪 |
|---|---|
| 加一个平台 | 新 backend class 实现 `IPlatformBackend` + `_platformDispatch` 注册一行 |
| 某平台的原生构建 / runner | 对应 `scripts/test/xtask_test_<plat>.z42` |
| R1–R7 场景或状态码 | 本页 §3.1 + `src/toolchain/workload/platform-contract.md` + 三份宿主语言测试 |
| 某用例在某平台跑不了 | `_targetExcludes`（`xtask_test_embedded_golden.z42`），并在注释里写清是能力缺口还是可移植性缺陷 |
| 分片数 / 超时 / 触发条件 | `.github/workflows/ci.yml` 对应 job |
