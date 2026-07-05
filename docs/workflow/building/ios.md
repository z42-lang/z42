# iOS facade — 嵌入 z42 到 iOS

> 🟢 已落地 · facade [`ios/platform/`](../../../src/toolchain/workload/ios/platform/) · spec [`2026-05-12-add-platform-ios/`](../../spec/archive/2026-05-12-add-platform-ios/)

把 z42 VM 编进 `Z42VM.xcframework`，让 Swift / SwiftUI app `import Z42VM` 跑 `.zbc`。iOS 只能在 **macOS 主机**上编。统一三段：**① Host 环境准备 → ② 编译（facade + 嵌入 app）→ ③ 运行测试用例**。

## 1. Host 环境准备

### 1.1 平台工具链（一次性，仅 macOS host）

```bash
# Xcode（一次性大下载；如已装跳过）
# 从 App Store 装最新 Xcode，启动一次接受 license
sudo xcodebuild -license accept
xcode-select --install
xcode-select -p     # 应输出 .../Xcode.app/Contents/Developer

# Rust iOS targets
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

❗ `xcrun: command not found` → Xcode 未装或 `xcode-select -s /Applications/Xcode.app/Contents/Developer` 没指对。

### 1.2 z42 工具链（编译器 + stdlib，一次性 / 改 stdlib 后重跑）

facade 会把 stdlib zpkg 打进 `Resources/`，故先备好 z42 工具链：

```bash
./xtask build compiler      # z42c 自举（或由 ./scripts/install-z42.sh 直接提供）
./xtask build stdlib
```

✅ 产出 `artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg` + stdlib zpkg 到 `artifacts/build/libraries/dist/release/*.zpkg`。
❗ `error: z42c not built` → 先 `./scripts/install-z42.sh` 或 `./xtask build compiler`。

## 2. 编译

### 2.1 编 facade

```bash
./xtask test platform ios build
```

`test platform ios build`（IosBackend）内部串接：cargo build × 3 target + `xcodebuild -create-xcframework`（含 ios-device/sim/macos slice）。

✅ 产物：`Z42VM.xcframework/` 含 `ios-arm64/` + `ios-arm64_x86_64-simulator/`；`Resources/stdlib/*.zpkg`（22 个，从 `artifacts/build/libraries/dist/release/` 拷入）。

❗ `linker not found for aarch64-apple-ios` → 1.1 rustup target 漏装。
❗ `libffi-sys` cross-compile 失败 → 检查 `runtime/Cargo.toml` 的 `libffi` 是否为 5.1+（旧 3.2/libffi-sys 2.3 的 bundled `sysv.S` 在 iOS arm64 触发 CFI advance_loc 错误）；当前默认已是 bundled 模式，无需手工切换。

### 2.2 嵌入到 app

`Package.swift`：

```swift
.package(path: "/abs/path/to/z42/src/toolchain/workload/ios/platform"),
// 或 release：
// .package(url: "https://github.com/codesigner-ui/z42-ios.git", from: "0.1.0"),
```

Target deps：

```swift
.product(name: "Z42VM", package: "Z42VM"),
```

Swift 用法：

```swift
import Z42VM

let vm = try Z42VM(zpkgResolver: BundleZpkgResolver())
vm.stdoutHandler = { bytes in textArea.append(String(decoding: bytes, as: UTF8.self)) }

let m = try vm.loadZbc(Data(contentsOf: zbcURL))
let e = try vm.resolveEntry(m, fqn: "App.Main")
_ = try vm.invoke(e)
```

❗ `dyld: Library not loaded` → xcframework slice 选错（真机用了 simulator slice 或反之）。

## 3. 运行测试用例

R1–R7 嵌入契约测试，跑在**真 iOS Simulator** 上（`xcodebuild test`）：

```bash
./xtask test platform ios
# ① cargo×targets + xcframework（含 ios-device/sim/macos slice）
# → ② fixtures+stdlib 进 Tests bundle
# → ③ xcodebuild test 在 iOS Simulator 跑 R1–R7（IosBackend 自动选 simctl 第一个可用 iPhone）
```

指定模拟器：`Z42_IOS_DEST='id=<udid>'`（或 `platform=iOS Simulator,name=...`）覆盖默认。完整三阶段见 [`../testing/platform-tests.md`](../testing/platform-tests.md)。

> 🚧 **占位**：app 级 demo（独立可跑的示例 app）+ XCTest 报告 + CI 接线推迟到独立 spec（`add-platform-ios-demo` / `-tests` / `-ci`），后续补。

## See also

- **本地打 per-slice SDK package**（自包含 `Package.swift` + `Z42VM.xcframework`）：[`../packaging.md`](../packaging.md) — `./xtask package release --rid ios-arm64 / iossim-arm64`
- Swift API + 错误码（spec 落地后补）：`ios/README.md`
- 跨平台契约：[`platform-contract.md`](../../../src/toolchain/workload/platform-contract.md)
- 设计 + 决策：[spec](../../spec/archive/2026-05-12-add-platform-ios/)
