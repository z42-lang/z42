# 组件化运行时（Componentized Runtime）

> 状态：静态产物层和通用组件协议已独立；每 VM 静态/动态装配框架已实现。执行后端和诊断组件的独立交付仍待实施。
>
> 本文是 z42 运行时**未来的组件化目标架构**：把执行后端、GC、调试等从单一 `z42` crate 中拆成可独立交付的组件，支持「独立 z42vm 运行时按需加载」与「嵌入按需链接」。
>
> 组件拆分按静态产物、共享核心、执行后端和可选诊断组件推进。静态链接和运行时加载共用宿主门面；`workload-distribution.md` 的 Deferred `runtime-future-jit-cdylib-split` 指向后端独立交付。
>
> 当前运行时架构（单 crate + feature gate）见 [vm-architecture.md](vm-architecture.md)；运行时/包分发见 [workload-distribution.md](../toolchain/workload-distribution.md)。

---

## 1. 目标

把运行时拆成可独立交付的组件，实现两种「按需」：

1. **独立 z42vm**：重组件（JIT/AOT/调试）**运行时按需 dlopen**，基线二进制小。
2. **嵌入**：宿主**编译期按需链接**只要的组件（模块化 staticlib / dylib）。

**非目标**：① 不为性能（拆分不得让 JIT 热路径退化）；② 不支持第三方组件（组件 ABI 是内部契约，随包同构建同发）；③ 初期接受少量磁盘代价换清晰边界。

---

## 2. 两条交付轴（必须分开看）

| 轴 | 机制 | 消费者 |
|---|---|---|
| 独立 z42vm | 运行时 **dlopen** 组件 cdylib | 跑 `.zbc` 的命令行工具 |
| 嵌入 | 构建期 **按需 link** staticlib / dylib（feature 矩阵）| 把 VM 嵌进 app 的开发者 |

两条轴用**同一套组件库**，只是绑定方式不同（见 §6）。

---

## 3. 组件清单与「切换类别」

并非每个组件都能运行时切换——由其**耦合热度**与**是否绑定全局共享 ABI**决定。

| 组件 | 拆成 | 运行时可切（动态）？ | 理由 |
|---|---|---|---|
| 值模型 / 堆与对象头布局 / loader / 类型注册 / host C ABI / **注册槽 + observer 框架** | **libz42 基座（不可再拆）** | — 永远在 | 所有组件共享的契约 |
| **interp** | libz42_interp（或留基座作基线） | ✅ 可作后端 | 模块入口处派发；是 jit 的回退基线 |
| **jit** | libz42_jit | ✅ | 模块入口派发；helper 地址 finalize 时 baked |
| **aot** | libz42_aot | ✅（跑 AOT 镜像） | 同后端模型（M9 后） |
| **gc** | libz42_gc | ❌ **仅编译期可插拔** | 在最热路径（每次分配 / 读写屏障 / safepoint），且 interp 与 jit 编译出的机器码都按**固定对象头布局**访问堆——换 GC = 换一套全局共享 ABI，dlopen 热路径换 GC 不现实（类比 Rust global allocator：构建期定，不运行时换） |
| **调试组件** | libz42_debug | ✅ | 经 observer 钩子注册，仅调试时加载；不在最内层热循环 |

**精炼分层**：
- **libz42 基座** = 收敛到「共享契约」：值/堆布局、loader、类型注册、host ABI、**注册槽 + observer 框架**。
- **最小可用运行时** = 基座 + **interp + gc**（一个引擎 + 内存管理）。
- **可选叠加** = jit / aot / debug。

---

## 4. 依赖反转与无环

### 4.1 当前库边界与引擎注册

`z42-zpkg` 拥有格式、解码、静态 IR、调试记录和不可变包映像，只依赖通用错误与哈希库，不依赖 VM。执行身份、链接、缓存和初始化状态留在运行时。它是静态链接的基础库，解析符号记录不要求加载调试器。

