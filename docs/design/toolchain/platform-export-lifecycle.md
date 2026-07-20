# 跨平台工程导出 & 生命周期

> ⚠️ **前瞻设计草案（未实施）**。设计"z42 项目 → 导出平台工程（Xcode/Android Studio…）→ 在各平台 build/publish/test"的工具。落地开 spec。命令由平台 workload 提供（见 [launcher-command-dispatch.md](launcher-command-dispatch.md)），不进 launcher 核心。

## 立柱不变量

**build 一次产出平台无关的 `app.zpkg`；导出/打包/测试才按平台分叉。**

[embedding.md](../runtime/embedding.md) 已定"移动端只装 VM、跑 host 编出的 zpkg"。因此同一份 `app.zpkg` 字节码在所有平台被打包/测试——平台间只差"嵌入 host + 原生打包/跑测外壳"，**无 per-platform 代码漂移**。整条生命周期都挂在这条立柱上。

## 两阶段：z42 项目（主，规范）→ 导出平台工程（派生，可重生）

```
① z42 new myapp            # 平台无关 z42 项目（z42.toml + src/）；桌面直接 build/run
② z42 platform add ios     # 往 z42.toml 加 [platform.ios] 段（+ 可选 platform-overrides/ios 骨架）
③ z42 export ios           # 读配置 → 导出 platforms/ios/（managed、可重生）；Xcode 打开跑
④ z42 eject ios            # 需要时停止托管，固化为自有工程（单向逃生口）
```

- z42 项目**规范、可提交、跨平台**；`platforms/` 是 `export` 派生的、可 gitignore、随时重生。
- **桌面优先、移动后长**：无 `[platform.*]` 即普通桌面项目，加一段解锁一个导出目标，增量不返工。

## 所有权模型：managed 默认 + eject 逃生口

managed 的成败在"**生成区 vs 你的区**"的边界（同 Expo prebuild / CNG）：

```
myapp/
  z42.toml                  # 唯一真相源（含 [platform.*]）          ┐
  src/                      # 你的 z42 代码                          │ 你的区（提交）
  assets/                   # 你的共享资源                            │
  platform-overrides/{ios,android}/   # 平台专属补丁，export 时合入   ┘
  platforms/                # ★ 生成区：wipe-and-regenerate，建议 gitignore
    ios/  MyApp.xcodeproj    #   ← 从 toml 生成
          Z42Host/           #   ← VM 嵌入 shim + zpkg loader（别手改）
          Resources/myapp.zpkg   # ← build hook 刷新的构建产物
    android/ …
```

三条机制：
1. **`platforms/` 生成且可丢弃**：`z42 export` 从 `(z42.toml + workload 嵌入件 + app.zpkg + platform-overrides/)` 整体重生 → 无 merge 冲突，真相恒为 toml+overrides。
2. **`platform-overrides/` 逃生 valve**：塞平台专属代码/权限/原生依赖，export 时合入。薄 VM host 多数用不到；要深度定制走这，再不够才全量 eject。
3. **`z42 eject <plat>`**：把 `platforms/<plat>/` 摘出托管 → 用户完全拥有、提交、随便改；export 不再碰它。**单向**，代价是升级 VM 转为手动（详见下"eject"）。

### eject 的作用与代价

| | managed | eject 后 |
|---|---|---|
| `platforms/<plat>/` | 生成物、可重生 | 你拥有、提交、随便改 |
| `z42 export` | 重生 | 跳过它 |
| VM 嵌入/zpkg 管线 | 工具从 toml 自动维护 | 固化成普通文件、你维护 |
| 升级 VM | 改 `runtime` 号 + 重 export | **手动**更新 xcframework/shim/build 设置 |

有 eject 兜底，才敢默认 managed（消除"被工具锁死"顾虑）；99% 停在 managed，eject 是给撞墙的 1% 的门。类比 `expo prebuild` bare / CRA `npm run eject`。

## `z42.toml`：程序「是什么」与平台「导出配方」分离

```toml
# —— 平台无关：这个 z42 程序是什么 ——
[project]      name = "myapp"   runtime = "0.3.x"   # runtime pin → 决定配套 xcframework/AAR 版本（ABI 一致）
[build]        …
[dependencies] …

# —— 导出配方：怎么包装到各平台 ——
[platform.ios]
bundle-id    = "com.acme.myapp"
display-name = "My App"
min-ios      = "15.0"
signing      = "Acme Dev"       # 身份引用名，不存证书
capabilities = ["camera"]       # → Info.plist / entitlements
icon         = "assets/icon.png"

[platform.android]
package     = "com.acme.myapp"
app-name    = "My App"
min-sdk     = 24
permissions = ["CAMERA"]
```

`platform add ios` = 按 `[project].name` 推默认 bundle-id 把这段 scaffold 出来。程序本身永远平台无关。

## `z42 export <plat>`：把三层 materialize 到 `platforms/<plat>/`

