# docs/workflow/

**面向开发者：怎么跑命令。** 设计原理归 [`docs/design/`](../design/)；spec 提案流程归 [`.claude/rules/workflow.md`](../../.claude/rules/workflow.md)。

## 先拿到 z42

**前置工具**：git + Rust stable（`rustc --version` 自检）+ `gh`（auth'd，下载 SDK 用）。
工具链 100% z42 自举：`z42c`（编译器）用 z42 写、编译为 zpkg；`z42vm`（VM）是 Rust。

首次获取 launcher、编 `xtask` dev CLI、跑第一个程序的完整步骤见 [`quickstart.md`](quickstart.md)。装好后核心两步：

```bash
./xtask build all    # 编译器 + VM + stdlib
./xtask test         # 全部测试
```

完整命令：`./xtask help`（源 = [`scripts/xtask*.z42`](../../scripts/)）。

> 从源码整套构建（不下预编译）见 [`building/`](building/)；冷启动 bootstrap 机制见 [`building/stdlib.md`](building/stdlib.md)；**自举 + 本地/CI 交叉验证的完整流程**（SDK vs Current 两套 toolchain、边界不变量、冗余清单）见 [`testing/bootstrap.md`](testing/bootstrap.md)。
> 可选环境变量：`Z42_LIBS`（stdlib 扁平目录，默认 `artifacts/build/libraries/dist/release/`）、`Z42_PORTABLE_VM`（z42vm 路径）——CI 显式设置，本地默认即可。

## 我要做 ... → 看 ...

| 我要 | 看 |
|------|-----|
| 在 **macOS** 从零开发 | [`building/macos.md`](building/macos.md) |
| 在 **Linux** 从零开发 | [`building/linux.md`](building/linux.md) |
| 在 **Windows** 从零开发 | [`building/windows.md`](building/windows.md) |
| 编 z42c 编译器（z42 自举）| [`building/compiler.md`](building/compiler.md) |
| 编 Rust VM | [`building/vm.md`](building/vm.md) |
| 重建 stdlib | [`building/stdlib.md`](building/stdlib.md) |
| 嵌入 z42 到 **WASM** | [`building/wasm.md`](building/wasm.md) |
| 嵌入 z42 到 **iOS** | [`building/ios.md`](building/ios.md) |
| 嵌入 z42 到 **Android** | [`building/android.md`](building/android.md) |
| 跑编译器单测（z42c 自举 + cargo test）| [`testing/unit-tests.md`](testing/unit-tests.md) |
| 跑 VM golden tests | [`testing/vm-tests.md`](testing/vm-tests.md) |
| 跑 stdlib `[Test]` | [`testing/stdlib-tests.md`](testing/stdlib-tests.md) |
| 跑 cross-zpkg e2e | [`testing/cross-zpkg.md`](testing/cross-zpkg.md) |
| 只跑 git diff 影响的测试 | [`testing/changed-only.md`](testing/changed-only.md) |
| 看 CI matrix / GREEN 标准 | [`ci.md`](ci.md) |
| **本地打 9 个 per-arch SDK package** | [`packaging.md`](packaging.md) |
| 打跨平台 release | [`release.md`](release.md) |
| lldb / gdb / dap 调试 | [`debugging.md`](debugging.md) |

## artifacts/ 目录速查

```
artifacts/
├── build/
│   ├── z42c/<member>/release/dist/      z42c 自举产物（z42c.driver.zpkg + 6 siblings）
│   ├── runtime/{debug,release}/         cargo build 产物（z42vm）
│   └── libraries/
│       ├── <lib>/release/dist/          per-lib workspace .zpkg
│       └── dist/release/                flat 视图（VM 默认加载路径；无 namespace 索引——读 NSPC）
└── xtask/xtask.zpkg                      编译后的 dev CLI
```

`artifacts/` 和 `target/` 都 gitignore。
