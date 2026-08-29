# 编译器构建（z42 自举）

z42 编译器 (`z42c`) 由 z42 自身写成，后端源码在 [`src/compiler/`](../../../src/compiler/)（3 个子包：`z42c.semantics` / `z42c.pipeline` / `z42c.driver`）；可移植前端 `z42c.core` / `z42c.syntax` 与 IR·后端库 `z42.ir` 已下沉 [`src/libraries/`](../../../src/libraries/)。自编译为 `z42c.driver.zpkg`（driver 入口）。

## 前置

- git + Rust stable + gh
- macOS / Linux / Windows 任一
- `z42c` 由 [`scripts/install-z42.sh`](../../../scripts/install-z42.sh) 装上 PATH（多数用户由此直接获得编译器，无需从源码自建）

## 从源码构建编译器

```bash
./xtask build compiler      # z42c 自举建后端 3 包（前端 z42c.core/syntax 由 build stdlib 建）
```

产物：`artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg` 是 driver 入口。多数用户由 `install-z42.sh` 直接提供 `z42c`，无需此步。

## 编译器入口（`z42c`）

### 单文件模式

```bash
z42c <file.z42> [--emit ir|zbc] [-o <out>]
```

> 命令需要 stdlib 时前缀 `Z42_LIBS="$PWD/.z42/libs"`。raw 单文件字节码也可 `z42vm z42c.driver.zpkg --emit-zbc <src> <out>`，但首选写 `z42c`。

| Flag | 用途 |
|------|------|
| `--emit zbc` | 产出 `.zbc` 字节码（VM 可直接执行） |
| `--emit ir` | 产出 `.zasm` 文本（调试查看 IR） |
| `-o <out>` | 输出路径 |

### 项目模式（`<name>.z42.toml`）

```bash
z42c build [<name>.z42.toml] [--release] [--bin <name>]
z42c check [<name>.z42.toml] [--bin <name>]
z42c run   [<path>] [--release] [--bin <name>] [--mode interp|jit|aot] [-- <program args>]
z42c clean [<name>.z42.toml]
```

> `run` 接受 `.z42.toml`(项目)或单个 `.z42`(脚本)。**`--` 之后的参数透传给被运行的 z42 程序**(经 `Std.IO.Environment.GetCommandLineArgs()` 读取),不被 z42c 解析。例:`z42c run app.z42 -- --verbose input.txt` → 程序看到 `["--verbose", "input.txt"]`。

manifest 字段定义见 [`docs/design/compiler/project.md`](../../design/compiler/project.md)；增量编译机制见 manifest 内 `[build]` + cache 行为说明。

### 工具命令

```bash
z42c disasm <file.zbc> [-o <file.zasm>]   # 反汇编 zbc → zasm 文本
z42c explain <ERROR_CODE>                 # E#### / W#### / WS### 详细说明
z42c errors                               # 列全部错误码
```

## 分发版 binary

把 z42vm + z42c 打到 `artifacts/build/runtime/release/`：

```bash
./xtask package sdk --profile debug   # debug
./xtask package sdk                    # release（默认）
```

详见 [`stdlib.md`](stdlib.md) 关于 stdlib 同步的描述。

## 单元测试

```bash
./xtask test compiler       # z42c 自举单测
```

见 [`../testing/unit-tests.md`](../testing/unit-tests.md)。