| 层（= 用户视角的三部分）| export 时做什么 |
|---|---|
| **1 VM 嵌入（固定）** | 从已装 workload 取**匹配 `runtime` 版本**的 xcframework/AAR，引用进工程（Swift Package local path / Gradle dep）；版本不匹配报错 |
| **2 app.zpkg** | 触发 `z42 build` 产 app zpkg → 放进 `Resources/`(iOS)/`assets/`(Android)；装 build hook |
| **3 平台设置** | 读 `[platform.*]` → 模板替换出 `.xcodeproj`/Info.plist/entitlements / `build.gradle`/AndroidManifest.xml；合入 `platform-overrides/` |

复用现状：原生骨架直接基于 `xtask package --rid <rid>` 已产的 `Package.swift`/`Sources/` + xcframework（[embedding.md §package 布局](../runtime/embedding.md)），叠加"配置替换 + 用户 zpkg + build hook + overrides 合入"。

### 原生 host shim（每平台一小块固定胶水）

参数化于 entry/导出函数，只干三件事：构造嵌入 `Z42VM`（C ABI）→ 用平台 resolver（iOS `BundleZpkgResolver` / Android assets）加载 stdlib + app zpkg → 跑 Main / 桥接 UI。属"生成区、别手改"。

### build hook

生成工程挂一个 build phase（Xcode）/ Gradle task，在原生编译**前**回调 `z42 build` 重编 `src/*.z42` → 刷 zpkg 进 `Resources|assets/`。于是 Xcode/AS 里按 Run 自动同步 z42 代码改动。

## 全平台生命周期动词

z42 编译产物是**平台无关字节码 `app.zpkg`**（跑在 z42vm 上，跨平台字节一致）。平台差异纯在"启动器/包装"——所以 `build` 永远平台无关，平台分叉只在 run（部署形态）/ publish（可分发件）/ export（IDE 工程）发生。

```
z42 build                    # src → app.zpkg（字节码，平台无关，产一次）
z42 run <app.zpkg>           # 跑 zpkg（host vm，最快内循环）
z42 run     <toml> --rid <rid>   # 以 rid 平台的部署形态跑：desktop=apphost / ios·android=on-device / wasm=浏览器
z42 publish <toml> --rid <rid>   # → release 可分发件（desktop apphost / .ipa / .aab / wasm bundle），到此为止
z42 export  <toml> --rid <rid>   # 原生 IDE 工程（ios / android；desktop·wasm 无 IDE 工程）
z42 test    <plat> [--device sim|emulator|hw]   # host + 平台上两面跑 [Test]
```

> **命令面统一为 `--rid`（unify-platform-deploy-rid, 2026-06-17）**：publish/export/run 的部署形态不再用平台子命令
> （`publish desktop` 等），改为 `<toml> --rid <rid>`——rid 的 category 决定平台，全平台一条命令、加平台不增命令。
> `run --rid` 检测 `--rid` 走部署形态，否则 `run <app.zpkg>` 跑字节码。desktop 的 apphost 由单 desktop workload
> 携全 RID `apphost-<rid>` 提供 → `publish --rid <target>` 可跨平台输出（macos 目标需 macos host 签名）。

