# Z42VM — Android facade

> 🟢 H4 落地（2026-05-12）。
>
> Spec：[`docs/spec/archive/2026-05-12-add-platform-android/`](../../../../docs/spec/archive/2026-05-12-add-platform-android/)
> 跨平台契约：[`../README.md`](../README.md)
> 实现原理：[`docs/internals/src/runtime/embedding.md`](../../../../docs/internals/src/runtime/embedding.md)
> 构建工作流：[`docs/internals/src/devinfra/build-platforms.md`](../../../../docs/internals/src/devinfra/build-platforms.md)

把 z42 VM 编进 Gradle AAR 模块，Kotlin / Compose app 引入后一行 `import io.z42.vm.Z42VM` 跑 `.zbc`。

## Quick Start

详细 step-by-step 见 [`docs/internals/src/devinfra/build-platforms.md`](../../../../docs/internals/src/devinfra/build-platforms.md)。最简略：

```bash
# 一次性：SDK + NDK + emulator + AVD + Gradle 全装到 artifacts/tools/
# 不动系统（~4 GB；详见 z42 xtask.zpkg deps install android-sdk）
z42 xtask.zpkg deps install android-sdk
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked
dotnet build src/compiler/z42.slnx                         # 编 stdlib

# 每次（export 一次后下次重用）
export ANDROID_HOME="$PWD/artifacts/tools/android-sdk"
export ANDROID_NDK_HOME="$PWD/artifacts/tools/android-ndk"
export GRADLE_USER_HOME="$PWD/artifacts/tools/gradle-user-home"
export JAVA_HOME=$(/usr/libexec/java_home -v 17+)
./xtask test platform android build      # cargo-ndk × ABIs + gradle AAR
./xtask test platform android assets     # fixtures + stdlib 进 assets
```

产物：`z42vm/build/outputs/aar/z42vm-release.aar` + `jniLibs/{arm64-v8a,x86_64}/libz42_platform_android.so` + `assets/stdlib/*.zpkg`（无 index——`AssetZpkgResolver` 读各 zpkg 的 NSPC）+ `androidTest/assets/test-fixtures/*.zbc`（32-bit ABI 已退场；见 memory project_supported_platforms）。

## Run tests

```bash
./xtask test platform android
```

全流程(build + assets + run)。③ run 由 `AndroidBackend.RunTests` 桥接 `test.sh`：自启 headless emulator `@z42_pixel6_api37`（installer 预创建的 AVD）+ adb 等 boot 完成 + 跑 `./gradlew :z42vm:connectedAndroidTest`，退出时 `adb emu kill`。期望尾部：

```
Starting 7 tests on z42_pixel6_api37(AVD) - 17
Finished 7 tests on z42_pixel6_api37(AVD) - 17
BUILD SUCCESSFUL
✅ Z42VMInstrumentedTest passed
```

7 个测试覆盖 [`platform-test-contract`](../../../../docs/spec/archive/2026-05-12-define-platform-test-contract/) R1–R7（smoke / 错误码 / resolver / lifecycle / 多行 stdout），与 iOS XCTest / wasm playwright 对齐。

## API 速记

```kotlin
import io.z42.vm.Z42VM
import io.z42.vm.AssetZpkgResolver

Z42VM(zpkgResolver = AssetZpkgResolver(assets)).use { vm ->
    vm.stdoutHandler = { bytes -> textView.append(String(bytes)) }
    val m = vm.loadZbc(assets.open("hello.zbc").readBytes())
    val e = vm.resolveEntry(m, "App.Main")
    vm.invoke(e)
}
```

### `Z42VM(zpkgResolver, stdoutHandler?, stderrHandler?)`

- `zpkgResolver: ZpkgResolver` —— 默认 `AssetZpkgResolver(context.assets)` 读 `assets/stdlib/<ns>.zpkg`
- `stdoutHandler / stderrHandler: ((ByteArray) -> Unit)?` —— 每条 z42 输出触发一次，UTF-8 字节

### `Z42VMValue`

```kotlin
sealed class Z42VMValue {
    object Null : Z42VMValue()
    data class I64(val v: Long)    : Z42VMValue()
    data class F64(val v: Double)  : Z42VMValue()
    data class Bool(val v: Boolean): Z42VMValue()
}
```

H2 marshal 限 null + 三种原语；string / object / Array 推迟。

### `Z42VMException`

`RuntimeException` + `val status: Int` (1..99) + 标准 status 常量。映射详见 [`platforms/README.md`](../README.md) §错误码映射表。

### `ZpkgResolver` 接口

```kotlin
interface ZpkgResolver {
    fun resolve(namespace: String): ByteArray?
}
```

内置：

- `AssetZpkgResolver(assets, subdir = "stdlib")` —— 读 AAR `assets/stdlib/<ns>.zpkg`
- `MapZpkgResolver(initial = emptyMap())` —— 测试 / 自定义来源

## 架构

```
io.z42.vm.Z42VM  (Kotlin / public API)
        │
        ▼ JNI external fun nativeInitialize / nativeLoadZbc / ...
        │
libz42vm_jni.so  (C, CMake-built; src/main/cpp/z42vm_jni.c)
        │
        ▼ z42_host_*  (C ABI from z42_host.h)
        │
libz42_platform_android.so  (cargo-ndk-built; thin re-export of z42_host_*)
        │
        ▼  in-process
src/runtime/  (interp + aot feature; no JIT inside Android sandbox)
```

`libz42vm_jni.so` 和 `libz42_platform_android.so` 都打进 AAR 的 `jniLibs/<abi>/`，每 ABI 一份。

## 限制（v0.1）

- **仅 interp 模式**：JIT 与 Android ART 互斥
- ~~**无 `native-interop`**~~ → **已启用**：libffi 5.1 / libffi-sys 4.1 的 bundled libffi 3.4.7 修复了旧 2.3 与 NDK 工具链不兼容的 CFI advance_loc 问题；`android` feature preset 现含 `native-interop`（首次 cross-compile 时通过 `cargo ndk` + NDK r25+ 验证；构建经 `AndroidBackend.BuildProject`）
- **单实例**
- **同步 invoke**：UI 上请用 `Dispatchers.Default` 异步包装
- **Demo / CI**：推迟到独立 spec（`add-android-demo` / `-ci`）；JUnit instrumented test 已在 `add-android-tests` (2026-05-12) 落地，跑 `./test.sh` 即可

## 故障排查

详细的 step-by-step 故障兜底见 [`docs/internals/src/devinfra/build-platforms.md`](../../../../docs/internals/src/devinfra/build-platforms.md) §Step 各栏的 ❗ 行。

## 与跨平台契约的对齐

类名 `Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMException`、`ZpkgResolver` 接口、错误码 → status 数值映射，全部与 [`platforms/README.md`](../README.md) 一致。同一份 `.zbc` 在 iOS / Android / WASM 三平台行为应等价。
