# platforms/ — Tier 3 platform facades for the z42 Embedding API

> 状态：📋 H4 设计期（2026-05-11）— 三平台 facade 共同契约。
>
> 上层规范：[`docs/internals/src/runtime/embedding.md`](../../../../docs/internals/src/runtime/embedding.md) §6 Tier 3 + §0 host/mobile 编译边界。
> 前置 ABI：[`docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/`](../../../../docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/) 必须先落地。

---

## 编译边界（重要）

**移动 / wasm 平台只装 VM，不带编译器**。每个 facade 的 `build.sh` 在 host 端用 `z42c` 把 `.z42` 源（含 stdlib 与 test fixture）编出 `.zbc` / `.zpkg`，复制进 `Z42VM.xcframework/Resources/` / `z42vm/src/main/assets/` / `pkg-{web,nodejs}/`；mobile 设备上**只 load 二进制产物**，不调 `z42c` / `dotnet`。该原则在自举完成前不变；详细背景见 [`embedding.md §0`](../../../../docs/internals/src/runtime/embedding.md#§0-编译边界host-编--mobile-跑)。

---

## 职责

把 z42 VM 包装成各**目标平台原生形态**的依赖包，让 native 应用（iOS / Android / 浏览器 / Node.js / wasm runtime）一行 `import` 就能跑 `.zbc`。

所有平台 facade **统一架在 [Tier 2 `z42-host` crate](../../workload/host-api/) 之上**（consolidate-platform-into-workload S1 已从 `host/embed/` 迁至 `workload/host-api/`）；没有任何 facade 直接调 `z42_runtime::interp::*` 或 `z42::host::*` extern C 函数 —— 那是 host 模块自己内部的事。

```
┌─────────────────────────────────────────────────────────────┐
│  iOS app (Swift)  │  Android app (Kotlin)  │  JS / TS app   │
└────────┬──────────┴──────────┬─────────────┴────────┬───────┘
         │ SwiftPM Z42VM       │ AAR Z42VM            │ npm Z42VM
         ▼                     ▼                      ▼
┌─────────────────────────────────────────────────────────────┐
│  platforms/ios/  │  platforms/android/  │  platforms/wasm/  │
│  (Swift facade   │  (Kotlin facade      │  (TS facade       │
│   + C bridge     │   + JNI bridge       │   + wasm-bindgen  │
│   + Rust cdylib) │   + Rust cdylib)     │   + Rust cdylib)  │
└────────┬─────────┴────────┬─────────────┴──────────┬────────┘
         │ extern "C"       │ extern "system" (JNI)  │ #[wasm_bindgen]
         ▼                  ▼                        ▼
┌─────────────────────────────────────────────────────────────┐
│         src/toolchain/workload/host-api/  (Tier 2)         │
│              z42-host crate — single source of truth        │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│            src/runtime/  (interp-only)                      │
└─────────────────────────────────────────────────────────────┘
```

不做：
- VM 执行引擎本身（归 `src/runtime/`）
- C ABI 实现（归 `src/runtime/src/host/`）
- 嵌入示例（C / Rust 宿主）：由学习手册「在 C / Rust 程序中嵌入 z42」一章的 `examples/platforms/embedding/` 提供（章节落地前暂缺）
- 测试 runner（归 `src/toolchain/test-runner/`）

---

## 共同 facade 形态

三个平台**必须暴露相同概念集合**，方法名按各语言惯例：

| 概念 | Swift (iOS) | Kotlin (Android) | TypeScript (WASM) |
|------|-------------|------------------|-------------------|
| VM 句柄 | `class Z42VM` | `class Z42VM : AutoCloseable` | `class Z42VM` |
| 已加载模块 | `class Z42VMModule` | `class Z42VMModule` | `class Z42VMModule` |
| 已解析入口 | `class Z42VMEntry` | `class Z42VMEntry` | `class Z42VMEntry` |
| 值类型 | `enum Z42VMValue { case null_, i64(Int64), f64(Double), bool(Bool) }` | `sealed class Z42VMValue { ... }` | `type Z42VMValue = null \| { i64: bigint } \| ...` |
| 错误 | `enum Z42VMError: Error` | `class Z42VMException(val status: Int, val message: String)` | `class Z42VMError extends Error` |
| zpkg 解析协议 | `protocol ZpkgResolver` | `interface ZpkgResolver` | `type ZpkgResolver = (name: string) => Uint8Array \| null` |

### 同形 API（最小 surface）

```
// 构造（接受可选 ZpkgResolver；默认平台-bundle 实现）
init(zpkgResolver: ZpkgResolver = <platform default>) throws

// 加载用户 .zbc 字节
loadZbc(bytes: BytesType) -> Z42VMModule

// 按 FQN 解析入口
resolveEntry(_ m: Z42VMModule, fqn: String) -> Z42VMEntry

// 同步调用（H2 仅支持 null / i64 / f64 / bool；string + object marshal 推迟）
invoke(_ e: Z42VMEntry, args: [Z42VMValue] = []) throws -> Z42VMValue

// stdout / stderr 回调（每条 z42 输出触发一次）
setStdoutHandler(handler: (BytesType) -> Void)
setStderrHandler(handler: (BytesType) -> Void)

// 资源释放（iOS/Swift deinit 自动；Kotlin AutoCloseable.close；JS dispose）
```

完整方法签名、错误码映射、`Z42VMValue` 构造器等具体形态由各平台 spec 锁定（链接见下方）。

---

## ZpkgResolver 协议

桌面端 z42 通过 `search_paths` 扫文件系统找 `*.zpkg`；移动 / wasm 没有文件系统（或不便扫），改成**回调机制**让宿主告诉运行时"namespace X 的 zpkg 字节在这里"。

### Tier 1 C ABI（前置 spec [`add-zpkg-resolver-hook`](../../../../docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/) 添加）

```c
typedef int (*Z42ZpkgResolverFn)(
    const char*  namespace_name,    /* "Std.IO" / "z42.core" */
    const uint8_t** out_bytes,
    size_t*      out_length,
    void*        user_data);

/* Appended at end of Z42HostConfig (ABI version unchanged): */
Z42ZpkgResolverFn  zpkg_resolver;
void*              zpkg_resolver_user_data;
```

### Tier 2 Rust trait

```rust
pub trait ZpkgResolver: Send + Sync {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>>;
}
```

### 各平台默认实现

| 平台 | 默认 resolver | 自定义路径 |
|------|--------------|-----------|
| **iOS** | `BundleZpkgResolver()` 在 `Bundle.main` 查 `<ns>.zpkg` | 实现 `ZpkgResolver` 协议，构造时传入 |
| **Android** | `AssetZpkgResolver(context.assets)` 读 `assets/stdlib/<ns>.zpkg` | 实现 `ZpkgResolver` interface |
| **WASM** | `MapZpkgResolver(Map<string, Uint8Array>)`（预 fetch 模式）+ `JsZpkgResolver(fn)`（懒 fetch 模式） | 任意 `(name: string) => Uint8Array \| null` |

桌面 `search_paths` 与 resolver 可**并存**；runtime 先调 resolver，miss 后回退扫 search_paths。

---

## 命名约定

| 项 | 规则 |
|----|------|
| **类名** | `Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMError`、`Z42VMException` —— `VM` 全大写跨语言一致 |
| **SwiftPM 包名** | `Z42VM`（Package.swift target name） |
| **AAR groupId / artifactId** | `io.z42:z42vm`（lowercase per Maven convention，类名仍 `Z42VM`） |
| **npm 包名** | `@z42/wasm`（scope `@z42`，包 `wasm`） |
| **Kotlin package** | `io.z42.vm` |
| **Swift module** | `Z42VM` |
| **Rust binding crate**（platforms/<x>/rust/） | `z42-platform-ios` / `z42-platform-android` / `z42-platform-wasm` |

---

## 构建产物

| 平台 | 产物 | 构建命令 |
|------|------|--------------|
| iOS | `Z42VM.xcframework`（含 ios-arm64 + ios-arm64_x86_64-simulator slices） | `./xtask test platform ios build`（IosBackend）|
| Android | `z42vm-<version>.aar`（含 arm64-v8a / x86_64 jniLibs；32-bit ABI 已退场） | `./xtask test platform android build`（AndroidBackend）|
| WASM | npm package（`pkg-web/` + `pkg-nodejs/`） | `./xtask test platform wasm build`（WasmBackend）|

> 构建逻辑已从各 `build.sh` 迁入 `scripts/xtask_test_{ios,android,wasm,desktop}.z42` 的 `IPlatformBackend` 后端（统一三阶段管线）。Android 的 `test.sh`（emulator 编排）暂留，由 `AndroidBackend.RunTests` 桥接。

每个产物**不入 git**（二进制大；CI 在 release 时上传）；源代码入 git。

---

## 平台资源 bundle

| 平台 | stdlib zpkg 位置 | 加载方式 |
|------|------------------|---------|
| iOS | `platforms/ios/Resources/stdlib/*.zpkg` → Xcode `Copy Bundle Resources` | `Bundle.main.url(forResource:withExtension:"zpkg")` |
| Android | `platforms/android/z42vm/src/main/assets/stdlib/*.zpkg` | `assetManager.open("stdlib/$name.zpkg")` |
| WASM | `platforms/wasm/js/stdlib/*.zpkg` → 打进 npm tarball | 浏览器 `fetch(url)` / Node `fs.readFileSync` |

zpkg 文件本身**由 `dotnet build src/compiler/z42.slnx` 编译标准库产出**，build script 复制到各平台 bundle 路径。

---

## 平台索引

| 平台 | 目录 | spec | 状态 |
|------|------|------|------|
| iOS | [`ios/`](ios/) | [`add-platform-ios/`](../../../../docs/spec/archive/2026-05-12-add-platform-ios/) | 🟢 H4 ✅ |
| Android | [`android/`](android/) | [`add-platform-android/`](../../../../docs/spec/archive/2026-05-12-add-platform-android/) | 🟢 H4 ✅ |
| WASM | [`wasm/`](wasm/) | [`add-platform-wasm/`](../../../../docs/spec/archive/2026-05-12-add-platform-wasm/) | 🟢 H4 ✅ |

---

## 错误码 → 平台异常映射

各平台 facade 把 [`Z42HostStatus`](../../../runtime/include/z42_host.h) 翻译成本平台原生错误形式，但**保留 status code + message**，便于宿主诊断：

| Z42HostStatus | iOS `Z42VMError` | Android `Z42VMException` | WASM `Z42VMError` |
|---------------|------------------|--------------------------|-------------------|
| `OK` | — | — | — |
| `ALREADY_INIT` | `.alreadyInit(message)` | code=1 | `name: "AlreadyInit"` |
| `NOT_INIT` | `.notInit(message)` | code=2 | `name: "NotInit"` |
| `BAD_CONFIG` | `.badConfig(message)` | code=3 | `name: "BadConfig"` |
| `FEATURE_OFF` | `.featureOff(message)` | code=4 | `name: "FeatureOff"` |
| `BAD_ZBC` | `.badZbc(message)` | code=10 | `name: "BadZbc"` |
| `VERIFICATION` | `.verification(message)` | code=11 | `name: "Verification"` |
| `ENTRY_NOT_FOUND` | `.entryNotFound(message)` | code=20 | `name: "EntryNotFound"` |
| `ARG_MISMATCH` | `.argMismatch(message)` | code=21 | `name: "ArgMismatch"` |
| `VM_EXCEPTION` | `.vmException(message)` | code=30 | `name: "VMException"` |
| `INTERNAL` | `.internal(message)` | code=99 | `name: "Internal"` |

实施细节见 [embedding.md §10 错误处理](../../../../docs/internals/src/runtime/embedding.md)。

---

## 关键设计原则

1. **三平台共享 Tier 2 `z42-host`** —— 任何"VM 启动 / 加载 / 调用"逻辑只能在 `z42-host` 中存在一份；platforms/ 只做语言绑定 + 资源 bundle
2. **不分叉 ABI** —— 三平台用相同 C ABI；hook 机制（ZpkgResolver）统一了"如何找 zpkg 字节"，没有 platform-specific 函数
3. **类名全平台 `Z42VM`** —— 跨平台文档 / 教学 / 错误信息可直接复用
4. **bundle by default** —— 三平台都假定 stdlib zpkg 与 app 一起 ship；网络下载 / 增量更新留 1.x+
5. **同步 invoke** —— v0.1 不引入 async；UI 线程切换由宿主负责
6. **panic 隔离** —— 任何 z42 异常 / Rust panic 不跨 FFI；统一翻为状态码 + message

---

## 不在本文档范围

- 各平台 build / CI 细节 → 各平台 spec
- xcframework / AAR / npm 发布流程 → 各平台 spec
- Demo app 设计 → 各平台 spec
- 测试体系（XCTest / JUnit / playwright）→ 各平台 spec
- 桌面 hello_c desktop build + R1–R7 端到端 → 已落地为 `desktop` 平台后端（`./xtask test platform desktop`；R1–R7 共用夹具在 `src/toolchain/workload/fixtures/`）