> **命令模型（define-cli-command-model, 2026-06-17）**：
> - **build 永远产平台无关 zpkg**（字节码语言，类比 `javac`→jar / `dotnet build`→dll）；不产平台件。
> - **run 双形态**：`z42 run`（无参）跑 zpkg 字节码于 host vm（走 launcher 解析，含 runtimeconfig/版本 pin）；`z42 run <plat>` 以该平台**部署形态**跑——`run desktop` 跑 **apphost**（走 apphost 自解析，预演真实部署启动路径；产物 temp、ephemeral）、`run ios/android` 部署 debug 到 sim/真机、`run wasm` serve+open。`run <plat>` = debug/临时；`publish <plat>` = release/留存——同管线两点（对标 `flutter run` vs `flutter build`、`dotnet run` vs `dotnet publish`）。
> - **publish 的 desktop 形态 = apphost**（取消 `export desktop`：desktop 无 IDE 工程可"导出"，apphost 是 publish 部署件）。`[platform.desktop] apphost = true` 即声明桌面输出（gate；`publish_dir` 仅输出位置，不再充当开关）；`apphost.z42` 的 stub-patch 逻辑是 desktop publish/run 的实现。**无独立 `z42 apphost` 命令**。apphost **stub** 解析序（2026-07-20 wire-z42b 阶段 7）：项目 `[build] hooks` 现场产出 → `Z42_APPHOST_TEMPLATE`（已装 workload）→ 报错——项目带 hook 即免装 desktop workload，见 [build-orchestrator.md](build-orchestrator.md#publish-apphosthook-免装-workload-产-stub需求③)。
> - **export 仅 ios/android**（生成 Xcode/gradle 原生工程供深度定制）；desktop·wasm 无此概念。
> - **AOT 在 publish 层**（可选 `--aot`，产平台原生机器码，类比 dotnet NativeAOT 是 publish 选项）；build 默认产字节码。
> `[platform.desktop]` 键表见 [project.md](../compiler/project.md)；命令分发分层见 [launcher-command-dispatch.md](launcher-command-dispatch.md)。

### `export` vs `publish`：共享逻辑，不同交付物（2026-06-17 裁决）

`publish` 内部会**复用 `export` 的工程生成逻辑**（产 .ipa 需先生成 Xcode 工程再 build），但两命令**交付物不同**，都保留、不合并（对标 expo `prebuild` vs `eas build`）：

| | `export <plat>` | `publish <plat>` |
|---|---|---|
| 交付物 | **原生工程**（Xcode/gradle 目录）| **可分发件**（.ipa/.aab/apphost）|
| build? | 否（只生成，快）| 是（内部生成工程 + build）|
| 给谁 | 人 / IDE / 第三方 CI / eject 接手 | 分发渠道 |

- **工程生成器是共享内部原语**：`export` 只暴露"生成"、`run <plat>`/`publish <plat>` 内部"生成 + build/deploy"。
- **保留 `export` 的关键价值 = "只要工程、不 build"**：eject / 在 Xcode 配签名 / 第三方 CI 接手时，要快速拿工程而非等一次完整 build（还可能因签名失败）。合进 `publish --export` 会丢这条快速路径，故不合并。
- `publish` 默认不留中间工程；`--keep-project` 可留下供排查。

### `publish` 的语义与边界（≈ `dotnet publish`，不是上架）

`publish` = **产出可分发件就停**；语义对齐 `dotnet publish`（产部署件，不部署到服务器/商店）。**上架差异性极大（签名/provisioning/审核/2FA/各家 CI），交给用户**。

| 在 scope | 出 scope（用户做）|
|---|---|
| 产 `.ipa/.aab/.apk/wasm/apphost` | 上传 App Store / Play / npm |
| 按 toml 身份引用做 dev/ad-hoc 签名（能装真机）| 发布级签名 / notarization / provisioning 全流程 |
| 嵌 VM + 打 zpkg + 平台 build 配置 | store 元数据/截图/审核/渠道 |

> 指引：上架用 Xcode Organizer/Transporter、Play Console、fastlane 等。与"Phase 1 全 unsigned、签名/notarization 留给 release CI"的现有延后一致。

### `test`：同一套 `[Test]`，两个运行面

| 运行面 | 命令 | 用途 | 机制 |
|---|---|---|---|
| **host**（默认、快）| `z42 test` | 内循环 / CI 主门 | z42b 在 host VM 跑 [Test]（z42.builder.zpkg 经 z42vm）|
| **平台上**（慢、真）| `z42 test ios --device sim` | 抓平台专属行为（PAL/fs、native interop、ABI、真机差异）| 导出 test-harness 版平台工程（入口跑 test-runner over 捆绑 `[Test]` zpkg）→ build+装+起 sim/emulator/device → 回收 TAP |

`[Test]` 用例**只写一份**，host 跑得快、平台上跑得真，无 per-platform 测试代码。类比 XCTest 包装 / Android instrumented test，payload 换成"test-runner 跑在嵌入 VM 上"。

## CI 咬合

- 内循环：host 测试（快）。
- pre-merge/nightly：per-RID 矩阵 `export → publish → test --device`，扩展现有 ci.yml 已有的"per-RID 跑 package"骨架。

## Decisions

| # | 决定 | 理由 |
|---|------|------|
| 1 | 立柱：build 一次产平台无关 zpkg，publish/test 分叉 | 消除 per-platform 代码漂移；同一字节码全平台复用 |
| 2 | 两阶段：z42 项目（规范）→ export 平台工程（派生）| z42.toml 单一真相；platforms/ 可丢弃可重生 |
| 3 | managed 默认 + eject 逃生口 | 解耦三层 + 升级只换版本号；eject 兜底消除锁死顾虑 |
| 4 | `publish` 命名（非 `package`）+ 停在可分发件 | 对齐 `dotnet publish` 语义、避与 `xtask package`(SDK件) 撞名；上架高变差交用户 |
| 5 | test 双运行面，单一 `[Test]` 套件 | host-fast 内循环 + on-platform 发布信心，零测试代码重复 |
| 6 | runtime pin 决定配套嵌入件版本 | ABI 一致；升级 = 改 `runtime` + 重 export |

## Deferred / 待 spec 细化

- `[platform.*]` 完整 schema（图标/splash/orientation/额外原生依赖/多 flavor）。
- `platform-overrides/` 合入机制（文件覆盖式 vs Expo 式声明 patch）。
- `platforms/` 默认 gitignore vs 团队 pin 的取舍。
- on-platform test 的 device 选择 / 真机签名 / TAP 回收协议细节。
- multi-arch 合并 container（multi-slice xcframework / multi-ABI AAR）——见 embedding.md Deferred。
- build hook 形态（shell 调 `z42 build` vs 原生 build-tool 插件）。
