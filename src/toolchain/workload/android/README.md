# workload/android — Z42VM Android facade

## 职责

把 z42 VM 编进 Gradle AAR 模块（`io.z42.vm.Z42VM`），Kotlin / Compose app 引入后跑 `.zbc`；
并含 Android 平台 workload 的 appbuilder（`export`）与 R1–R7 嵌入契约测试。仅 interp，JIT 与 ART 互斥。
不做：编译 `.z42`（host 端 z42c 编好 `.zbc` / `.zpkg` 再装进 assets）。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| Kotlin 公开 API（`Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMException`） | `platform/z42vm/src/main/java/io/z42/vm/` |
| zpkg 解析（`AssetZpkgResolver` / `MapZpkgResolver`） | `ZpkgResolver.kt` |
| JNI 桥 → `z42_host_*` C ABI | `platform/z42vm/src/main/cpp/z42vm_jni.c` |
| Rust cdylib（`libz42_platform_android.so`，`z42_host_*` 再导出） | `platform/rust/` |
| 发布 / 导出管线（`: WorkloadBase`） | `appbuilder/AndroidWorkload.z42`、`appbuilder/export.z42` |
| 设备侧测试宿主 | `Z42TestHost.kt`（`main`）、`Z42VMInstrumentedTest.kt` / `Z42EmbeddedInstrumentedTest.kt`（`androidTest`） |

## 基础用法

```kotlin
import io.z42.vm.Z42VM
import io.z42.vm.AssetZpkgResolver

Z42VM(zpkgResolver = AssetZpkgResolver(assets)).use { vm ->
    vm.stdoutHandler = { bytes -> textView.append(String(bytes)) }
    val m = vm.loadZbc(assets.open("hello.zbc").readBytes())
    vm.invoke(vm.resolveEntry(m, "App.Main"))
}
```

构建与安装：

```bash
./xtask deps install --os android        # SDK + NDK + emulator + AVD + Gradle，装到 artifacts/tools/
./xtask build stdlib
./xtask test platform android build      # cargo-ndk × ABIs + gradle AAR
./xtask test platform android assets     # fixtures + stdlib 进 assets
```

产物在宿主工程副本 `artifacts/build/toolchain/workload/android/tests/host/`（不写本目录）：
`z42vm/build/outputs/aar/z42vm-release.aar`、`jniLibs/{arm64-v8a,x86_64}/libz42_platform_android.so`、
`assets/stdlib/*.zpkg`（`AssetZpkgResolver` 读各 zpkg 的 NSPC 建索引）。

限制：仅 interp；单实例；同步 invoke（UI 线程请用 `Dispatchers.Default` 包装）；
marshal 仅 null + `I64` / `F64` / `Bool`（string / object / Array 见 embedding.md 的 Deferred）。

## 如何测试验证

```bash
./xtask test app android [--filter <pat>]
```

一条命令跑完整条流水线（与 CI 相同）：test agent → .so + AAR + assets 进宿主副本 → 嵌入 bundle →
z42b 跑一次 `./gradlew :z42vm:connectedAndroidTest`（R1–R7 + 嵌入语料）。已有接着的设备 / 在跑的模拟器就复用
（CI 的 emulator-runner 就是这样）；没有就以 headless 方式启动 `@z42_pixel6_api37`（模拟器组件缺了先自动装，
约 4GB），等 boot 完成再跑，跑完 `adb emu kill`。期望尾部 `Finished 7 tests` + `BUILD SUCCESSFUL`
（7 个测试 = R1–R7，与 iOS XCTest / wasm playwright 对齐）。

## 关联文档

- 跨平台契约：[`../platform-contract.md`](../platform-contract.md)
- 嵌入机制：[embedding.md](../../../../docs/internals/src/runtime/embedding.md)
- 构建与故障排查：[build-platforms.md](../../../../docs/internals/src/devinfra/build-platforms.md)

## 核心文件

| 路径 | 职责 |
|------|------|
| `appbuilder/` | workload handler + export |
| `template/` | `export` 渲染进用户工程的脚手架 |
| `platform/z42vm/` | AAR 模块（Kotlin API + JNI C + androidTest） |
| `platform/rust/` | cargo-ndk 构建的 cdylib |