执行后端目前仍属于主运行时 crate。门面 `assembly/engine.rs` 按 feature 装配 Engine 与优化入口；`VmCore.exec.engines` 持有每 VM 注册表和一次发布的 TierApi。VM 的模式分发、初始化执行及共享 ToString 调用均通过注册能力，不静态选择具体后端。

面向嵌入宿主的接口和类型统一从 `host` 提供，具体实现可以属于独立组件。CLI 使用相同门面。组件间的内部 Rust API 不自动成为宿主 API；静态链接与动态加载不能要求宿主改换调用接口。

CPU/堆分析器和调试代理属于可选组件，通过核心的观察者、采样和诊断接口接入。核心保留共享事件契约及执行所需状态，不因未加载工具组件而引入其实现。调试器与 profiler 可以按需启用；产物符号解析和崩溃栈定位属于基础能力，不能随调试器移除。

interp 的静态、虚、闭包和通用混合入口传入共享参数视图；TierBackend 的 JIT 实现拥有 JitRuntime、JitFrame、编译器与代码。OSR 的计数和阈值比较留在解释器热循环，实际交接经 TierApi。JIT 的冷/不可翻译调用经已注册 Engine 回落，两个后端的生产代码互不引用。

派发纯事实与缓存归 `vm/runtime/dispatch`；可能加载、初始化或物化异常的执行编排归 `vm/exec/ops`。同一帧链与栈读取面归 VM，具体后端只持自己的激活。代码与 worker 共享的注册状态由所属 VmCore 保活，各 VM 独立。VmContext 内联组合 exec/runtime/diagnostics/threads 的每线程状态，所属线程的帧、异常槽、缓存与暂停计数不会因共享 Core 而共享。

BuiltinCatalog 在 runtime 保存纯名称/ID 与折叠属性，exec 保存 callable 表并派发；门面从 corelib / interop 描述符装配两者，不让 runtime 引入执行 ABI 或具体实现。默认目录不可变，现有 native 符号按 interop 原时序加载。

JIT 的安装输入为后端拥有的 JitConfig，门面负责配置转换和注册适配；分层/编译/翻译仅使用已传入的值。该内部输入是同构建 Rust API，不是动态库 ABI。

VM 的内部构造输入为 `vm/config.rs` 的 `VmConfig`，由门面从已决议的 `RuntimeConfig` 捕获 GC、预算、采样与 trace 设置。内部构造消费纯值，不解析环境变量或门面旋钮；既有构造入口负责转换。`process_loh_bytes` 明确表示构造期应用的进程级 LOH 覆盖，仍在 core 建立后应用，不提供每 VM LOH 隔离；safepoint 与 stackalloc 的进程策略也保持独立。该输入不存入 VmCore，不改变线程视图布局。

Loader 拥有私有 `FusionInputs`，由门面的惰性函数指针在每次实际计算融合侧表时提供。它沿用进程级 `RuntimeConfig` 的首次读取语义。公开加载入口、可用性探测以及每 VM 的惰性加载和可回收程序集上下文使用同一配置来源。装配只传递指针，模块就绪判断、解码错误和融合识别顺序保持原样。

主 VM 构造由 crate 门面 `assembly/context.rs` 实现既有 `VmContext` 入口，负责配置捕获、组件选择与 core 接线。`vm/context/construct.rs` 保留线程状态访问、共享 core 的 worker 加入和 Drop；worker 不重新选择组件或重建扩展。主构造的弱引用扫描器、native 扩展加载、固定地址登记及 TLAB/SATB 顺序保持一致，扩展资源随最后一个持有 core 的上下文释放。

核心库的隔离应用执行与退出报告经 `corelib/application.rs` 的私有服务接入，门面 `host/services.rs` 负责应用选项与报告策略。冻结前注入的服务只含函数指针，不拥有 VM 或托管值；并行用例只复制服务值与 Rust 路径/VFS。核心库仍负责 stdout sink 栈、按输入排序和 join 时 park，进程退出仍先报告再终止且不运行析构。

