# z42 Runtime — Rust VM

## 职责

`src/lib.rs` 编译唯一的运行时包 `z42`。`src/` 内的 host、VM、对象、GC、PAL、config、corelib、interop 与 profiler 都是包内模块。`src/main.rs` 只通过版本化 C 入口动态加载运行时；CLI 实现在 `host/cli.rs`，与嵌入端共用 host 服务。Cargo 命令用 `-p z42` 选择运行时。

执行 z42 编译产物（`.zbc` / `.zpkg`）。后端：解释器（interp）+ Cranelift JIT（desktop 默认，惰性逐函数编译）；AOT 为桩。
不含编译器（在 `src/compiler/`）与标准库（在 `src/libraries/`）。

解释器与 JIT 分别属于独立的 z42-interp、z42-jit 库，单向依赖运行时；生产运行时不链接具体后端。host 统一选择静态/动态组件并注册引擎，每 VM 的 TierApi 归执行层，后端拥有编译器和代码页。

## 目录结构与核心文件

### 运行时与宿主入口
| 文件 | 职责 |
|------|------|
| `main.rs` | 动态加载运行时，检查 C 入口的 ABI 和构建身份 |
| `host/cli.rs` | 参数解析、启动策略和调用共用宿主服务 |
| `startup.rs` | `z42vm` 启动期辅助：stdlib 定位、tracing 初始化、panic hook、构建信息打印、模块搜索路径解析 |
| `corelib/builtins.rs` | 门面按既定 provider 顺序装配 builtin 描述符及纯名称/ID 目录 |
| `corelib/mod.rs` | 核心库提供 GetType 和默认 ToString 的对象协议绑定；宿主将其注入 VM |
| `host/artifact.rs` | 门面在函数融合准备时读取进程配置，为公开加载入口与每 VM loader 提供同一惰性输入 |
| `host/context.rs` | 门面实现既有 VmContext 主构造入口，装配配置、引擎、builtin 与扩展；worker 加入和 Drop 机制留在 vm/context |
| `host/extensions.rs` | 门面装配 interop 与核心库每 VM 资源状态，保留库句柄到最后释放，所有构造入口共用 |
| `vm/threads/state.rs` | `ThreadState` 拥有每 VM 线程注册、暂停握手与 fatal 状态，VmCore 组合持有 |
| `vm/runtime/state.rs` | `RuntimeState` 组合入口 module 与运行时身份表，保留每 VM 链接隔离 |
| `vm/loader/state.rs` | `LoaderState` 组合每 VM 加载上下文、初始化队列与 VFS |
| `vm/runtime/statics/` / `vm/exec/init.rs` | 静态存储/初始化事实与执行/等待的职责边界 |
| `host/services.rs` | 门面适配隔离应用执行与退出报告，经冻结扩展服务注入 corelib |
| `host/app_state.rs` | 门面拥有的应用参数，经核心库服务接口提供快照 |
| `vm/exec/thread_state.rs` | `ThreadExecState` 组合每线程帧、arena、异常槽和中立 JIT 头 |
| `vm/exec/state.rs` | `ExecState` 组合每 VM 的引擎注册与代码寿命 |
| `vm/config.rs` | `VmConfig`：门面捕获的构造输入，包含 heap 与采样设置；LOH 覆盖仍应用到进程策略 |
| `vm/ext.rs` | `VmExtensions` 创建前登记与冻结的每 VM Rust 扩展状态，`VmContext::ext` 借用查询 |
| `vm/mod.rs` | `Vm`：持有执行配置，从 VmContext 取得 Module，按 `ExecMode` 调用已注册引擎入口 |
| `host/app.rs` | `host::run_app`：加载 `.zbc`/`.zpkg` + 合并依赖 + 执行入口（z42vm / `z42_run_app` / wasm `runTestApp` 共用）|
| `host/boot.rs` | 合并后的启动步骤（`boot_context` / `prepare_execution`），`host::run_app` 与宿主 API（`host/ops.rs`）共用，防两条路径分叉 |
| `lib.rs` | 库入口，re-export 公开 API |
| `config/mod.rs` + `config/` | `RuntimeConfig`（每个 `Z42_*` 旋钮的单一登记处）：`config/mod.rs` 为 hub（Default / from_env / toml 加载 / 全局单例）；`config/knob_table.rs` 的 `KNOWN_KNOBS` 旋钮表、`knobs.rs`、`parse.rs`（各 `parse_*`）、`resolve.rs`（四层输入 + 默认的分层解析与 provenance）、`split.rs`（转换为子系统拥有的 `GcConfig` / `JitConfig` / `VmConfig`）、`source.rs`（`Z42_CONFIG` / `Z42_APP_CONFIG` 文件层）、`cli.rs`（`--set key=value`）、`availability.rs`（旋钮可用性）、`render.rs`（`--info` / `--list-knobs` / `--show-config` 渲染）；单测在 `config/config_tests.rs` |
| `vm/diagnostics/counters.rs` | `RuntimeCounters`：VM 可观测性原子计数器 |
| `vm/diagnostics/observer.rs` | `RuntimeObserver`：非 GC 运行时事件的 push 流（与 `gc::GcObserver` 对称） |
| `host/signal.rs` | 硬崩溃时捕获 z42 调用栈的 OS 信号 handler（调 `pal::signal`） |

