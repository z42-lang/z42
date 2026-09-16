# Z42VM — iOS facade

> 🟢 H4 落地（2026-05-12）。
>
> Spec：[`docs/spec/archive/2026-05-12-add-platform-ios/`](../../../../docs/spec/archive/2026-05-12-add-platform-ios/)
> 跨平台契约：[`../README.md`](../README.md)
> 实现原理：[`docs/internals/src/runtime/embedding.md`](../../../../docs/internals/src/runtime/embedding.md)
> 构建工作流：[`docs/workflow/building/ios.md`](../../../../docs/workflow/building/ios.md)

把 z42 VM 编进 SwiftPM 包 + xcframework，让 Swift / SwiftUI iOS app 一行 `import Z42VM` 跑 `.zbc`。

## Quick Start

```bash
# 1. 一次性 toolchain
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin

# 2. 编 compiler + stdlib
dotnet build src/compiler/z42.slnx
./xtask build stdlib

# 3. 编 iOS facade（含 macOS arm64 slice）+ test 资产
./xtask test platform ios build
./xtask test platform ios assets
```

产物：`Z42VM.xcframework/` (ios-arm64 + ios-arm64_x86_64-simulator + macos-arm64) + `Resources/stdlib/*.zpkg`（无 index——`BundleZpkgResolver` 读各 zpkg 的 NSPC）。详见 [`docs/workflow/building/ios.md`](../../../../docs/workflow/building/ios.md)。

## Run tests

`test platform ios` 全流程：build xcframework + 编 fixture 进 `Tests/Z42VMTests/Resources/` + 在 **iOS Simulator** 上 `xcodebuild test` 跑 7 个 XCTest（R1–R7），产 `artifacts/test-reports/ios/junit.xml`：

```bash
./xtask test platform ios
```

测试覆盖 `add-ios-tests` spec 落地的 R1–R7 facade 契约（smoke / 错误码映射 / resolver / lifecycle / 多行 stdout），见 [`docs/spec/archive/2026-05-12-add-ios-tests/`](../../../../docs/spec/archive/2026-05-12-add-ios-tests/)。

## API 速记

```swift
import Z42VM

let vm = try Z42VM(
    zpkgResolver: BundleZpkgResolver(),
    stdoutHandler: { bytes in
        textArea.append(String(decoding: bytes, as: UTF8.self))
    }
)
let module = try vm.loadZbc(Data(contentsOf: zbcURL))
let entry  = try vm.resolveEntry(module, fqn: "App.Main")
_ = try vm.invoke(entry)
// vm.deinit 自动调 z42_host_shutdown
```

### `Z42VM.init(zpkgResolver:stdoutHandler:stderrHandler:)`

- `zpkgResolver: ZpkgResolver` — 默认 `BundleZpkgResolver()`（读 `Bundle.main/stdlib/<ns>.zpkg`）
- `stdoutHandler / stderrHandler: ((Data) -> Void)?` — 每条 z42 输出触发一次，UTF-8 字节

### `Z42VMValue`

```swift
public enum Z42VMValue: Equatable {
    case null
    case i64(Int64)
    case f64(Double)
    case bool(Bool)
}
```

v0.1 marshal 仅支持 null + 三种原语；string / object / Array 推迟到后续 spec（[`embedding.md §12 Deferred`](../../../../docs/internals/src/runtime/embedding.md)）。

### `Z42VMError`

`enum` 含 10 个 case 对应 `Z42HostStatus`：`alreadyInit` / `notInit` / `badConfig` / `featureOff` / `badZbc` / `verification` / `entryNotFound` / `argMismatch` / `vmException` / `internal`。每个携带 message + 数值 `status: Int32`。

### `ZpkgResolver` 协议

```swift
public protocol ZpkgResolver {
    func resolve(namespace: String) -> Data?
}
```

内置实现：

- `BundleZpkgResolver(bundle:subdirectory:)` —— 默认从 `Bundle.main/stdlib/<ns>.zpkg` 加载
- `MapZpkgResolver([:])` —— `[String: Data]` 测试用 / 自定义来源

## 限制（v0.1）

- **仅 interp 模式**：App Store 政策禁动态代码生成；JIT 不可用，AOT 占位
- ~~**无 native interop**~~ → **已启用**：libffi 5.1 / libffi-sys 4.1 的 bundled 汇编（libffi 3.4.7）修复了旧 2.3 在 iOS arm64 上的 CFI advance_loc 不兼容；`ios` feature preset 现含 `native-interop`
- **单实例**：与其他平台一致；一个进程一个 `Z42VM`
- **同步 invoke**：长任务阻塞调用线程；UI 上请用 `DispatchQueue.global().async`
- **Demo / CI**：推迟到独立 spec（`add-platform-ios-demo` / `-ci`）；XCTest 已在 `add-ios-tests` (2026-05-12) 落地，跑 `swift test` 即可

## 错误码映射

详见 [`platforms/README.md`](../README.md) §错误码映射表。

## 故障排查

| 现象 | 处理 |
|------|------|
| `linker not found for aarch64-apple-ios` | `rustup target add aarch64-apple-ios` |
| `Z42VMError(20): function ... not found` | `.zbc` 与 stdlib 的命名空间不一致；检查 FQN 拼写 |
| `dyld: Library not loaded` | xcframework slice 选错；真机 vs simulator |
| stdoutHandler 没触发 | z42 代码用了非 corelib I/O；或 sink 在异步线程被 race；切回同一线程 retry |

## 与跨平台契约的对齐

类名、API 形态、错误码与 [`platforms/README.md`](../README.md) 一致。同一份 `.zbc` 在 iOS / Android / WASM 三平台行为应等价（marshal 类型限制相同）。
