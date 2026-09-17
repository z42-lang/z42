# 平台发布与导出：动词模型

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/toolchain/launcher/core/launcher_export.z42`、`src/toolchain/builder/core/builder_publish.z42`、`src/toolchain/builder/core/builder_device.z42`
>
> 命令与旗标怎么敲 → [工具链参考](../../../reference/src/toolchain/README.md)；
> 产物形态的坐标系 → [部署模型](deployment-model.md)；工程生成细节 → [export](export.md)。

z42 有五个跟平台沾边的动词（`build` / `run` / `export` / `publish` / `test`），
但**只有三个真的会按平台分叉**。这页写清每个动词在哪一层分叉、分叉后交给谁、
以及今天实际支持到哪一步。**加平台、改动词语义、判断某个组合该由谁实现时读这页。**

## 立柱：build 永远不分叉

**编译一次，产出平台无关的 `app.zpkg`。** zbc 字节码跨平台字节一致，移动端只装 VM、
跑宿主编出来的 zpkg（见[嵌入](../runtime/embedding.md)）。平台之间只差「嵌入 host + 原生打包/跑测外壳」，
**没有 per-platform 的应用代码**。

这条立柱在命令面上是可验证的：`z42c build` 根本没有 `--rid` 旗标。
分叉只发生在 export（IDE 工程）、publish（可分发件）和 on-device test 上。

## RID 的类别决定平台

平台不是子命令，是 `--rid` 的**类别**。加一个平台不增加命令。

| 前缀 | 类别 |
|---|---|
| `macos-` / `linux-` / `windows-` | desktop |
| `ios-` / `iossim-` | ios |
| `android-` | android |
| `browser-wasm` | wasm |

launcher 与 z42b 各有一份同构的映射（`_ridCategory` / `_familyOfRid`）——
launcher 用它决定把活派给谁，z42b 用它选 workload 族。

## 动词矩阵

| 动词 | 分叉？ | 谁实现 | 产出 | 现状 |
|---|---|---|---|---|
| `z42 build <manifest>` | 否 | z42c（launcher 直接转发 `bin/z42c`） | `dist/<name>.zpkg` | ✅ |
| `z42 run [target]` | 否 | launcher | 在宿主 vm 上跑 | ✅ |
| `z42 export <manifest> --rid` | **是** | launcher + 三个 workload 包 | 原生 IDE 工程 | ✅ ios / android / wasm；desktop 报错 |
| `z42 publish <manifest> --rid` | **是** | 转发 z42b | 发布件 | ✅ desktop（apphost）；其余报「未实现」 |
| `z42 test` | 否（默认） | 转发 z42b | 宿主上跑 `[Test]` | ✅ |
| `z42b test --rid <设备 RID>` | **是** | z42b 的设备驱动 | 设备上跑 `[Test]` + 回收报告 | ✅ wasm / ios / android |

### export vs publish：共享工程生成，交付物不同

| | `export` | `publish` |
|---|---|---|
| 交付物 | 原生工程目录 | 可分发件（apphost / `.ipa` / `.aab` / wasm bundle） |
| 跑原生构建？ | 否（只渲染模板，快） | 是 |
| 给谁 | 人 / IDE / 第三方 CI | 分发渠道 |

两者不合并，因为 export 的关键价值就是**只要工程、不 build**：在 Xcode 里配签名、
交给第三方 CI、或者接手托管时，要的是「快速拿到工程」而不是等一次完整构建（还可能因签名失败）。
合成 `publish --export` 会丢掉这条快速路径。

**desktop 没有 export**：桌面没有 IDE 工程可生成，它的发布产物 apphost 是 publish 形态。
`z42 export --rid macos-arm64` 会明确报错并指向 `z42 publish`。

### publish 的边界

`publish` = **产出可分发件就停**，语义对齐 `dotnet publish`（产部署件，不部署到服务器/商店）。

| 在范围内 | 出范围（用户自己做） |
|---|---|
| 产 apphost / `.ipa` / `.aab` / wasm bundle | 上传 App Store / Play / npm |
| 按清单里的身份引用做 dev / ad-hoc 签名 | 发布级签名、notarization、provisioning 全流程 |
| 嵌 VM + 打 zpkg + 平台构建配置 | 商店元数据、截图、审核、渠道 |

上架流程各家差异极大（签名 / provisioning / 审核 / 2FA / 各家 CI），交给用户。

### desktop publish 的分工

`z42 publish --rid <desktop-rid>` 的实现横跨两个进程，分界线画得很清楚：

- **launcher 管「已装的东西在哪」**：定位工程清单、解析已装 desktop workload 里的
  `apphost-<rid>` stub，经 `Z42_APPHOST_TEMPLATE` 传下去。
- **z42b 管「怎么做出产物」**：gate `[platform.desktop] apphost = true`、确保 zpkg 最新、
  patch stub、搬 payload 与依赖。

这样切的收益是 z42b 的 publish **不含任何 runtime / workload 解析逻辑**，
也就不依赖 `z42.project` / `z42.build`——这对兼作 stdlib 测试运行器的 z42b 是硬约束
（见 [z42b](z42b.md#定位与进入方式)）。

stub 的解析序是 **工程 `[build] hooks` 现场产出 → `Z42_APPHOST_TEMPLATE` → 报错**。
第一条让带 hook 的工程免装 desktop workload；本仓的 `./xtask` 走的就是这条。

desktop workload 是**一个 RID 无关的包**，里面带每个桌面 RID 的 `apphost-<rid>`，
所以任意 host 都能 `publish --rid <别的桌面 RID>`——除了 macOS 目标必须在 macOS host 上签名。

## on-device test：同一套 `[Test]`，两个运行面

| 运行面 | 命令 | 用途 |
|---|---|---|
| **宿主**（默认、快） | `z42 test` | 内循环 / CI 主门；z42b 在宿主 VM 上反射跑 `[Test]` |
| **设备上**（慢、真） | `z42b test <bundle> --rid <设备 RID>` | 抓平台专属行为：PAL/fs、native interop、ABI、真机差异 |

用例只写一份，没有 per-platform 测试代码。

设备路径的职责切分是这页最容易搞错的一处：**z42b 是单目标执行器，不是舰队编排器**。
z42b 跑在宿主的 z42vm 上，它当然不可能「是」浏览器里或模拟器里的那个 runner；
所谓 `--run` 是在宿主上拉起平台原生驱动（playwright / xcodebuild / gradlew），
把暂存好的部署件注入进去，再把报告读回来。原生工具链（node / 浏览器 / Xcode / NDK）由编排方
（xtask）**预备**并把位置传进来（`--build-root` / `--node-bin`），z42b 只拥有**调用逻辑**。

三个子步骤可以单独跑（CI / 调试用），不带任何一个就是 build+deploy+run 全流程：

| 旗标 | 做什么 |
|---|---|
| `--stage-only` | 只组装 `{app, libs, bundle}` 部署件（平台无关，任何设备 RID 都能做） |
| `--build` | 跑平台原生构建（wasm-pack / xcframework / cargo-ndk） |
| `--run` | 部署到设备/模拟器、跑、回收报告 |

## 边界与限制

- **没有 `z42 run <平台>`**：设计里的「run 双形态」（`run desktop` 起 apphost 预演部署启动、
  `run ios/android` 部署到模拟器、`run wasm` 起服务）**未实现**。`z42 run` 不接受 `--rid`，
  只跑宿主上的 zpkg / 工程 / 单文件。要预演 apphost 就先 `publish` 再直接执行产物。
- **mobile / wasm 的 publish 未实现**：`--rid` 落到非 desktop 类别时，
  launcher 与 z42b 各自给出一条「not yet implemented」并以 2 退出。
- **托管工程模型（managed + eject）未实现**：设计里的 `platforms/` 生成区、
  `platform-overrides/<plat>/` 合入、`z42 platform add` 与 `z42 eject` 都还不存在，
  仓库里只有 workload 类骨架的注释提到它们。今天 export 的输出目录就是普通目录，
  重新 export 会覆盖，平台专属改动没有受保护的去处（见 [export](export.md)）。
- **workload 的尾阶段没接线**：`iOSWorkload` / `AndroidWorkload` / `WasmWorkload` /
  `DesktopWorkload` 描述了 `Configure → Package` 该做什么，但方法体是注释，
  且没被 `_selectWorkload` 选中。今天真跑的平台逻辑在 launcher 的 export 与 z42b 的 publish 里，
  不在管线上（见 [z42b](z42b.md#管线之外的两条实路)）。
