# 平台识别的脚本表面（Std.Platform / Std.OperatingSystem）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/libraries/z42.core/src/Platform.z42`、`src/libraries/z42.core/src/OperatingSystem.z42`、
> `src/runtime/src/corelib/platform.rs`、`src/runtime/src/corelib/system.rs`、
> `src/runtime/src/corelib/builtin_table_ext.rs`（注册）、`src/runtime/src/pal/system.rs`
>
> Rust 侧 `#[cfg]` 该收敛到哪里 → [PAL 平台抽象层](pal.md)；
> 一个二进制里编进了哪些 feature、怎么构建各平台产物 → [跨平台](cross-platform.md)。

脚本问「我在什么 OS 上 / 这个 VM 能干什么」，答案从 `Std.Platform` 与 `Std.OperatingSystem`
两个静态类出来。本页讲这条链：facade 为什么不做任何分支、**同一份整数契约为什么在两种语言
里各写了一份**、wasm 那个唯一不能直通的常量，以及能力查询（`Capabilities()` / `ExecModes()`）
凭什么是「这个二进制的事实」而不是猜。加一个 OS、加一个能力位、或者动这些 builtin 之前读本页。

## 1. 链路：facade 零分支，分支全在 corelib

```
z42 脚本          Platform.IsMacOS() / OperatingSystem.Hostname()
   │
   ▼  z42.core 的两个静态类：[Native("__…")] 名字映射 + 纯 z42 的便利谓词，无平台分支
   │    Platform.z42            Std.Platform / Std.OSKind / Std.ArchKind
   │    OperatingSystem.z42     Std.OperatingSystem
   ▼  builtin 分发表：corelib/builtin_table_ext.rs（__platform_* 7 条 + __system_* 7 条）
   │
   ▼  corelib/platform.rs   ← std::env::consts::{OS,ARCH,FAMILY}（编译期常量）+ cfg(feature)
      corelib/system.rs     ← std::process / std::env / std::thread + pal::system
                                 │
                                 ▼  pal/system.rs：hostname() / os_version() 的 unix·wasm·其他三分支
```

facade 侧唯一的「逻辑」是谓词：`IsLinux()` 等 7 个 OS 谓词、`IsUnix()`（5 个 kind 的或）、
以及 4 个能力谓词，全部是对同一个 builtin 的一层包装，各自每调一次就再问一次 builtin——
热路径上要自己缓存。

## 2. 三种形态、一个真相

同一件事故意给了三种问法，让 Rust 习惯和 .NET 习惯的人都顺手：

```z42
if (Platform.OS() == "macos") { … }          // 字符串，值域与 Rust target triple 一致
int k = Platform.OSKindValue();               // 整数，便于 switch 式分派
if (k == OSKind.MacOS) { … }
if (Platform.IsMacOS()) { … }                 // 谓词
```

字符串形态出自 `__platform_os`，整数与谓词形态出自 `__platform_os_kind`，两者读同一个
`HOST_OS` 常量——所以三种形态不会各说各话。`Arch()` 同理返回 Rust 值域拼写（`x86_64` /
`aarch64` / `wasm32`），而 `ArchKind` 的**常量名**用 .NET 风格短名（`X64` / `Arm64` / `Wasm` /
`X86`）：`X86_64` 这种「字母-数字-下划线-数字」标识符在类体内会被 lexer 切错，短名同时也更好认。
名字只是名字，跨语言对齐的是整数值。

## 3. 整数契约双写，改一处必须改两处

`OSKind` / `ArchKind` 的值在 z42 侧（`Platform.z42` 的两个静态类）和 Rust 侧
（`corelib/platform.rs` 的两个 `match`）**各硬编码一份**：