### 子模块
| 目录 | 职责 |
|------|------|
| `src/vm/` | VM 统一入口、上下文及各子系统，见 [`src/vm/README.md`](src/vm/README.md) |
| `crates/z42-zpkg/src/` | 静态包信息、IR、映像与静态查询（见下） |
| `src/vm/threads/` | mutator 暂停、safepoint 与 GC 根服务适配，见 [`src/vm/threads/README.md`](src/vm/threads/README.md) |
| `src/vm/exec/` | 引擎共享的帧栈、寄存器池、arena、栈读取与标量指令语义，见 [`src/vm/exec/README.md`](src/vm/exec/README.md) |
| `src/vm/loader/` | 包加载、惰性依赖、定位和加载上下文，见 [`src/vm/loader/README.md`](src/vm/loader/README.md) |
| `src/vm/runtime/` | 每 VM 的身份、类型组装与链接缓存，见 [`src/vm/runtime/README.md`](src/vm/runtime/README.md) |
| `src/object/` | 值、类型形状、布局、对象头、句柄及存储原语，见 [`src/object/README.md`](src/object/README.md) |
| `src/vm/exec/ops/object/` | 引擎无关的对象操作（字段 / 数组 / 静态字段 / struct 叶子的读写与错误通道），interp 与 JIT 共用，见 [`src/vm/exec/ops/object/README.md`](src/vm/exec/ops/object/README.md) |
| `crates/z42-interp/src/interp/` | 字节码解释器，见 [`crates/z42-interp/src/interp/README.md`](crates/z42-interp/src/interp/README.md) |
| `crates/z42-jit/src/jit/` | Cranelift JIT 后端，见 [`crates/z42-jit/src/jit/README.md`](crates/z42-jit/src/jit/README.md) |
| `src/vm/diagnostics/` | VM 共享诊断状态：已结束线程的计数、运行时观察者、暂停分布、锁争用统计与采样器；`VmContext` 的观察 API 保持统一入口 |
| `src/corelib/` | 内置函数（builtin）实现，统一入口 `invoke_builtin` 供 interp / JIT 调用（builtin ABI v2：`vm/exec/call/builtin_abi/`），见 [`src/corelib/README.md`](src/corelib/README.md) |
| `src/gc/` | 具体 Heap 与分代 / STW / 增量 major 回收，见 [`src/gc/README.md`](src/gc/README.md) |
| `src/profiler/` | CPU 采样、堆摘要、V8 快照与保留链，见 [`src/profiler/README.md`](src/profiler/README.md) |
| `src/vm/context/` | `VmContext`（由 `vm` 统一导出）：固定地址的每线程视图，组合 exec/runtime/diagnostics/threads 状态并共享 Arc<VmCore> |
| `src/interop/` | Tier 1 C ABI 运行时 + stdlib native 扩展加载，见 [`src/interop/README.md`](src/interop/README.md) |
| `src/components/` | 统一组件协议、工厂加载、后端选择与每 VM 生命周期，见 [`src/components/README.md`](src/components/README.md) |
| `src/host/` | CLI 与外部宿主共用的 Rust 加载、调用及应用服务，包含 `z42_host.h` 的 C ABI 适配，见 [embedding.md](../../docs/internals/src/runtime/embedding.md) |
| `src/pal/` | 平台抽象层（OS 相关 `#[cfg]` 分支集中处），见 [`src/pal/README.md`](src/pal/README.md) |
| `src/vm/exec/exception/` | 异常对象布局、传播模型与栈迹捕获 |

### crates/z42-zpkg/src/ — 静态包信息
| 文件 | 职责 |
|------|------|
| `format/` | `mod.rs` 的 magic 与依赖记录、`versions.rs` 的严格版本校验、`build_id.rs` 的符号配对标识 |
| `reader/` | zbc / zpkg 二进制解码：节目录、指令、签名、类型、包目录与 sidecar；输出 IR 与调试记录 |
| `ir/` | `module` / `class` / `function` / `instruction` / `insn` 的解码结构与 `reg_type` 寄存器类型；Module / Function 只含静态定义；`exec_mode` 提供执行策略枚举 |
| `image.rs` | `PackageImage`：可跨 VM 共享的不可变定义快照，持有模块数据与函数定义的 Arc；不持有类型表或运行期缓存 |
| `index/` | `name` 字段/vtable 名称索引、`type_names` 类型名操作、`well_known_names` 常量与 `type_param_markers` 静态标记替换 |
| `debug/` | `symbols` 的源位置 / 局部变量 / DBUG 记录，`frames` 的帧名与源文件查询；sidecar 定位及覆盖由 loader 执行 |


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

