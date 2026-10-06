# z42 Runtime — Rust VM

## 职责

执行 z42 编译产物（`.zbc` / `.zpkg`）。后端：解释器（interp）+ Cranelift JIT（desktop 默认，惰性逐函数编译）；AOT 为桩。
不含编译器（在 `src/compiler/`）与标准库（在 `src/libraries/`）。

## 目录结构与核心文件

### 顶层（`src/`）
| 文件 | 职责 |
|------|------|
| `main.rs` | `z42vm` CLI 入口，加载产物并交给 `Vm` 执行 |
| `startup.rs` | `z42vm` 启动期辅助：stdlib 定位、tracing 初始化、panic hook、构建信息打印、模块搜索路径解析 |
| `vm.rs` | `Vm`：持有 `Module`，按 `ExecMode` 分发到 interp / jit / aot |
| `app.rs` | `app::run`：加载 `.zbc`/`.zpkg` + 合并依赖 + 执行入口（z42vm / `z42_run_app` / wasm `runTestApp` 共用）|
| `boot.rs` | 合并后的启动步骤（`boot_context` / `prepare_execution`），`app::run` 与宿主 API（`host/ops.rs`）共用，防两条路径分叉 |
| `probing.rs` | zpkg 依赖的额外搜索目录展开（`Z42_PROBING_PATHS` / `[runtime] probing-paths`） |
| `lib.rs` | 库入口，re-export 公开 API |
| `config.rs` + `config/` | `RuntimeConfig`（每个 `Z42_*` 旋钮的单一登记处）：`config.rs` 为 hub（Default / from_env / toml 加载 / 全局单例）；`config/knob_table.rs` 的 `KNOWN_KNOBS` 旋钮表、`knobs.rs`、`parse.rs`（各 `parse_*`）、`resolve.rs`（四层输入 + 默认的分层解析与 provenance）、`source.rs`（`Z42_CONFIG` / `Z42_APP_CONFIG` 文件层）、`cli.rs`（`--set key=value`）、`availability.rs`（旋钮可用性）、`render.rs`（`--info` / `--list-knobs` / `--show-config` 渲染）；单测在 `config_tests.rs` |
| `semantics.rs` | 语言标量语义（算术 / 比较 / 数值转换）的单一真相源，interp / JIT / 常量折叠共同引用 |
| `counters.rs` | `RuntimeCounters`：VM 可观测性原子计数器 |
| `observer.rs` | `RuntimeObserver`：非 GC 运行时事件的 push 流（与 `gc::GcObserver` 对称） |
| `signal_handler.rs` | 硬崩溃时捕获 z42 调用栈的 OS 信号 handler（调 `pal::signal`） |
| `aot.rs` | AOT 后端桩（未实现） |

### 子模块
| 目录 | 职责 |
|------|------|
| `src/metadata/` | IR 元数据与加载层（见下） |
| `src/interp/` | 字节码解释器，见 [`src/interp/README.md`](src/interp/README.md) |
| `src/jit/` | Cranelift JIT 后端，见 [`src/jit/README.md`](src/jit/README.md) |
| `src/corelib/` | 内置函数（builtin）实现，统一入口 `exec_builtin_by_id` 供 interp / JIT 调用，见 [`src/corelib/README.md`](src/corelib/README.md) |
| `src/gc/` | GC 子系统（`trait MagrGC` + 分代 / STW / 并发 / 增量 major），见 [`src/gc/README.md`](src/gc/README.md) |
| `src/vm_context/` | `VmContext`：单个 VM 实例所有可变状态的唯一持有者（静态字段、type / 方法解析缓存、isa cache、cctor、资源登记、native 句柄表） |
| `src/native/` | Tier 1 C ABI 运行时 + stdlib native 扩展加载，见 [`src/native/README.md`](src/native/README.md) |
| `src/host/` | 嵌入 / 宿主 API（`z42_host.h` 的 Tier 1 C ABI 实现），见 [embedding.md](../../docs/internals/src/runtime/embedding.md) |
| `src/pal/` | 平台抽象层（OS 相关 `#[cfg]` 分支集中处），见 [`src/pal/README.md`](src/pal/README.md) |
| `src/exception/` | 异常对象布局、传播模型与栈迹捕获 |
| `src/thread/` | 线程支持（占位，无公开 API；线程原语在 `corelib/threading.rs` / `monitor.rs`） |