值的无上下文格式化与盒装 struct 哈希由 `object/value` 的 crate 内部叶子模块定义，VM、解释器、JIT 与异常展示直接消费。`corelib::convert` 的既有公开格式化路径直接再导出同一定义；typed builtin 保留参数检查、错误和装箱编排。64 字节栈缓冲、UTF8 完整片段、spill、标量文本、FNV 顺序与盒装值借用边界保持一致；用户 ToString 协议和托管对象物化不属于这些纯值原语。

执行层的对象协议由 `vm/exec/object_protocol.rs` 定义，门面 `assembly/object_protocol.rs` 将核心库既有 GetType 和默认 ToString 函数直接绑定到每 core 的不可变服务。服务只存函数指针，不持有 VM 或托管值，worker 共享同一注册。装箱 GetType 在原派发分支内传入完整接收者，反射实现仍在调用时物化托管对象；引用对象的默认 ToString 返回 Rust String，执行调用方仍在原位置分配托管字符串并处理 BuiltinError。用户 override、纯值格式化和普通 builtin 的派发路径不经此默认回落。

### 4.2 独立交付仍是目标设计

注册边界见 [VM 总体架构](vm-architecture.md)「引擎注册与调用边界」。通用组件 ABI 与静态/动态装配已经实现，具体协议见下文「当前组件框架」。现有 EngineRegistry/TierApi 仍是 crate 内 Rust 接口，尚未接入该 ABI；后端独立交付须维持每 VM 所有权边界，不能改成进程全局 JIT 单例。

目标依赖仍是组件依赖 core，core 经中立注册能力调用组件。现有模块化注册不表示 `libz42_jit` 已可独立 dlopen。

---

## 5. 切换语义：可用集 vs 选用

两个独立维度，勿混：
- **可用集（availability）**：这个部署里**存在哪些组件**。
- **选用（selection）**：某次运行**用哪个**（`--mode interp|jit`、`--debug`），在模块入口处定。`--mode jit` 下函数先解释执行、到调用阈值再编译，热循环经 OSR 中途转入原生码——这些都在 jit 组件内部完成（见 [jit.md](jit.md)），不改变「可用集 / 选用」这两个维度。

interp 是基座基线（近乎永远可用）；jit/aot/debug 是可选可切的。规则：

| 交付方式 | 可用集何时定 | 能否切换 |
|---|---|---|
| **动态（dlopen / 动态链接）** | 运行时 / 加载期——看 dylib 在不在 | ✅ 丢一个 dylib 即增减；`--mode` 启动时选；缺则清晰报错 |
| **静态链接** | 编译/链接期——链进去什么就是什么 | ❌ 冻结：静态 interp-only → JIT 永不可用，须重链；静态 interp+jit → 运行时能在二者间选，但集合固定 |

一句话：**动态 = 可用集运行时开放、可增可换；静态 = 链接期冻结，只能在已链入的之间选（或没得选）**。GC 永远落在「静态/冻结」类（§3，本质决定）。

---

## 6. 静态与动态绑定

组件使用同一种 `ComponentInit` C 函数签名，返回不可变工厂描述符；具体能力通过独立接口表提供。

| 来源 | host 配置 | 自动装配时机 |
|---|---|---|
| 静态链接 | `ComponentOptions::with_static(id, init)` 保存直接函数引用，防止未引用入口被链接器删除 | `LoadedProgram` 准备执行环境后调用 init 并创建实例 |
| 动态库 | `ComponentOptions::with_dynamic(id, path, symbol)` 保存可信原生库与入口 | PAL 打开库、解析 init，再走同一装配流程 |

不依赖全局构造器或目录枚举顺序注册。静态清单与动态选择由宿主明确给出；将来 CLI 的编译配置和 runtime pack 清单可以生成这些选项。
组件来源决定可用集，执行模式只在已装配引擎中选择。当前 CLI/C 宿主尚未提供组件选择参数。

引擎适配后的注册调用用于模式入口、跨后端回落和 tier/OSR 交接；原生同后端调用与 JIT baked helper 地址继续使用自己的调用约定。性能以最终构建的 A/B 门禁验证。

---