### src/vm/loader/ — 包与加载上下文

| 目录 / 文件 | 职责 |
|------|------|
| `package/` | VFS 产物读取、模块合并和执行索引准备 |
| `locate/` | namespace 候选索引、依赖路径与 probing 目录展开 |
| `lazy/` | 按符号/namespace 路由的惰性依赖加载、注册与调试符号 |
| `contexts.rs` | 加载上下文和 assembly 身份、惰性卸载关联 |
| `availability.rs` | 加载期可用性折叠与不可达 CFG 剪枝 |

### src/vm/runtime/ — 每 VM 的链接状态

| 目录 | 职责 |
|------|------|
| `ids/` | token 与函数、类型、字符串身份表，读无锁的分段存储 |
| `types/` | 类型注册表组装、继承补全及约束验证 |
| `resolve/` | 每函数 ResolvedTokens 的预链接 |
| `dispatch/` | VCall / Field IC、IsaCache、函数/接收者/类型的纯派发查询与原子发布 |


`Module` / `Function` 共享 `PackageImage` 的静态定义，分别持有类型注册表与 FnId、解析、初始化、转发和站点缓存。
链接期修改静态字段使用写时复制；`Module::image()` 生成当前定义快照，旧映像保持不变。
`VmContext::with_image` 为同一映像构建独立的类型表与缓存；`new_with_core` 继续共享同一 VM 的状态。

### crates/ — Rust workspace 子 crate
静态产物组件库、native interop 三层 ABI 的 Rust 侧公开接口与宿主 / native 扩展 crate；详见 [`crates/README.md`](crates/README.md)。

| 子 crate | 职责 | 状态 |
|---------|------|------|
| `crates/z42-interp/` | 基线解释器、Engine 与组件工厂 | 在用 |
| `crates/z42-jit/` | Cranelift、tier/OSR、代码页与组件工厂 | 在用 |
| `crates/z42-zpkg/` | 静态产物格式、解析、IR 与不可变包映像；无 VM 依赖 | 在用 |
| `crates/z42-abi/` | Tier 1 C ABI 的 Rust `#[repr(C)]` 镜像（`no_std`，无依赖） | 在用 |
| `crates/z42-rs/` | Tier 2 用户面向 trait/type（`Z42Type`、`Z42Traceable`、`Visitor`） | 在用（高层能力待 C5） |
| `crates/z42-macros/` | proc macro 入口（`methods`、`module!`；`Z42Type` derive / `trait_impl` 待 C5，现报 `compile_error!`） | 在用 |
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

`default = ["cli", "interp", "jit", "native-interop", "net", "tls"]`（桌面）。平台预设经
`--no-default-features --features <wasm|ios|android>` 裁剪。

| feature | 作用 |
|---------|------|
| `cli` | 共用 CLI 策略、参数解析和动态进程入口；嵌入式平台预设不启用 |
| `jit` | 启用运行时 JIT 契约；具体 Cranelift 实现由独立组件提供（desktop x64/arm64） |
| `component-loading` | 通过 PAL 加载可选运行时组件；native-interop 会启用此能力 |
| `native-interop` | Tier 1 原生扩展 ABI（dlopen + libffi） |
| `net` | `Std.Net` socket（TCP / UDP / DNS，Rust 标准库 net 模块 + `socket2`）。关掉时网络 builtin 抛 `NetUnsupportedException` |
| `tls` | 在 `net` 之上加 rustls TLS 客户端（`ring` 后端 + `webpki-roots`）。关掉时 `__tls_*` 抛 `NetUnsupportedException` |
| `mimalloc-alloc` | 最终静态宿主 `z42vm_static` 选择 mimalloc；动态 SDK 使用共享 std 的 System 分配器 |

## 关联文档

- 用法与规则（参考手册）：[嵌入](../../docs/reference/src/embedding/README.md)
- 设计 / 机制（实现内幕）：[运行时](../../docs/internals/src/runtime/README.md) · [VM 总体架构](../../docs/internals/src/runtime/vm-architecture.md) · [产物格式](../../docs/internals/src/formats/README.md)

`xtask build runtime` 在同一 Cargo 图中构建动态运行时及独立后端。SDK 使用动态库；桌面 runtime 另外提供静态嵌入归档。`examples/z42.rs` 是本包的最终链接 target，启用 `static-embedding` 后生成统一命名的 C 静态库和动态库；iOS/Android 不再需要包装 Cargo 包。静态 Rust 宿主链接所需后端 rlib，并先调用 `host::register_static_component`；最终 C 嵌入库先调用 `z42_linked_components_init`。不支持 JIT 的 target 不编译 Cranelift，请求不可用后端明确报错。详见[组件化运行时](../../docs/internals/src/runtime/componentized-runtime.md)。
