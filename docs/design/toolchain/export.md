# z42 export — 原生平台工程生成器

> 实现：`src/toolchain/launcher/core/launcher_export*.z42`（add-export-command, 2026-06-14）

## 概述

`z42 export <platform> <project.z42.toml>` 从一个 z42 项目描述文件生成对应平台的原生工程骨架，供开发者在 Xcode / Android Studio / 浏览器中进一步配置和发布。

支持的平台：
| 平台 | 命令 | 输出 |
|------|------|------|
| iOS / iPadOS | `z42 export ios` | 裸 `.xcodeproj` + Swift AppDelegate |
| Android | `z42 export android` | Kotlin + Gradle 工程 |
| WebAssembly | `z42 export wasm` | `index.html` + `index.js` |

> **desktop 不在 export 范畴**（rework-desktop-publish-run, 2026-06-17）：desktop 没有 IDE 工程可生成，它的发布产物 apphost 是 **publish** 形态——`z42 publish` / `z42 run desktop`（读 `[platform.desktop]`）。命令模型见 [platform-export-lifecycle.md](platform-export-lifecycle.md)。

## 工作流

```
z42c build <project.z42.toml>         # 编译 → app.zpkg
z42 install 0.3.0 --rid ios-arm64    # 下载 iOS 平台 SDK（一次性）
z42 export ios <project.z42.toml>    # 生成 Xcode 工程
```

## 配置来源

配置优先级（高 → 低）：CLI 标志 > `[platform.<plat>]` toml 段 > 内置默认值。

toml 示例：

```toml
[platform.ios]
bundle_id    = "com.example.myapp"
display_name = "My App"
min_ios      = "15.0"

[platform.android]
app_id       = "com.example.myapp"
version_code = 1

[platform.wasm]
title = "My App"
```

完整 toml 键表见 [`docs/design/compiler/project.md`](../compiler/project.md)（`[platform.*]` 段）。

## 平台 SDK 路径

平台 SDK 安装在 `~/.z42/runtimes/<rid>/<ver>/`，与桌面运行时共用 `runtimes/` 根，用 RID 子目录区分：

| RID | 用途 |
|-----|------|
| `ios-arm64` | iOS 真机 SDK（z42vm.xcframework） |
| `iossim-arm64` | iOS 模拟器 SDK |
| `android-arm64` | Android ARM64 SDK（libz42vm.so） |
| `browser-wasm` | WASM SDK（z42vm.js + z42vm.wasm） |

安装命令：
```
z42 install <ver> --rid ios-arm64
z42 install <ver> --rid android-arm64
z42 install <ver> --rid browser-wasm
```

`--rid` 标志在 `add-export-command` 中追加到 `z42 install`。当指定的 RID 与宿主 RID 不同时，产物安装到 `runtimes/<rid>/<ver>/`（平台 SDK 布局）；宿主 RID 沿用 `runtimes/<ver>/`（兼容现有 `z42 run` 逻辑）。

## 生成产物

### iOS（`z42 export ios`）

```
<output>/
  <Name>.xcodeproj/
    project.pbxproj            ← 完整 Xcode 工程配置（固定确定性 UUID）
  <Name>/
    main.swift                 ← UIApplicationDelegate 骨架
    Info.plist                 ← CFBundleIdentifier + display name
    Assets.xcassets/           ← 空资产目录
    app.zpkg                   ← 从 dist_dir 拷贝（若已构建）
  z42vm.xcframework/           ← 需手动从平台 SDK 拷贝
```

`project.pbxproj` 使用**固定的 24 字符十六进制对象 ID**（非随机），保证重复生成字节相同。单目标（iOS app），引用 `main.swift`（Sources）、`app.zpkg`（Resources）、`z42vm.xcframework`（Frameworks）。

`main.swift` 是 AppDelegate 骨架，留 `// TODO: call z42_run_app(zpkgPath)` 占位；当 z42vm C ABI 稳定后填充。

