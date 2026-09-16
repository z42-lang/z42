# Std.Platform / Std.OperatingSystem 实现原理

> 2026-05-14 add-platform-os-stdlib · spec：
> [`docs/spec/archive/...-add-platform-os-stdlib/`](../../spec/) (pending archive)

z42 stdlib 中跨平台 OS / 架构识别、进程信息、主机信息的 stdlib 层与
runtime 实现。

## 架构

```
┌────────────────────────────────────────────────────────────────┐
│ z42 user code                                                  │
│   if (Platform.IsMacOS()) { ... }                              │
│   int n = OperatingSystem.CpuCount();                          │
└──────────────────────┬─────────────────────────────────────────┘
                       │ [Native("__name")] dispatch
                       ▼
┌────────────────────────────────────────────────────────────────┐
│ src/libraries/z42.core/src/                                    │
│   Platform.z42         Std.Platform (5 extern + 8 predicates)  │
│                        + Std.OSKind (8 const ints)             │
│                        + Std.ArchKind (5 const ints)           │
│   OperatingSystem.z42  Std.OperatingSystem (7 extern)          │
│ src/libraries/z42.io/src/                                      │
│   Environment.z42      + UnsetEnv / GetAll (在原 class 内追加)  │
└──────────────────────┬─────────────────────────────────────────┘
                       │
                       ▼
┌────────────────────────────────────────────────────────────────┐
│ src/runtime/src/corelib/                                       │
│   platform.rs  5 builtins → std::env::consts (compile-time)    │
│   system.rs    7 builtins → libc / std::env / std::process     │
│   fs.rs (+=)   __env_unset / __env_vars                        │
└────────────────────────────────────────────────────────────────┘
```

z42 facade 只做 `[Native("__name")]` 字符串映射；所有跨平台分支收敛在 corelib。

## 双形态共存：String + Enum + Predicate

z42 stdlib 同时提供 Rust-style 字符串和 .NET-style 枚举 / 谓词两种形态，
用户挑：

```z42
// Rust-style
if (Platform.OS() == "macos") { ... }

// .NET-style enum
int k = Platform.OSKindValue();
if (k == OSKind.MacOS) { ... }

// .NET-style predicate
if (Platform.IsMacOS()) { ... }
```

三种形态用同一个 builtin (`__platform_os_kind`) 返回的整数承载语义；
predicate 是 z42 stdlib 一层简单包装。整数常量值在 z42 stdlib 与
Rust corelib 双侧硬编码——z42 单测覆盖每个 CI OS verify
`Platform.OSKindValue() == OSKind.<expected>`，避免漂移。

## 跨平台差异表

| API | Unix (libc) | Windows | Wasm | 失败时 |
|---|---|---|---|---|
| `Platform.OS()` / `Arch()` / `Family()` | `std::env::consts::*`（compile-time） | 同左 | 同左 | 不会失败 |
| `OperatingSystem.CurrentPid()` | `std::process::id()` | 同左 | 同左 | 不会失败 |
| `ExecutablePath()` / `CurrentDirectory()` | `std::env::current_*` | 同左 | 同左 | 返回 `""` |
| `Hostname()` | `libc::gethostname` | **未实现**（返回 `""`）| 返回 `""` | 返回 `""` |
| `CpuCount()` | `std::thread::available_parallelism()` | 同左 | 同左 | 返回 `1` |
| `OsVersion()` | `libc::uname()` 三段组合 | **未实现**（返回 `""`）| 返回 `"wasm"` | 返回 `"" ` |

**Windows hostname / OS version 留 follow-up**：z42 CI 暂无 Windows runner，引入
`winapi` dep 没有 CI 验证不安全。一旦 Windows CI 落地，直接补
`GetComputerNameW` + `RtlGetVersion` 即可（[design.md decision 2](../../spec/archive/2026-05-15-add-platform-os-stdlib/design.md#decision-2-不引入-hostname--os_info-crate)）。

## OSKind / ArchKind 整数值表

| z42 常量 | 值 | Rust `std::env::consts::OS` | 备注 |
|---|---|---|---|
| `OSKind.Unknown` | 0 | 其他 | 不识别归并 |
| `OSKind.Linux` | 1 | `"linux"` | |
| `OSKind.MacOS` | 2 | `"macos"` | 不同于 .NET `OSX` |
| `OSKind.Windows` | 3 | `"windows"` | |
| `OSKind.Android` | 4 | `"android"` | |
| `OSKind.IOS` | 5 | `"ios"` | |
| `OSKind.Wasm` | 6 | `"wasm"` | wasm32-unknown-unknown |
| `OSKind.FreeBSD` | 7 | `"freebsd"` | |

| z42 常量 | 值 | Rust `std::env::consts::ARCH` | .NET `Architecture` |
|---|---|---|---|
| `ArchKind.Unknown` | 0 | 其他 | — |
| `ArchKind.X64` | 1 | `"x86_64"` | `X64` |
| `ArchKind.Arm64` | 2 | `"aarch64"` | `Arm64` |
| `ArchKind.Wasm` | 3 | `"wasm32"` | `Wasm` |
| `ArchKind.X86` | 4 | `"x86"` | `X86` |

> 命名采用 .NET 风格（`X64` / `Arm64` / `Wasm` / `X86`）而非 Rust target-triple
> 拼写（`x86_64` / `aarch64` / `wasm32`）：z42 lexer 处理 `X86_64` 这种字母-数字-下划线-数字混合标识符在类体内部有 token 切分问题，且 .NET 风格更短更易读。Arch
> 字符串本身（`Platform.Arch()`）仍按 Rust 值域返回（`"x86_64"` 等），保持
> 与底层 toolchain 一致。

> 加新 OS / Arch：两侧（`src/libraries/z42.core/src/Platform.z42` 的
> `OSKind` / `ArchKind` static class + `src/runtime/src/corelib/platform.rs` 的
> `match` 表）必须**一起改**。z42 单测会在每个 CI OS 上 assert 一致性。

## Future Work

- **Windows runner CI** 落地后，补 `GetComputerNameW` / `RtlGetVersion`
- **Linux 发行版识别** (`/etc/os-release` 解析)：独立 spec
- **物理 CPU 数 / NUMA**：当前只暴露 logical CPU count
- **`Map<string, string>` 形态的 GetEnvironmentVariables** 重载：等 z42 Map
  marshal 路径稳定
- **SemVer-shaped OsVersion**：返回 `Std.SemVer.Version` 而非字符串