### src/metadata/ — IR 元数据与加载层
| 文件 | 职责 |
|------|------|
| `types.rs` + `types/` | 运行时值类型与对象模型：`field`（FieldSlot / TAG_*）、`type_desc`（TypeDesc / Cold）、`layout` / `codec`（字节布局与编解码）、`object` / `obj_storage`（ScriptObject / NativeData）、`array` / `array_access`（ArrayObj）、`value` / `value_aux`（Value / ExecMode / Closure 数据）；hub 全量 `pub use` |
| `bytecode.rs` + `bytecode/` | zbc IR 数据结构：`module`（Module）、`class`（ClassDesc / FieldDesc / 布局描述 / CLASS_FLAG_*）、`function`（Function / BasicBlock / 异常表）、`insn`（*Insn 载荷）、`instruction`（Instruction / Terminator）；`bytecode_serde.rs` 为 TypedReg 兼容 serde |
| `formats.rs` | `.zbc` / `.zpkg` magic 常量 + 依赖记录 `ZpkgDep` |
| `zbc_reader/` | zbc / zpkg 二进制读取：`cursor` / `opcodes` / `instr_decode` / `func_reader` / `type_reader` / `zpkg` / `zpkg_index` / `sidecar` / `versions` |
| `loader.rs` + `loader/` | 统一加载入口 `load_artifact(path)` → `Module`；`build_type_registry` 预构建 `TypeDesc` 注册表；`namespace` / `indices` / `constraints` / `availability` 等关注点子模块 |
| `lazy_loader.rs` + `lazy_loader/` | 惰性依赖加载（启动只载 `z42.core`，其余 zpkg 按命名空间首次引用时加载） |
| `merge.rs` | 多模块合并：字符串池重映射 + 函数拼接 |
| `resolver.rs` + `resolver/` | 加载期 token 解析（预填每函数 `ResolvedTokens`）+ 内联缓存（`ic.rs`） |
| `context.rs` | 加载上下文模型（`AssemblyLoadContext` 对等的代码边界抽象） |
| `tokens.rs` / `name_index.rs` / `namespace_index.rs` / `vstr.rs` | 热路径 token 新类型 / 字段·vtable 名称索引 / namespace→zpkg 索引 / GC 堆内不可变字符串句柄 |
| `superinstr.rs` | 超级指令融合框架 |
| `test_index.rs` / `build_id.rs` / `well_known_names.rs` / `ir_type.rs` | 编译期测试发现 TIDX 段 / 分离调试符号的 build id / 常用限定名常量 / 寄存器类型 tag |

### crates/ — Rust workspace 子 crate
native interop 三层 ABI 的 Rust 侧公开接口与宿主 / native 扩展 crate；详见 [`crates/README.md`](crates/README.md)。

| 子 crate | 职责 | 状态 |
|---------|------|------|
| `crates/z42-abi/` | Tier 1 C ABI 的 Rust `#[repr(C)]` 镜像（`no_std`，无依赖） | 在用 |
| `crates/z42-rs/` | Tier 2 用户面向 trait/type（`Z42Type`、`Z42Traceable`、`Visitor`） | 在用（高层能力待 C5） |
| `crates/z42-macros/` | proc macro 入口（`methods`、`module!`；`Z42Type` derive / `trait_impl` 待 C5，现报 `compile_error!`） | 在用 |
| `crates/z42-host/` | Tier 2 宿主进程内嵌入 API（workload host facade） | 在用 |
| `crates/z42-compression/` | 压缩后端 cdylib（`z42.compression` 的 native 侧） | 在用 |
| `crates/z42-repl/` | host-only REPL 行编辑器 cdylib（`z42i`，被 `z42vm` 懒 dlopen） | 在用 |

> apphost 的**进程外**运行时解析在桌面 apphost 桩的 `hostrun` 模块
> （`src/toolchain/workload/desktop/platform/apphost/src/hostrun.rs`），不是本 workspace 的 crate。

C 头文件位于 [`include/z42_abi.h`](include/z42_abi.h)；`.z42abi` manifest schema 在 [`docs/internals/src/formats/manifest-schema.json`](../../docs/internals/src/formats/manifest-schema.json)。

## 构建与测试

`tests/` 存放跨模块集成测试（`zbc_compat.rs` / `native_interop_e2e.rs` / `signal_handler_e2e.rs` / GC loom 模型测试等）；`benches/` 见 [`benches/README.md`](benches/README.md)。

```bash
xtask build runtime                           # cargo build z42vm + libz42（--release）
(cd src/runtime && cargo test --workspace)   # 全部 Rust 单测与集成测试
xtask test runtime                            # 同上（xtask 串行封装）
xtask test e2e                                # VM golden 端到端（interp + jit）
```

### Cargo features

`default = ["jit", "native-interop", "mimalloc-alloc"]`（桌面）。平台预设经
`--no-default-features --features <wasm|ios|android>` 裁剪。

| feature | 作用 |
|---------|------|
| `jit` | Cranelift JIT 后端（desktop x64/arm64） |
| `native-interop` | Tier 1 原生扩展 ABI（dlopen + libffi） |
| `mimalloc-alloc` | z42vm 二进制的 `#[global_allocator]` 走 mimalloc。z42c 自编译**分配受限**，换 mimalloc 后 z42c 编译显著提速。仅二进制生效（嵌入 lib 用宿主分配器）；wasm/移动预设不含（C 构建不入 wasm 沙箱 / 移动体积敏感） |