### Android（`z42 export android`）

```
<output>/
  build.gradle                 ← 根 Gradle 脚本（plugin 声明）
  settings.gradle              ← rootProject.name + include ':app'
  gradle/wrapper/
    gradle-wrapper.properties  ← Gradle 8.4 + GRADLE_USER_HOME
  app/
    build.gradle               ← compileSdk / minSdk / versionCode / jniLibs
    src/main/
      AndroidManifest.xml      ← <activity> + MAIN/LAUNCHER intent-filter
      kotlin/<pkg>/
        MainActivity.kt        ← AppCompatActivity 骨架 + TODO 注释
      assets/
        app.zpkg               ← 从 dist_dir 拷贝（若已构建）
      res/
        layout/activity_main.xml
        values/strings.xml
```

`libz42vm.so` 从平台 SDK 拷贝到 `app/jniLibs/arm64-v8a/`（命令输出给出 cp 提示）。`sourceSets.main.jniLibs.srcDirs = ['jniLibs']` 已在 `app/build.gradle` 配置。

### WASM（`z42 export wasm`）

```
<output>/
  index.html                   ← z42 App 页面（canvas + output pre）
  index.js                     ← fetch app.zpkg → Z42VM.create → vm.run()
  app.zpkg                     ← 从 dist_dir 拷贝（若已构建）
  z42vm.js                     ← 从平台 SDK 拷贝（若已安装）
  z42vm.wasm                   ← 从平台 SDK 拷贝（若已安装）
```

`index.js` 在 `Z42VM` 未定义时给出友好提示而不崩溃。用任意静态 HTTP 服务器（如 `python3 -m http.server 8080`）即可本地预览。

## 设计决策

### D1：SDK 不存在时不阻塞工程生成

**问题**：平台 SDK 尚未安装时是否应报错退出？  
**决定**：继续生成工程骨架，打印 ⚠ 提示和安装命令。工程骨架对开发者有独立价值（如填充 UI 代码、配置签名），SDK 可后补。

### D2：xcframework 手动拷贝

**问题**：为什么不自动拷贝 xcframework？  
**原因**：`Std.IO.Directory.Copy` 尚未实现（stdlib gap）。当前给出 `cp -r` 命令提示，待 `Directory.Copy` 可用后升级为自动拷贝。

### D3：固定 pbxproj UUID

**问题**：pbxproj 对象 ID 应该随机还是固定？  
**决定**：固定（基于对象语义，如 `A10000000000000000000006` 对应 main.swift FileRef）。好处：重复生成字节相同、方便 diff、无需实现 RNG。Xcode 要求 ID 在同一工程内唯一，不要求跨工程唯一。

### D4：[platform.*] 而非 [export.*]

**问题**：配置段命名 `[export.ios]` 还是 `[platform.ios]`？  
**决定**：`[platform.ios]`。`platform` 表达"该项目针对此平台"，比 `export` 更通用——即使不导出工程，平台特定参数（如最低 iOS 版本）也适用。

## Deferred / Future Work

### exp-auto-xcframework-copy

- **来源**：add-export-command 实施
- **触发原因**：`Directory.Copy` 尚未实现
- **前置依赖**：`Std.IO.Directory.Copy` stdlib 实装
- **触发条件**：stdlib 添加 `Directory.Copy` 后，将 `launcher_export_ios.z42 _generateIosProject` 的 `cp -r` 提示替换为自动拷贝
- **当前 workaround**：输出 `cp -r <src> <dst>` 命令，用户手动执行

### exp-z42vm-cabi-ios

- **来源**：add-export-command 实施
- **触发原因**：z42vm C ABI（`z42_run_app`）尚未稳定
- **触发条件**：z42vm 发布 stable C ABI 后，填充 `main.swift` 中的 `// TODO` 调用点
- **当前 workaround**：`main.swift` 含 `_ = zpkgPath` 占位，xcframework 链接后手动补调用
