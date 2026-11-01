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
| `vm.rs` | `Vm`：持有执行配置，从 VmContext 取得 Module，按 `ExecMode` 分发到 interp / jit / aot |
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
| `src/metadata/` | 静态包信息与尚在此目录的链接/加载实现（见下） |
| `src/object/` | 值、类型形状、布局、对象头、句柄及存储原语，见 [`src/object/README.md`](src/object/README.md) |
| `src/objops/` | 引擎无关的对象操作（字段 / 数组 / 静态字段 / struct 叶子的读写与错误通道），interp 与 JIT 共用，见 [`src/objops/README.md`](src/objops/README.md) |
| `src/interp/` | 字节码解释器，见 [`src/interp/README.md`](src/interp/README.md) |
| `src/jit/` | Cranelift JIT 后端，见 [`src/jit/README.md`](src/jit/README.md) |
| `src/corelib/` | 内置函数（builtin）实现，统一入口 `invoke_builtin` 供 interp / JIT 调用（builtin ABI v2：`corelib/abi/`），见 [`src/corelib/README.md`](src/corelib/README.md) |
| `src/gc/` | GC 子系统（`trait MagrGC` + 分代 / STW / 并发 / 增量 major），见 [`src/gc/README.md`](src/gc/README.md) |
| `src/vm_context/` | `VmContext`：单个 VM 实例所有可变状态的唯一持有者（静态字段（`static_table.rs`：每字段一个地址稳定、读写无锁的单元）、type / 方法解析缓存、isa cache、cctor、资源登记、native 句柄表） |
| `src/interop/` | Tier 1 C ABI 运行时 + stdlib native 扩展加载，见 [`src/interop/README.md`](src/interop/README.md) |
| `src/host/` | 嵌入 / 宿主 API（`z42_host.h` 的 Tier 1 C ABI 实现），见 [embedding.md](../../docs/internals/src/runtime/embedding.md) |
| `src/pal/` | 平台抽象层（OS 相关 `#[cfg]` 分支集中处），见 [`src/pal/README.md`](src/pal/README.md) |
| `src/exception/` | 异常对象布局、传播模型与栈迹捕获 |
| `src/thread/` | 线程支持（占位，无公开 API；线程原语在 `corelib/threading.rs` / `monitor.rs`） |

### src/metadata/ — IR 元数据与加载层
| 文件 | 职责 |
|------|------|
| `format/` | `mod.rs` 的 magic 与依赖记录、`versions.rs` 的严格版本校验、`build_id.rs` 的符号配对标识 |
| `reader/` | zbc / zpkg 二进制解码：节目录、指令、签名、类型、包目录与 sidecar；输出 IR 与调试记录 |
| `ir/` | `module` / `class` / `function` / `instruction` / `insn` 的解码结构与 `reg_type` 寄存器类型；Module / Function 只含静态定义；`exec_mode` 提供执行策略枚举 |
| `image.rs` | `PackageImage`：可跨 VM 共享的不可变定义快照，持有模块数据与函数定义的 Arc；不持有类型表或运行期缓存 |
| `index/` | `name` 字段/vtable 名称索引、`type_names` 类型名操作、`well_known_names` 常量与 `type_param_markers` 静态标记替换 |
| `debug/` | `symbols` 的源位置 / 局部变量 / DBUG 记录，`frames` 的帧名与源文件查询；sidecar 定位及覆盖由 loader 执行 |
| `loader.rs` + `loader/` | 统一加载入口 `load_artifact(fs, path)` → `Module`（从调用方给的 VFS 读）；`build_type_registry` 预构建 `TypeDesc` 注册表；`namespace` / `indices` / `constraints` / `availability` 等关注点子模块 |
| `lazy_loader.rs` + `lazy_loader/` | 惰性依赖加载（启动只载 `z42.core`，其余 zpkg 首次用到时加载：按 DEPS 引用表精确路由到定义包 `symbols.rs`，无静态引用的名字按命名空间前缀兜底 `resolve.rs`；加载与注册 `registry.rs`） |
| `merge.rs` | 多模块合并：字符串池重映射 + 函数拼接 |
| `resolver.rs` + `resolver/` | 加载期 token 解析（预填每函数 `ResolvedTokens`）+ 内联缓存（`ic.rs`） |
| `context.rs` | 加载上下文模型（`AssemblyLoadContext` 对等的代码边界抽象） |
| `tokens.rs` / `namespace_index.rs` / `vstr.rs` | 运行期 token 与目标键 / namespace→zpkg 索引 / GC 堆内不可变字符串句柄 |
| `func_table.rs` | `FuncTable`：VM 级函数身份（`FnId` → `Function`，读无锁）；入口模块函数 + 惰性包函数统一编号 |
| `type_table.rs` | `TypeTable`：VM 级类型身份（进程级 `TypeId` → 最新版本 `TypeDesc`，读无锁，稀疏分段）；入口模块类型 + 惰性包发布的类型；类型判定目标名的保留键 |
| `str_table.rs` | `StrTable`：VM 级字符串字面量身份（`ConstStr` 操作数 = 字符串 id → 文本 + 驻留的 GC 字符串，驻留后读无锁）；入口模块字符串池 + 惰性包追加的池统一编号，驻留串是 GC 根 |
| `seg_vec.rs` | `SegVec`（只追加、读无锁的分段向量）/ `SparseSegTable`（按 id 索引、段首次触达才分配的侧表） |
| `superinstr.rs` | 超级指令融合框架 |


