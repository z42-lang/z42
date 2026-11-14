# export：原生工程生成器

> 代码：`src/toolchain/launcher/core/launcher_export.z42`、`src/toolchain/workload/{ios,android,wasm}/appbuilder/export.z42`
>
> `[platform.*]` 有哪些键 → [`z42.toml` 参考](https://z42-lang.github.io/z42/reference/toolchain/z42-toml.html)；
> export 与其它动词的分工 → [平台发布与导出](platform-export.md)。

`z42 export <manifest> --rid <rid>` 从一份平台无关的工程描述生成对应平台的原生工程骨架，
交给开发者在 Xcode / Android Studio / 浏览器里继续配置。生成器是**确定性模板渲染**：
没有原生编译、没有平台工具链依赖，缺 SDK 也照样产出工程。
**改生成模板、加平台配置键、查「导出的工程为什么缺 xcframework / 缺字节码」时读这页。**

## 谁拥有生成器

生成逻辑住在三个 workload 包里，而不是 launcher 自己：

| RID 类别 | 入口 | 源 |
|---|---|---|
| `ios-*` / `iossim-*` | `Z42.Workload.Ios.IosExport.Generate` | `src/toolchain/workload/ios/appbuilder/export.z42` |
| `android-*` | `Z42.Workload.Android.AndroidExport.Generate` | `src/toolchain/workload/android/appbuilder/export.z42` |
| `browser-wasm` | `Z42.Workload.Wasm.WasmExport.Generate` | `src/toolchain/workload/wasm/appbuilder/export.z42` |
| 桌面 RID | —— | 报错：桌面没有 IDE 工程可导出，改用 `z42 publish` |

`launcher_export.z42` 只做四件事：定位工程清单、按优先级解析配置、定位已构建产物与平台 SDK 目录、
按 RID 类别调对应的 `Generate`。

⚠️ **这三个包是 launcher 的编译期依赖**（`z42.launcher.z42.toml` 的 `[dependencies]`），
随 SDK 一起分发（`programs/launcher/z42.workload.*.zpkg`），**不是** `z42 workload install` 之后才被发现的。
`workload install` 装的是**平台 SDK 载荷**（xcframework / `.so` / wasm 包），不是生成器代码。
所以「没装 workload 也能 export 出工程」不是降级行为，是设计——工程骨架本身有独立价值
（填 UI 代码、配签名），SDK 可以后补。

## 配置解析

优先级：**CLI 旗标 > `[platform.<plat>]` 清单段 > 内置默认值**。

| 平台 | 必填 | 主要键与默认 |
|---|---|---|
| ios | `bundle_id`（或 `--bundle-id`） | `display_name` = `[project].name`；`min_ios` = `16.0` |
| android | `app_id`（或 `--app-id`） | `display_name` = 工程名；`version_code` = `1`；`version_name` = `[project].version`；`min_sdk` = `26`；`target_sdk` = `37` |
| wasm | —— | `title` = 工程名；入口 = `--entry` > `[project].entry` > `<name>.Main` |

段名用 `[platform.ios]` 而不是 `[export.ios]`：`platform` 表达「这个工程针对此平台」，
即使不导出工程，平台参数（最低系统版本之类）也适用。

输出目录：`--output`，缺省 `<工程目录>/<name>-{ios,android,wasm}`。

平台 SDK 目录：`<SDK 根>/runtimes/<rid>/<ver>/`。`<ver>` 取 `--sdk-ver`，
缺省 ios/android 用 `[project].version`、wasm 用 `nightly`。

## 生成的工程

### iOS

```
<output>/
  <Name>.xcodeproj/project.pbxproj     ← 完整 Xcode 工程（固定确定性对象 ID）
  <Name>/
    AppDelegate.swift                         ← AppDelegate，进程内起 VM 跑 app.zpkg
    Info.plist                         ← bundle id / display name / 最低版本
    Assets.xcassets/Contents.json
    app.zpkg                           ← 从 dist 拷（已构建时）
  z42.framework/                      ← 从对应 RID 的 runtime pack 自动复制
  z42_*.framework/                    ← interp/JIT、profiler、compression、共享 std
```

`project.pbxproj` 用**固定的 24 字符十六进制对象 ID**（按对象语义写死，不随机），
于是重复生成字节相同、可 diff、也不需要实现 RNG。Xcode 只要求 ID 在同一工程内唯一。
单 target，引用 `AppDelegate.swift`（Sources）与 `app.zpkg`、`libs/`（Resources）。应用只链接 `z42.framework`；组件与共享 std 通过 Embed Frameworks 复制和签名，profiler 不进入应用的启动链接依赖。

`AppDelegate.swift` 生成的是可运行的 AppDelegate：从 bundle 里取 `app.zpkg` 路径，
后台队列上先注册同构建组件，再调用 `z42_host_run_app`，**进程内**跑（和 desktop 的
self-contained 同一个嵌入核）。iOS 的 bundle 资源是真实文件系统路径，所以 zpkg 与 `libs/` 直接引用、
无需解压。app 的 stdout 进设备 console。

### Android

```
<output>/
  build.gradle                         ← 根脚本；AGP + Kotlin classpath
  settings.gradle
  gradle/wrapper/gradle-wrapper.properties   ← Gradle 9.7.1（带 distributionSha256Sum）
  app/
    build.gradle                       ← compileSdk/targetSdk 37、minSdk 26、jniLibs 目录
    src/main/
      AndroidManifest.xml
      kotlin/<pkg>/MainActivity.kt     ← 从 assets 解出 zpkg + libs，再进程内跑
      assets/app.zpkg
      res/{layout/activity_main.xml,values/strings.xml}
```

根脚本不声明 `kotlin-android` 插件——AGP 9 自带 Kotlin 编译；buildscript classpath 里的 KGP 条目
只是把 Kotlin 版本抬过 AGP 内置下限。

`MainActivity.kt` 与 iOS 对称，但多一步**解压**：`z42_host_run_app` 需要真实文件系统路径，
而 Android 的 assets 不是文件系统，所以先把 `app.zpkg` 与 stdlib 从 assets 拷到应用私有目录再跑。
同样跑在非主线程上（避免 ANR），stdout 进 logcat。

`libz42.so` 由 `z42 workload install android` 铺进 workload 的 jniLibs
（见[分发](workload-distribution.md)），导出的工程里 `sourceSets.main.jniLibs` 指向 `jniLibs` 目录，
需要时把 `.so` 拷进去。

### WASM

```
<output>/
  index.html                           ← 页面（输出区 + 模块脚本）
  index.js                             ← init → 组 resolver → 建 VM → 取 app.zbc → 跑入口
  app.zbc                              ← 从解析出的位置拷（若找得到）
  z42_wasm.js / z42_wasm_bg.wasm       ← 从平台 SDK 拷（若已装）
  stdlib-resolver.js
  libs/*.zpkg + libs/files.json        ← stdlib，供浏览器端 resolver 按名解析
```

wasm 是唯一消费 **`.zbc`**（单模块字节码）而不是 `.zpkg` 的导出目标：
`index.js` 用 `vm.loadZbc` 装模块、`vm.resolveEntry(mod, "<入口>")` 找入口再调用。
`Z42VM` 未定义时给出友好提示而不是崩。本地预览用任意静态 HTTP 服务器即可。

## 设计取舍

**iOS 复制同批组件。** 从 runtime pack 复制组件 Framework，缺失的组件直接报错；核心 Framework 不存在时提示安装对应运行时包。

**pbxproj 用固定 ID。** 换来重复生成字节相同，代价是同一工程里的 ID 必须手工保证不撞。

发布包验证会编译实际导出的 arm64 模拟器应用，检查所有 Framework 已进入应用包，并从 Mach-O 依赖中确认 profiler 没有被启动链接。

## 代码地图

| 组件 | 位置 |
|---|---|
| `export` 入口、RID 类别路由、配置解析、SDK/产物定位 | `src/toolchain/launcher/core/launcher_export.z42` |
| iOS 工程渲染（pbxproj / AppDelegate.swift / Info.plist） | `src/toolchain/workload/ios/appbuilder/export.z42` |
| Android 工程渲染（gradle / manifest / MainActivity） | `src/toolchain/workload/android/appbuilder/export.z42` |
| WASM 页面渲染 + stdlib 铺设 | `src/toolchain/workload/wasm/appbuilder/export.z42` |
| 平台模板与原生绑定 | `src/toolchain/workload/<plat>/{template,platform}/` |

## 边界与限制

- **🔴 wasm 找不到自己的 `.zbc`**：`_expResolveZbc` 只找 `<output_dir>/<name>.zbc`、
  `<projDir>/.cache/<name>.zbc`、`<projDir>/<name>.zbc` 三处，而编译器实际按源文件树逐文件写
  `dist/<源相对路径>.zbc`（如 `dist/src/main.zbc`）。默认布局的工程**必然**命中不了，
  导出的 wasm 工程总是缺字节码。
- **iOS 导出需要对应 RID 的运行时包**；模拟器与真机 Framework 不能混用。
- **导出是单向的**：生成器每次整份重写目标目录里它负责的文件，不做合并。
  工程里要塞平台专属改动，目前只能在导出之后手改，改完再 export 会被盖掉。
