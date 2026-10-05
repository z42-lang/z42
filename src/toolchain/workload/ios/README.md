# workload/ios — Z42VM iOS facade

## 职责

把 z42 VM 编进 SwiftPM 包 + xcframework（`import Z42VM`），Swift / SwiftUI iOS app 跑 `.zbc`；
并含 iOS 平台 workload 的 appbuilder（`export`）与 R1–R7 嵌入契约测试。仅 interp（App Store 禁动态代码生成）。
不做：编译 `.z42`（host 端 z42c 预编）。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| Swift 公开 API（`Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMError`） | `platform/Sources/Z42VM/` |
| zpkg 解析（`BundleZpkgResolver` / `MapZpkgResolver`） | `platform/Sources/Z42VM/ZpkgResolver.swift` |
| C 桥头（`z42_host.h`） | `platform/Sources/Z42VMC/` |
| Rust staticlib（`z42_host_*` 再导出） | `platform/rust/` |
| 发布 / 导出管线（`: WorkloadBase`） | `appbuilder/iOSWorkload.z42`、`appbuilder/export.z42` |
| 设备侧测试宿主 | `Z42TestHost.swift`；XCTest 在 `platform/Tests/Z42VMTests/` |

## 基础用法

```swift
import Z42VM

let vm = try Z42VM(zpkgResolver: BundleZpkgResolver(),
                   stdoutHandler: { bytes in textArea.append(String(decoding: bytes, as: UTF8.self)) })
let module = try vm.loadZbc(Data(contentsOf: zbcURL))
let entry  = try vm.resolveEntry(module, fqn: "App.Main")
_ = try vm.invoke(entry)
```

构建：

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
./xtask build stdlib
./xtask test platform ios build          # xcframework（ios-arm64 + simulator + macos-arm64 slice）
./xtask test platform ios assets         # fixtures + stdlib
```

产物在宿主工程副本 `artifacts/build/toolchain/workload/ios/tests/host/`（不写本目录）：
`Z42VM.xcframework/`、`Resources/stdlib/*.zpkg`。

限制：仅 interp；单实例；同步 invoke；marshal 仅 null + `i64` / `f64` / `bool`。

## 如何测试验证

```bash
./xtask test platform ios
```

build xcframework + 编 fixture + 在 iOS Simulator 上 `xcodebuild test` 跑 R1–R7（7 个 XCTest），
JUnit → `artifacts/reports/tests/ios/junit.xml`。

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
| `platform/Sources/` | Swift facade + C 桥 |
| `platform/rust/` | staticlib crate |
| `platform/Tests/` | XCTest（R1–R7 + 嵌入语料） |