### src/object/ — 对象模型

| 目录 | 职责 |
|------|------|
| `value/` | `Value`、引用/闭包/pin 载荷、codec、原子 cells、引用字、型参单元和 struct blob |
| `types/` | `TypeDesc` 形状、字段、布局计算、元素类型及泛型实参句柄 |
| `header/` | 16 B 区块头、标记位、`VarGcRef` / `ObjRef` / `ArrRef` 与弱句柄 |
| `object/` | `ScriptObject` / `NativeData`、字段原语和堆外 `OwnedObject` |
| `array/` | 数组模式、描述符、元素访问、视图与排序 |
| `string/` | `Str` 与非 ASCII 字符索引 |

GC 的 chunk 分配、代数与弱句柄 epoch 校验仍由 `gc/var_region` 管理。

### src/vm/runtime/ — 每 VM 的链接状态

`Module` / `Function` 共享 `PackageImage` 的静态定义，分别持有类型注册表与 FnId、解析、初始化、转发和站点缓存。
链接期修改静态字段使用写时复制；`Module::image()` 生成当前定义快照，旧映像保持不变。
`VmContext::with_image` 为同一映像构建独立的类型表与缓存；`new_with_core` 继续共享同一 VM 的状态。

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

`default = ["jit", "native-interop", "mimalloc-alloc", "net", "tls"]`（桌面）。平台预设经
`--no-default-features --features <wasm|ios|android>` 裁剪。

| feature | 作用 |
|---------|------|
| `jit` | Cranelift JIT 后端（desktop x64/arm64） |
| `native-interop` | Tier 1 原生扩展 ABI（dlopen + libffi） |
| `net` | `Std.Net` socket（TCP / UDP / DNS，Rust 标准库 net 模块 + `socket2`）。关掉时网络 builtin 抛 `NetUnsupportedException` |
| `tls` | 在 `net` 之上加 rustls TLS 客户端（`ring` 后端 + `webpki-roots`）。关掉时 `__tls_*` 抛 `NetUnsupportedException` |
| `mimalloc-alloc` | z42vm 二进制的 `#[global_allocator]` 走 mimalloc。z42c 自编译**分配受限**，换 mimalloc 后 z42c 编译显著提速。仅二进制生效（嵌入 lib 用宿主分配器）；wasm/移动预设不含（C 构建不入 wasm 沙箱 / 移动体积敏感） |

## 关联文档

- 用法与规则（参考手册）：[嵌入](../../docs/reference/src/embedding/README.md)
- 设计 / 机制（实现内幕）：[运行时](../../docs/internals/src/runtime/README.md) · [VM 总体架构](../../docs/internals/src/runtime/vm-architecture.md) · [产物格式](../../docs/internals/src/formats/README.md)