## 7. 核心代码只保留一份

Rust `staticlib` 会包含依赖；`cdylib` 依赖核心的 rlib 时也可能静态带入另一份核心。
不能通过给每个插件依赖整个 VM 的方式拆库，否则配置单例、堆注册表、TLAB/SATB TLS 都可能重复。
不能依赖可执行文件的 Rust 符号可见性补救这一所有权问题。

当前独立动态测试组件只依赖 `z42-component-api`，经 RuntimeApi 回调访问核心，不包含 VM 实现。
真实执行后端有两种待验证的交付形态：

- 叶子 C ABI 组件，只经中立服务表访问核心。
- 同构建 Rust dylib 共享核心，固定工具链和链接选项，并随包携带所需 Rust 动态依赖。

真实后端拆库前须验证核心身份、全局状态和 TLS 单份，以及 GC、异常、worker、OSR 和代码寿命。
不能直接承诺每个 Rust staticlib 只含自身代码。最终 SDK/runtime pack 的库清单随实际链接方案确定。

---

## 8. 构建身份与生命周期

组件协议为内部同构建契约。公共表头同时检查 ABI 版本与结构大小，描述符再检查内容构建身份和预期组件 ID；不匹配即拒绝。
版本和身份校验只能检查协议，不能把不可信原生代码变成安全代码，因此 host 的原生工厂注册入口为 unsafe。

首版在程序执行前装配并冻结，不支持运行中更换实现或热卸载。组件实例由所属 Core 持有，接口租约延长 Core 和动态库寿命。
组件须在 destroy_vm 中停止自有任务并释放资源，不得在销毁完成后继续使用借用的服务句柄。

---

## 9. 后续接入顺序

1. 将 EngineRegistry/TierApi 适配到组件接口，明确执行、元数据、GC 根与线程服务。
2. 拆出真实 interp/JIT，验证单份核心和所有执行回退路径。
3. 拆出 profiler 与 debugger，经诊断/采样服务接入，未启用时不创建其线程和重型状态。
4. 完成 CLI/C 宿主组件选择、静态/动态 SDK 和 runtime pack 的平台矩阵；AOT 按其后端计划接入。

每项均需完整运行时验证、SDK verify 和性能门禁。通用框架与测试组件通过不代表真实后端已独立交付。

---

## 10. 与现有设计的关系

- **当前架构**（单 crate、`#[cfg(feature = "jit"/"aot"/"native-interop"/"interp-only"/"bundled-compression")]`）见 [vm-architecture.md](vm-architecture.md)：本文是它的演进目标，feature gate 是迈向组件化的第一步雏形。
- **嵌入 API / C ABI** 见 [embedding.md](embedding.md)：host 入口归 libz42 基座，不拆。
- **包分发** 见 [workload-distribution.md](../toolchain/workload-distribution.md)：其 Deferred `runtime-future-jit-cdylib-split` 是本架构的第一个落地切口。
- **分层执行 / OSR / 回收 / hot-reload**（叠在本组件框架之上，引擎内部各自分层）见 [tiered-execution.md](tiered-execution.md)。
- **IR 优化 / 特化 / intrinsic / tier0 基线质量** 见 [ir-specialization.md](ir-specialization-design.md)。
- **zpkg 加载上下文 / 重载 / 卸载回收 / 保留根诊断**（ALC 式，复用 observer/注册基座）见 [load-context.md](load-context.md)。
- **诊断与跟踪**（事件 / 计数 / 时间；统一事件总线，`libz42_debug` 是其 sink）见 [diagnostics.md](diagnostics.md)。
- **AOT 后端**（libz42_aot 组件；cranelift-AOT 复用 JIT 翻译核）见 [aot.md](aot.md)。

## VM 入口与实现归属

嵌入 Rust 入口统一由 `host` 提供，内部执行对象为 `vm::{Vm, VmContext, VmCore}`。上下文与共享 Core 的实现位于 `vm/context`；引擎共享的异常构造与原生栈保护位于 `vm/exec`；计数、运行时事件和观察者位于 `vm/diagnostics`。门面负责主上下文的配置与提供方装配，worker 加入、线程注册和 Drop 留在上下文实现中。目录归属不改变状态字段、repr、偏移和同步协议。

