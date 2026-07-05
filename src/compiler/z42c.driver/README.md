# z42c.driver

## 职责
镜像 C# [z42.Driver](../../compiler/z42.Driver/README.md)：CLI 入口（命令路由）。唯一 **exe** 子包，对外别名 = 用户 `z42c` 命令。绝不 fallback 到 dotnet z42c.dll。命令逐子版本解锁：**前端 dump 全实现**（`--dump-tokens`/`--dump-ast`/`--dump-bound`）+ **首个产物命令 `--emit-zbc`**（源 → IrGen → ZbcWriter → `.zbc` 文件，z42vm 可直接执行；ZW-1A/1B opcode 子集，无 DBUG）；manifest-check / build（zpkg）待后端落地。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Main.z42` | `void Main()`：读 `Environment.GetCommandLineArgs()`，路由 `--dump-tokens`/`--dump-ast` → `DumpTool`、`--dump-bound` → `SemanticDump`、`--emit-zbc <src> <out>` → `IrDump.ZbcBytes` + `File.WriteAllBytes`（`namespace Z42.Driver`）|

## 入口点
`Z42.Driver.Main`（auto-detected exe 入口）。
用法：`z42c --dump-tokens|--dump-ast|--dump-bound <file.z42>` / `z42c --emit-zbc <file.z42> <out.zbc>` /
`z42c build <project.z42.toml> [--release] [--no-incremental]` / `z42c build --workspace [--output-dir <d>]`。

## 增量编译（port-incremental-build-cache，2026-07-05）
单工程 `build` 逐文件写 fullMode `.zbc` 到 cache 目录（`[build].cache_dir` →
`${output_dir}/.cache` → `<projectDir>/.cache` 级联），并按「hash + cache zbc + TSIG」
整包 probe——全命中完全跳过重写（`no changes; preserved`），任一变化整包重编。
`--no-incremental` 强制全量；`Z42_INCR_DEBUG=1` 看逐文件 miss 原因。workspace/flat
模式不落 cache、不 probe（见 [project.md 增量编译节](../../../docs/design/compiler/project.md)）。

## 依赖关系
→ z42c.syntax, z42c.semantics, z42c.core。stdlib（Std / Std.IO）自动可用。

## 运行（自举产物）
z42c 跨包 dep 解析读 `Z42_LIBS`。**通常无需手动设置**：z42vm 会把它解析出的 libs 目录
（`<binary>/../libs` SDK 布局 / dev flat view）回写进 `$Z42_LIBS`，z42c 透明读到（见
vm-architecture.md 的「VM 启动流程」`libs_env_to_publish`）。SDK 安装后直接：
```
z42vm <programs/z42c>/z42c.driver.zpkg -- --emit-zbc <file.z42> <out.zbc>
z42vm <out.zbc> Main        # 执行自举编译器产物
```
仅当 libs 不在 VM 的默认搜索路径（如把 z42c 7 包 + stdlib 临时合到自定义 flat 目录）时，
才需显式 `Z42_LIBS=<flat>` 覆盖——此时**必须是单个**目录含全部依赖（见 self-hosting.md 的
Z42_LIBS 单目录陷阱）。端到端冒烟由 `xtask test compiler` 的 e2e 步骤覆盖（自检程序 +
div-by-zero oracle）。
