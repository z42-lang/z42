# workload/ios — Z42VM iOS facade

## 职责

提供 SwiftPM 宿主接口（`import Z42VM`），Swift / SwiftUI iOS app 跑 `.zbc`。运行时按 RID 发布 libz42.a 与 z42.framework；SwiftPM 所需的静态适配封装在本地生成，不在 runtime 包重复分发；
并含 iOS 平台 workload 的 appbuilder（`export`）与 R1–R7 嵌入契约测试。默认 interp；模拟器可显式选择 JIT，真机不包含 JIT 实现。
不做：编译 `.z42`（host 端 z42c 预编）。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| Swift 公开 API（`Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMError`） | `platform/Sources/Z42VM/` |
| 启动文件读取（`BundleZpkgResolver` / `MapZpkgResolver`） | `platform/Sources/Z42VM/ZpkgResolver.swift` |
| C 模块 `Z42VMC` | [模块描述](module.modulemap) 和 `z42.h` 随 iOS Runtime 发布在 `native/include/`。workload 安装时从该目录组装 SwiftPM 的 XCFramework |
| Rust staticlib（`z42_host_*` 再导出） | `src/runtime/examples/z42.rs` |
| 发布 / 导出管线（`: WorkloadBase`） | `appbuilder/iOSWorkload.z42`、`appbuilder/export.z42` |
| 设备侧测试宿主 | `Z42TestHost.swift`；XCTest 在 `platform/Tests/Z42VMTests/` |

## 基础用法

```swift
import Z42VM

let vm = try Z42VM(fileResolver: BundleZpkgResolver(),
                   stdoutHandler: { bytes in textArea.append(String(decoding: bytes, as: UTF8.self)) })
let module = try vm.loadZbc(Data(contentsOf: zbcURL))
let entry  = try vm.resolveEntry(module, fqn: "App.Main")
_ = try vm.invoke(entry)
```

构建：

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
./xtask build stdlib
./xtask test app ios build          # xcframework（ios-arm64 + simulator + macos-arm64 slice）
./xtask test app ios assets         # fixtures + stdlib
```

产物在宿主工程副本 `artifacts/intermediate/toolchain/workload/ios/host/`（不写本目录）：
`Z42VM.xcframework/`、`Resources/stdlib/*.zpkg`。

模拟器选择 JIT：`Z42VM(..., execMode: .jit)`；运行时提供独立 `libz42_jit.a` / `z42_jit.framework`。真机请求 JIT 明确报错。

## Runtime 组装

iOS Runtime 包提供静态归档、动态 Framework 和公开 C 头。`module.modulemap` 随头文件发布，workload 安装器使用静态归档和发布头生成 SwiftPM 的 XCFramework。动态宿主将同包的核心、解释器、compression 和共享 Rust std 随应用嵌入并签名。

静态 Swift 宿主通过统一 C 头中的 `z42_native_register_static` 登记 compression 提供方，检查 `Z42_NATIVE_OK`；该注册接口属于 loader，不使用 host 会话的 last-error。登记本身不调用解析器，脚本首次使用时才解析库符号。

导出工程嵌入同批 interp/JIT、profiler、compression 与共享 std Framework。应用只链接 `z42.framework`；profiler 由核心在首次分析请求时加载。

限制：单实例；同步 invoke；marshal 仅 null + `i64` / `f64` / `bool`。

## 如何测试验证

```bash
./xtask test app ios
```

build xcframework + 编 fixture + 在 iOS Simulator 上 `xcodebuild test` 跑 R1–R7（7 个 XCTest），
JUnit → `artifacts/reports/tests/ios/junit.xml`。

`./xtask package runtime --rid iossim-arm64 --verify` 验证发布归档与 Framework 的实际 C 宿主，并编译导出的 arm64 模拟器 Xcode 应用，检查组件随应用嵌入且 profiler 没有被启动链接。

## 关联文档

- 跨平台契约：[`../platform-contract.md`](../platform-contract.md)（含错误码映射）
- 嵌入机制：[embedding.md](../../../../docs/internals/src/runtime/embedding.md)
- 构建与故障排查：[build-platforms.md](../../../../docs/internals/src/devinfra/build-platforms.md)

## 核心文件

| 路径 | 职责 |
|------|------|
| `appbuilder/` | workload handler + export |
| `template/` | `export` 渲染进用户工程的脚手架 |
| `platform/Package.swift` | SwiftPM 清单 |
| `platform/Sources/` | Swift facade |
| `src/runtime/examples/z42.rs` | staticlib crate |
| `platform/Tests/` | XCTest（R1–R7 + 嵌入语料） |
