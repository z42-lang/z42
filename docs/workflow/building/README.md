# building/

按"我要做什么"挑文件。每份文档都是**编号 step 配方**，从零开始可直接照抄。本目录分三类：

## ① 平台开发环境（在某 OS 上从零开发 z42）

| 文件 | 用途 |
|------|------|
| [`macos.md`](macos.md) | macOS（arm64，主开发平台）从零设置 + 构建 |
| [`linux.md`](linux.md) | Linux（x64 / arm64）从零设置 + 构建 |
| [`windows.md`](windows.md) | Windows（Git Bash + MSVC）从零设置 + 构建 |

## ② 编译 z42 组件

| 文件 | 用途 |
|------|------|
| [`compiler.md`](compiler.md) | z42c 编译器（z42 自举）/ `z42c` 命令 |
| [`vm.md`](vm.md) | Rust VM / `z42vm` + feature flag |
| [`stdlib.md`](stdlib.md) | 24 个 stdlib 包 workspace |

> 跨平台 / 多 RID 打包见 [`../packaging.md`](../packaging.md)；平台支持矩阵设计见 [`docs/design/runtime/cross-platform.md`](../../design/runtime/cross-platform.md)。

## ③ 嵌入 z42 到宿主（facade）

> ⚠️ 这三类是把 z42 **VM 嵌进宿主运行时**（消费者视角），不是"开发 z42 本身"。统一三段结构：① Host 环境准备 → ② 编译（facade + 嵌入 app）→ ③ 运行测试用例。

| 文件 | 用途 |
|------|------|
| [`wasm.md`](wasm.md) | 🟢 WASM facade（`@z42/wasm` npm 包）|
| [`ios.md`](ios.md) | 🟢 iOS facade（`Z42VM.xcframework` SwiftPM 包）|
| [`android.md`](android.md) | 🟢 Android facade（`z42vm.aar` AAR module）|

平台 facade 的源码 + 跨平台契约见 [`platform-contract.md`](../../../src/toolchain/workload/platform-contract.md)；设计与决策见 [`docs/spec/`](../../spec/)。

---

## 日常入口：xtask

桌面日常用 **xtask**（编译产物 `artifacts/xtask/xtask.zpkg`，源码 `scripts/xtask*.z42`）：`./xtask build [all]` / `./xtask test`。

更省事：把 xtask 编成一个**原生 apphost** `./xtask`（仓库根），直接 `./xtask build [all]` / `./xtask test`，免走 `z42 …zpkg` 入口。产出（两条命令，无 wrapper 脚本）：

```bash
Z42_LIBS="$PWD/.z42/libs" z42c build scripts/xtask.z42.toml --release   # 编 xtask.zpkg
z42 publish scripts/xtask.z42.toml                                        # 读 [platform.desktop] → ./xtask
```

`./xtask` 原生 + 平台相关 + 已 gitignore（重生不提交）；机制与 `[apphost].publish_dir` 配置见 [`runtime/launcher.md`](../../design/runtime/launcher.md#z42toml-配置apphost-publishapphost-out-path-2026-06-10)。