| `OSKind` | 值 | `HOST_OS` |
|---|---|---|
| `Unknown` | 0 | 其他一切（归并） |
| `Linux` | 1 | `linux` |
| `MacOS` | 2 | `macos` |
| `Windows` | 3 | `windows` |
| `Android` | 4 | `android` |
| `Ios` | 5 | `ios` |
| `Wasm` | 6 | `wasm` |
| `FreeBSD` | 7 | `freebsd` |

| `ArchKind` | 值 | `std::env::consts::ARCH` |
|---|---|---|
| `Unknown` | 0 | 其他一切 |
| `X64` | 1 | `x86_64` |
| `Arm64` | 2 | `aarch64` |
| `Wasm` | 3 | `wasm32` |
| `X86` | 4 | `x86` |

加一个 OS / arch = 两侧同时加，否则脚本侧的常量与 builtin 返回的整数会悄悄错位。

⚠️ **这条契约目前没有自动守门**。`src/libraries/z42.io/tests/platform.z42` 只验**自洽**
（字符串非空、至多一个 `IsXxx()` 为真、`k == OSKind.Linux ⇒ IsLinux()`）——而谓词本身就是
拿同一个整数比出来的，所以哪怕 Rust 侧把 macOS 映射成 3，全套测试照绿。要真守住，得有一个
「本 CI OS 上 `OSKindValue()` 必须等于某个具体值」的断言。

## 4. wasm：唯一不能直通的那个常量

其余平台 `Platform.OS()` 就是 `std::env::consts::OS`，**wasm32 例外**——
`wasm32-unknown-unknown` 的 `consts::OS` 是空串而不是 `"wasm"`。所以 `corelib/platform.rs`
顶上有一个覆写常量：

```rust
const HOST_OS: &str = if cfg!(target_arch = "wasm32") { "wasm" } else { std::env::consts::OS };
```

整条 OS 识别链都吊在它身上：`OS()`、`OSKindValue()`、`IsWasm()`，以及测试框架的
`[Skip(platform: "wasm")]`（`z42.test` 的 `Runner._skipApplies` 直接拿 `Platform.OS()` 与
属性里的字符串比）。绕过 `HOST_OS` 直接写 `consts::OS` 就会让这四样在 wasm 上同时失效，而
wasm 上「`IsWasm()` 恒假」不会报错，只会安静地少跳过一批测试。

## 5. 能力查询：编译进来了什么，而不是平台大概有什么

`Platform.Capabilities()` 与 `Platform.ExecModes()` 返回字符串数组，值来自**本二进制的 cfg**：

| 能力位 | 来源 |
|---|---|
| `jit` / `native-interop` / `bundled-compression` | cargo feature |
| `threads` | `cfg(not(target_arch = "wasm32"))`——真 OS 线程随 `std::thread` 存在，**不是** feature |
| `socket` | 同上：真 OS 网络（TCP/UDP/HTTP/WS）随 `std::net` 存在 |

`ExecModes()` 恒含 `interp`，`jit` / `aot` 按 feature 追加。注意 **`aot` 出现在列表里只说明
「编进来了」，不说明「跑得动」**——AOT 后端仍是桩；「能不能真跑」由 exec-profile 支持矩阵的
策略层另判。数组顺序稳定（按声明序），所以探针输出可直接比对。

两个消费者：

- `z42.test` 的 `[Skip(feature: X)]`——读作「这个测试需要能力 X，缺 X 就跳过」，
  `Runner._capsHas` 在 `Capabilities()` 里线性找；**未知 feature 不在数组里 ⇒ 判为缺失 ⇒
  跳过**（deny-by-default，拼错一个能力名的后果是静默跳过，不是报错）。
- exec-profile 的 bench / 测试编排：探测**目标** VM 二进制，让每条测量结果带上它自己报告的
  能力，而不是按平台猜。

本机实测（macOS / aarch64 / 默认 feature 集）：`Capabilities() = [jit, native-interop,
threads, socket]`、`ExecModes() = [interp, jit]`。

## 6. 各 API 的平台退化

约定是**优雅降级**：不可用就返回空串 / 0 / 1 这类中性值，不 panic、不抛。

