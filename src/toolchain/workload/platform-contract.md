# 平台嵌入契约

Runtime 提供共享的宿主语义和 [C SDK](../../runtime/include/README.md)。各 workload 提供绑定、资源装配和契约测试。

## 构建与绑定

z42c 在构建宿主上生成 `.zbc` 或 `.zpkg`，各平台宿主加载这些产物。各 workload 负责按目标导出工程和部署 Runtime。 [嵌入架构](../../../docs/internals/src/runtime/embedding.md)

| 平台 | 宿主路径 |
|---|---|
| [桌面](desktop/README.md) | C 宿主从 `z42.h` 使用 Runtime |
| [iOS](ios/README.md) | Swift 门面经 `Z42VMC` 模块调用 C ABI |
| [Android](android/README.md) | Kotlin 门面经 JNI 桥调用 C ABI |
| [Wasm](wasm/README.md) | JS 门面经 wasm-bindgen 调用 Runtime 的 Rust 宿主接口 |

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
| 启动文件读取协议 | `protocol FileResolver` | `interface FileResolver` | `type FileResolver = (name: string, kind: FileKind) => Uint8Array \| null` |

### 同形 API（最小 surface）

```
// 构造（接受可选 FileResolver；默认平台-bundle 实现）
init(fileResolver: FileResolver = <platform default>) throws

// 加载程序字节及其可选逻辑名称
loadBytes(bytes: BytesType, fileName: String? = nil) -> Z42VMModule

// 按 FQN 解析入口
resolveEntry(_ m: Z42VMModule, fqn: String) -> Z42VMEntry

// 同步调用（值类型见各绑定的接口文档）
invoke(_ e: Z42VMEntry, args: [Z42VMValue] = []) throws -> Z42VMValue

// stdout / stderr 回调（每条 z42 输出触发一次）
setStdoutHandler(handler: (BytesType) -> Void)
setStderrHandler(handler: (BytesType) -> Void)

// 资源释放（iOS/Swift deinit 自动；Kotlin AutoCloseable.close；JS dispose）
```

完整方法签名、错误码映射、`Z42VMValue` 构造器等具体形态由各平台 spec 锁定（链接见下方）。

---

## FileResolver 协议

resolver 按逻辑文件名读取包、字节码和启动侧车。iOS 与 Android 使用 C writer 同步提交字节，Wasm 将 JS
返回的 Uint8Array 复制给 Rust 文件 resolver。 [C resolver 契约](../../../docs/reference/src/embedding/c-abi.md#文件-resolver-回调)

| 平台 | 默认文件来源 |
|------|--------------|
| iOS | `BundleZpkgResolver` 从 Bundle 读取 |
| Android | `AssetZpkgResolver` 从 AssetManager 读取 |
| Wasm | `bundleStdlibNode` 按名读取，`bundleStdlibBrowser` 按部署清单预取 |

## 命名约定

| 项 | 规则 |
|----|------|
| **类名** | `Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMError`、`Z42VMException` —— `VM` 全大写跨语言一致 |
| **SwiftPM 包名** | `Z42VM`（Package.swift target name） |
| **AAR groupId / artifactId** | `io.z42:z42vm`（lowercase per Maven convention，类名仍 `Z42VM`） |
| **npm 包名** | `@z42/wasm`（scope `@z42`，包 `wasm`） |
| **Kotlin package** | `io.z42.vm` |
| **Swift module** | `Z42VM` |

---

## 平台能力与验证

各 workload README 记录入口、平台依赖、可选组件和验证命令。R1–R7 端到端契约共用 `fixtures/` 下的程序，各平台用各自的绑定运行。

---

## 错误码 → 平台异常映射

各平台 facade 将 [`Z42HostStatus`](../../runtime/include/z42_host.h) 映射为本平台的错误形式，保留 status code 与 message 供宿主诊断。

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
| `FATAL` | `.fatal(message)` | code=31 | `name: "Fatal"` |
| `INTERNAL` | `.internal(message)` | code=99 | `name: "Internal"` |

实施细节见 [embedding.md §10 错误处理](../../../docs/internals/src/runtime/embedding.md)。

---

## 关键设计原则

- Runtime 统一实现宿主生命周期、装载、调用和错误语义。平台绑定负责语言转换和资源适配。
- C 消费者引用 Runtime 发布的同一组头文件。可选能力及其装配方式记录在各 workload 中。
- `invoke` 同步执行，UI 线程切换由宿主负责。平台绑定将 z42 异常和运行时错误映射为本语言的错误类型。
