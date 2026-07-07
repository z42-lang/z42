# macOS 从零开发

在一台干净的 macOS（arm64，主开发平台）上从零搭好 z42 开发环境并构建。Windows 见 [`windows.md`](windows.md)，Linux 见 [`linux.md`](linux.md)。

## 1. 前置工具（一次性）

```bash
# Xcode Command Line Tools —— cargo 编 C 依赖（zlib-ng / libffi 经 z42.compression）要 cc + make
xcode-select --install

# Rust stable
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # 或 brew install rustup
rustc --version

# GitHub CLI（auth'd，下载预编译 SDK 用）
brew install gh && gh auth login
```

> macOS 是**唯一**能产 iOS 包的 host（需 Xcode）。iOS facade 见 [`ios.md`](ios.md)。

## 2. 拿到 z42

**(推荐) 下载预编译 primer** —— 鸡生蛋的唯一原生引导：

```bash
./scripts/install-z42.sh                       # → ./.z42/（z42 launcher + z42c + z42vm + stdlib）
export PATH="$PWD/.z42:$PWD/.z42/bin:$PATH"     # z42 / z42c / z42vm 上 PATH
Z42_LIBS="$PWD/.z42/libs" z42c build scripts/xtask.z42.toml --release   # 编 dev CLI → artifacts/xtask/xtask.zpkg
```

**(替代) 从源码整套构建**（不下预编译）：

```bash
./xtask build all     # 编译器（z42c 自举）+ VM（cargo）+ stdlib
```

> 冷启动 bootstrap 机制（warm/cold 种子、成对分代）见 [`../testing/bootstrap.md`](../testing/bootstrap.md)。

## 3. 日常工作流

```bash
./xtask build all    # 编译器 + VM + stdlib
./xtask test         # 全套测试（GREEN gate）
./xtask -h           # 全部命令
```

按组件细分：编译器 [`compiler.md`](compiler.md)、VM [`vm.md`](vm.md)、stdlib [`stdlib.md`](stdlib.md)。

## 4. macOS 特定注意

- **C 依赖**：`xcode-select --install` 没装会让 cargo 在编 zlib-ng / libffi 时报 `cc: command not found` / `xcrun: error`。
- **平台打包**：iOS 见 [`ios.md`](ios.md)；Android/wasm 也可在 macOS host cross-compile（见 [`android.md`](android.md) / [`wasm.md`](wasm.md)）。
- 不支持 `macos-x64`（只支持厂商官方维护架构，见 memory `project_supported_platforms`）。

## See also

- 9 RID per-arch SDK 包：[`../packaging.md`](../packaging.md)
- 跨平台 / 多 RID 打包：[`../packaging.md`](../packaging.md)
- 测试入口：[`../testing/`](../testing/)