门面的具体装配实现在 `src/runtime/src/assembly/`，应用执行与参数服务在 `src/runtime/src/host/`。CLI 与嵌入适配通过 `host::run_app` / `LoadedProgram` 共用加载与调用服务，`assembly` 仅在 crate 内可见；共享启动步骤由应用入口和宿主调用，worker 生命周期仍归 VM 上下文。

CPU 采样通过 `vm/diagnostics/sampling.rs` 的 crate 私有接口装配：VM 只持有采样能力，具体 `profiler/cpu::Sampler` 由门面按既有配置构造。关闭快路使用不可变布尔 gate，启用后才派发到 backend；采样仍发生在原 safepoint，输出与 backend Drop 生命周期保持在原诊断状态边界。

运行时 crate 的源码根入口为 `lib.rs`；命令行入口为同级的 `main.rs`，进程启动辅助在 `startup.rs`，共享应用入口在 `host/`，具体组件接线在 `assembly/`，配置在 `config/`，崩溃报告在 `host/signal.rs`。AOT 请求仍由引擎注册表明确拒绝，不设空后端模块；线程协调归 `vm/threads/`，标准库线程胶水归 `corelib/core/threading.rs`。目录归属与依赖约束分别审查：查询层不应承担加载、执行初始化或等待线程的职责。

解释器栈分配策略由门面从已解析的 `RuntimeConfig` 转为 `StackAllocPolicy`，主构造存入 `VmCore.exec`，worker 共享同一策略。执行层的 `stack_alloc_enabled(ctx)` 与 `stats_enabled(ctx)` 只查询当前 VM 的不可变输入，不读取进程配置；on/off/stats 的语言行为保持一致，生产配置仍沿用门面原有单例读取时机。


## 当前组件框架

协议在 `crates/z42-component-api`，工厂加载与校验在 `host/components.rs`、`host/component_catalog.rs`，
实例和核心服务在 `vm/components.rs`。物理核心拆库不要求先扩大核心内部字段的公开面。

工厂描述符的公共头校验版本、大小和构建身份，随后校验预期 ID、回调、依赖和承诺接口。
构建身份由 workspace Rust 源码、清单/锁文件、工具链、目标及目标特性生成，跨版本组件拒绝加载。
静态清单的函数引用和动态库入口汇合到同一目录；目录在创建实例前拒绝重复、缺依赖与环，并按稳定 ID 拓扑排序。

`create_vm` 接收核心 RuntimeApi 和不透明 VmHandle，创建独立状态。
当前核心服务包含 VmInfo 的堆/VM 身份，以及只允许查询已声明依赖的接口服务。
每个承诺接口均检查版本、大小、对齐和返回状态；所有实例完成后才一次发布到 VmCore。
创建失败的非空部分状态也必须可销毁，失败回滚和正常销毁均按依赖反序。
销毁用于停止任务和释放组件资源，不执行托管代码；VM 信息与依赖接口在清理期间保持有效。
注册表锁不会跨组件回调持有。

接口表采用 C 布局和函数指针，不传 Rust trait 对象、Vec、String 或其析构所有权。
宿主的 ComponentLease 保留 Core 和实例；实例保留服务上下文、工厂和库。
worker 共享 Core 的实例；每 VM 的服务与状态不会因共享工厂而共享。最后的接口租约释放后，才能销毁 VM。
未选择任何组件时不创建组件实例、服务上下文或后台任务。

真实静态及动态测试组件在 `src/runtime/tests/data/component`，只依赖组件协议，验证依赖调用、VM 隔离、回滚与库保活。
现有 EngineRegistry/TierApi 尚未适配到跨库协议；EngineApi/TieringApi、完整执行/GC 服务以及 profiler/debugger 独立库属于后续组件接入。
静态/动态框架已经可用，不表示现有 JIT 已经能够单独 dlopen。
