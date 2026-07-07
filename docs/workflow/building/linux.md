# Linux 从零开发

在一台干净的 Linux（x64 / arm64）上从零搭好 z42 开发环境并构建。macOS 见 [`macos.md`](macos.md)，Windows 见 [`windows.md`](windows.md)。

## 1. 前置工具（一次性）

```bash
# C 工具链 —— cargo 编 C 依赖（zlib-ng / libffi 经 z42.compression）要 cc + make
sudo apt install build-essential        # Debian/Ubuntu；Fedora: sudo dnf groupinstall "Development Tools"

# Rust stable
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version

# GitHub CLI（auth'd，下载预编译 SDK 用）
# 安装见 https://github.com/cli/cli#installation
gh auth login
```

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

## 4. Linux 特定注意

- **C 依赖**：没装 `build-essential`（或等价 `cc` + `make`）会让 cargo 在编 zlib-ng / libffi 时报 `cc: command not found`。
- **平台打包**：Android / wasm 可在 Linux host cross-compile（见 [`android.md`](android.md) / [`wasm.md`](wasm.md)）；iOS 需 macOS host。
- 仅支持厂商官方维护架构（linux-x64 / linux-arm64），见 memory `project_supported_platforms`。

## See also

- 9 RID per-arch SDK 包：[`../packaging.md`](../packaging.md)
- 跨平台 / 多 RID 打包：[`../packaging.md`](../packaging.md)
- 测试入口：[`../testing/`](../testing/)