| API | unix | wasm32 | 其他（今天 = Windows） |
|---|---|---|---|
| `Platform.OS()` / `Arch()` / `Family()` | 编译期常量 | `OS()` 走 `HOST_OS` 覆写 | 编译期常量 |
| `OperatingSystem.CurrentPid()` | `std::process::id()` | `0` | 同 unix |
| `ExecutablePath()` | `std::env::current_exe()`，失败 `""` | `""` | 同 unix |
| `CurrentDirectory()` | `std::env::current_dir()`，失败 `""` | `"/"` | 同 unix |
| `SetCurrentDirectory()` | `std::env::set_current_dir`（失败向上抛） | no-op | 同 unix |
| `Hostname()` | `libc::gethostname` | `""` | `""`（未实现） |
| `CpuCount()` | `available_parallelism()`，失败 `1` | 同左 | 同左 |
| `OsVersion()` | `libc::uname` 的 `sysname release version` 三段拼接 | `"wasm"` | `""`（未实现） |

Windows 的 `hostname` / `os_version` 留着 `None` / `""` 是有意的：引入 `winapi` 依赖而没有
Windows CI runner 验证，等于把未验证代码塞进 PAL；等 runner 到位再补
`GetComputerNameW` / `RtlGetVersion`，`pal::system` 的签名不用动。

⚠️ 分支位置不齐：`hostname` / `os_version` 老老实实走 `pal::system`，但 `pid` / `exe_path` /
`cwd` / `set_cwd` 的 wasm 分支是**就地写在 `corelib/system.rs` 里的** `#[cfg]` 块（wasm 上
这四个 `std` 调用会 panic "not supported"，必须挡）。这与 PAL 的「consumer 零 cfg」不变量
相抵；要收，应该往 `pal::system` 里加 `pid()` / `exe_path()` / `cwd()` / `set_cwd()` 四个
OS-neutral 函数。

## 7. 相邻的环境变量与文件系统

同属「平台相关的脚本表面」、但不在这两个类里的两块：

- **环境变量 / 进程**：`Std.Environment`（`src/libraries/z42.core/src/IO/Environment.z42`）的
  `__env_get` / `__env_set` / `__env_unset` / `__env_vars` / `__env_args` /
  `__env_get_cwd` / `__env_set_cwd` 实现全在 `corelib/fs.rs`，注册分散在
  `builtin_table.rs` 与 `builtin_table_ext.rs` 两张表里。
- **文件系统**：`corelib/fs.rs` 的路径型 builtin 一律走
  `corelib/fs_backend::active()`，两个实现——`native`（`std::fs`）与 `memory`（`path → bytes`
  的内存 VFS，宿主挂载 zpkg）。默认按 `cfg(target_arch)` 选（wasm → Memory），`set_backend`
  可在运行期切，所以「VFS 与磁盘行为一致」这类测试能在 native 上跑。加 WASI / 移动端后端 =
  多一个模块 + 多一组 match 分支，builtin 本身不动。

## 8. 加一个 OS / 加一个能力

- **加 OS 或 arch**：`Platform.z42` 的 `OSKind` / `ArchKind` 静态类加常量 →
  `corelib/platform.rs` 对应 `match` 加分支（值必须一致）→ 需要谓词就在 `Platform.z42` 加
  `IsXxx()`，属于 unix 家族的还要进 `IsUnix()` 的或链。
- **加能力位**：`builtin_platform_caps` 里按 cfg push 一个新字符串（feature 用
  `#[cfg(feature=…)]`，非 feature 的真实能力用 `#[cfg(target_arch=…)]` 这类直接判定）→
  想要谓词就在 `Platform.z42` 加一个 `HasXxx()` + 在 `z42.core/tests/platform_capabilities.z42`
  加一条「谓词与数组一致」的不变量测试。**别在 z42 侧硬编码能力名清单**——脚本只应该问数组。
