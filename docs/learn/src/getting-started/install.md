# 安装 z42

这一章把 z42 工具链装到你的电脑上，并确认它能正常工作。整个过程只需要一条命令，通常不到一分钟。

装好之后你会得到：

- `z42` 命令——新建、构建、运行、测试工程都通过它完成；
- 编译器、虚拟机和标准库——`z42` 会自动调用它们，平时不需要直接接触。

## 支持的平台

| 系统 | 架构 | 安装方式 |
|------|------|---------|
| macOS | Apple Silicon（arm64） | 安装脚本 |
| Linux | x64、arm64 | 安装脚本 |
| Windows | x64 | PowerShell 安装脚本 |

> 目前还不支持 Intel 芯片的 Mac。

## 一行命令安装

**macOS / Linux**：打开终端，运行：

```sh
curl -fsSL https://z42-lang.github.io/z42/install.sh | sh
```

**Windows**：打开 PowerShell，运行：

```powershell
irm https://z42-lang.github.io/z42/install.ps1 | iex
```

安装脚本会：

1. 识别你的系统和架构，从 [GitHub Releases](https://github.com/z42-lang/z42/releases) 下载对应的最新版 SDK；
2. 用发布页上的 SHA-256 校验和验证下载内容没有损坏或被篡改；
3. 解压到 `~/.z42`（Windows 上是 `%USERPROFILE%\.z42`）；
4. 把 `~/.z42` 和 `~/.z42/bin` 加入 `PATH`：macOS / Linux 写入你所用 shell 的配置文件（zsh 是 `~/.zshrc`，bash 是 `~/.bashrc` 或 `~/.bash_profile`，fish 是 `~/.config/fish/conf.d/z42.fish`），Windows 写入当前用户的 `Path` 环境变量。

安装成功时，终端最后会提示你重新打开终端，并给出接下来可以运行的命令。

## 验证安装

**重新打开一个终端窗口**（让新的 `PATH` 生效），然后运行：

```sh
z42 --version
```

看到类似下面的输出就说明安装成功了（版本号和日期会不同）：

```text
z42 0.6.0 (macos-arm64, 2026-09-16)
```

括号里依次是运行平台和这个版本的构建日期。

再看看 `z42` 能做什么：

```sh
z42 help
```

它会列出全部命令。现在不必记住它们，后面的章节会逐个用到。

## 安装选项

安装脚本接受几个参数。通过管道运行时，参数写在 `sh -s --` 后面：

```sh
curl -fsSL https://z42-lang.github.io/z42/install.sh | sh -s -- --version 0.6.0
```

| 参数 | 作用 |
|------|------|
| `--version <版本>` | 安装指定版本，例如 `0.6.0`；默认 `nightly`，即最新构建 |
| `--dest <目录>` | 安装到指定目录；默认 `~/.z42`（设置了 `Z42_HOME` 环境变量时用它） |
| `--no-modify-path` | 不修改 shell 配置文件，只打印需要手动加入 `PATH` 的那一行 |
| `--archive <文件>` | 从本地已下载的 SDK 压缩包安装，适合无法联网的机器 |
| `--force` | 即使已是同一版本也重新安装 |
| `--dry-run` | 只显示将要做什么，不做任何修改 |

Windows 的 `install.ps1` 有同样的选项（`-Version`、`-Dest`、`-NoModifyPath`、`-Archive`、`-Force`、`-DryRun`）。通过 `irm ... | iex` 运行时无法传参数，可以用环境变量指定版本和位置：

```powershell
$env:Z42_VERSION = "0.6.0"; $env:Z42_HOME = "D:\z42"
irm https://z42-lang.github.io/z42/install.ps1 | iex
```

## 更新

重新运行一遍安装命令即可：

```sh
curl -fsSL https://z42-lang.github.io/z42/install.sh | sh
```

如果已经是最新版，脚本会直接告诉你「already up to date」而不重复下载。更新只替换 SDK 自身的文件，你装过的平台 workload 等内容会保留。

## 卸载

z42 的全部文件都在安装目录里，删除它即可：

```sh
rm -rf ~/.z42
```

然后从 shell 配置文件中删掉安装脚本加入的两行（以 `# added by the z42 installer` 开头的那一段）。

Windows 上删除 `%USERPROFILE%\.z42` 目录，并在「系统属性 → 环境变量」里从用户 `Path` 中移除 `.z42` 和 `.z42\bin` 两项。

## 不用脚本，手动安装

也可以自己下载解压：

1. 打开 [Releases 页面](https://github.com/z42-lang/z42/releases)，下载与你平台对应的 `z42-sdk-<版本>-<平台>.tar.gz`（Windows 是 `.zip`），同时下载 `SHA256SUMS`；
2. 校验：`shasum -a 256 -c SHA256SUMS --ignore-missing`；
3. 解压到任意目录，例如 `~/.z42`；
4. 把这个目录和它下面的 `bin` 目录加入 `PATH`。

## 常见问题

**运行 `z42` 提示 `command not found`（Windows 上是「无法识别」）**

安装脚本修改的是 shell 配置文件，只对**新打开**的终端生效。关掉当前终端重新打开即可。如果仍然不行，检查配置文件里是否有安装脚本加入的那一段；用了 `--no-modify-path` 时需要自己把 `~/.z42` 和 `~/.z42/bin` 加入 `PATH`。

**下载失败**

安装脚本从 GitHub 下载文件。网络无法访问 GitHub 时，可以在能联网的机器上下载 SDK 压缩包，拷贝过来后用 `--archive` 安装。

**提示 `Intel Macs are not supported yet`**

目前 macOS 只提供 Apple Silicon 版本。

## 下一步

z42 已经装好了。下一章我们创建第一个工程，让它在终端里打印出 `Hello, World!`。
