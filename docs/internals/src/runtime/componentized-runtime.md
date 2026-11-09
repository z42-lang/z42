# 组件化运行时（Componentized Runtime）

运行时主库直接拥有共享 VM 实现，解释器和 JIT 是独立 Rust 库。静态链接与同构建动态组件共用 host 门面与组件装配协议；profiler/debugger 独立交付仍是后续工作。

## 1. 库边界

| 库 | 拥有的实现 | 依赖方向 |
|---|---|---|
| z42-zpkg | zpkg/zbc/zsym 解析、静态 IR 和不可变 PackageImage | 不依赖 VM |
| z42（runtime/src/lib.rs） | host、VM、对象、GC、PAL、配置、corelib、interop 与诊断实现 | 不依赖具体引擎 |
| z42-interp | 解释器、基线 Engine 和组件工厂 | 依赖 core，不依赖 JIT |
| z42-jit | Cranelift、编译代码、tier/OSR 实现和组件工厂 | 依赖 core，不依赖 interp 实现 |

运行时只有一个 Cargo 包；runtime/src 内的目录都是它的模块。主库提供 rlib/dylib，后端各自提供 rlib/cdylib 和 init 入口。生产运行时不依赖具体后端；测试和最终静态嵌入示例通过 dev-dependency 链接独立后端的 rlib，不形成生产依赖环。

main.rs 不调用 Rust VM 接口，只查找 libz42 并取得版本化 C 入口。先验证头版本、大小、构建身份和回调，再把参数交给 host::cli。已打开的库保留到进程结束，避免运行时或依赖的 TLS 析构跳入已卸载代码。

嵌入接口与类型统一从 z42::host 提供。CLI 只处理参数、启动和进程策略，也调用 host。组件内部 Rust API 不自动成为嵌入 API。

GC 与值/对象布局仍属于 core，不能运行中切换；它们位于分配、屏障和 safepoint 热路径，机器码使用固定布局。解析符号和崩溃栈定位是基础能力，不依赖可选调试器。

## 2. 引擎注册与选择

VmCore.exec.engines 持有基线 Engine、JIT 模式 Engine、TierApi 和后端配置。门面选择组件，校验后调用 ExecutionApi.attach 注册能力；core 只调用已注册能力。静态与动态后端使用同一 attach 路径，不依赖全局构造器。

interp 的静态、虚、闭包和混合调用通过共享参数视图进入 Engine；JIT 的冷/不可翻译调用通过注册的基线 Engine 回落。解释器热循环保留 OSR 计数及阈值比较，交接经 TierApi。两个后端不互相引用实现，不使用进程全局 JIT 单例。

LoadOptions.backends 的每个后端可选 Auto、Linked、Dynamic(path)、Disabled。Auto 优先使用静态实现，否则查询动态库；JIT 模式同时需要基线解释器。显式 JIT 请求在加载/准备阶段验证，未链接、库缺失、平台不支持、协议不匹配和被禁用均返回错误，不改成解释模式。CLI 的 --mode jit、配置和 Z42_MODE=jit 遵循相同规则。

原生库搜索共用 host 的 native_paths：显式配置（含 Z42_NATIVE_PATH）、可执行文件旁 native/、../native/ 和可执行文件目录。Auto 按固定候选顺序查询，显式 Dynamic 只使用所给路径。可用性查询表示候选是否存在，真正装配仍须完成库加载和接口校验。

### 构建矩阵

| 构建 | 后端可用集 | 运行时选择 |
|---|---|---|
| 桌面 SDK / xtask build runtime | 动态 interp + jit | interp / jit；缺库报错 |
| Rust 静态宿主 / C 静态归档 | 按最终宿主链接并登记的后端 | 未登记的后端明确报错 |
| 不含 interp/jit 的普通静态构建 | 无后端 | 请求执行明确报错 |
| wasm / ios / android 预设 | 静态 interp | 不编入 JIT/Cranelift，请求 jit 报错 |
| shared-core 动态桌面包 | 动态 interp / jit | 由部署与运行设置决定；缺库报错 |

JIT 的依赖仅在 Linux/macOS/Windows 的 x64/arm64 target 选择；即便不支持的平台误传 jit feature，也不引入 Cranelift 实现。动态组件包仅支持在目标桌面原生构建，不支持跨平台复制 Rust dylib。

## 3. 描述符、身份与边界

组件 init 返回不可变 ComponentDescriptor；其头检查协议版本、结构大小、构建身份和预期 ID。依赖先排序，拒绝重复、缺依赖和环；接口表检查版本、大小与对齐。组件全部创建成功后才发布注册表，失败按依赖反序回滚。

构建身份来自 workspace Rust 源码、清单/锁文件、工具链、target 和 target features。ExecutionApi 还返回核心物理身份及编译期 Core ABI 摘要；摘要含核心布局、偏移和影响布局/行为的构建选项。传递借用的 VmContext 地址前必须全部匹配，拒绝把另一份静态 core 或不兼容 feature/profile 的 core 当作同一 VM。

通用协议只传 C 布局、函数指针与不透明句柄，不跨边界传 Rust 容器所有权或 unwind。真实执行后端的 attach 使用同构建共享 core 的 Rust 内部契约：借用指针仅在 attach 调用期间有效，后端通过该 core 注册 Engine/TierApi。这不是第三方或跨 Rust 版本插件 ABI。身份校验不能使不可信原生代码安全，原生工厂注册入口为 unsafe。

## 4. 单份核心与动态构建

运行时的单一主库直接提供 rlib/dylib。 Static builds use rlib; dynamic packs use prefer-dynamic, one runtime dylib and the matching Rust std. Backends directly provide rlib/cdylib and init entries.

所有动态宿主、后端、共享核心与验证可执行文件必须在一次 Cargo 图中构建。分开运行 Cargo 可能因开发依赖或传递依赖 feature 合并不同而覆盖成不兼容的 Rust metadata/符号；打包器一次构建全部目标后，从同一组产物搬移并验证，不再用后续 Cargo 调用重写核心。

共享 std 的动态构建使用 System 分配器。自定义分配器和 dhat 位于最终静态宿主 z42vm_static，通过独立 target-dir 构建，不覆盖动态运行时。C 静态归档 z42_static 链接独立后端，调用方先调用 z42_linked_components_init；Rust 宿主使用 host::register_static_component。静态登记仅保留进程寿命工厂，不保留 VM 实例。

SDK 的 native/ 包含统一 libz42、interp/JIT 组件、compression 和匹配的 Rust std。runtime 包另外提供 C 头文件和最终静态嵌入归档；Windows 另含导入库。macOS 使用 @rpath 标识和 @loader_path，ELF 使用 $ORIGIN；库与头文件、stdlib 一起搬移后可使用。Windows 的启动器还需要 bin/ 内的匹配 std DLL，确保在进入 main 之前完成系统装载。

## 5. 每 VM 生命周期

工厂可在克隆的选项间共享缓存，每个 VM 的实例、JIT 状态和堆保持独立。create_vm 接收借用 RuntimeApi 和 VmHandle；依赖接口查询只允许已声明的依赖。注册表锁不跨组件回调持有，成功的无状态组件允许空实例。

worker 加入同一 Core，不重建组件或扩展。ComponentLease 保留 Core、实例、工厂和库，最后租约释放后才能销毁 VM。EngineRegistry 单独保留不反向持有 Core 的代码租约，避免循环引用；后端状态和代码页先于它们借用的模块/堆释放，代码租约最后释放。

动态组件代码映像按规范化路径在共享 core 中缓存，保留到进程结束；重复装配复用同一原生句柄。Rust 或依赖库的 TLS 析构回调可能在线程退出时才执行，因此最后一个 VM/接口租约释放后仍不能卸载代码映像。即使描述符校验失败，已加载映像也须保留，因为库初始化器可能已注册 TLS。此缓存只保留代码，不保留 VM、堆或 JIT 实例。加载库时不持有缓存锁。

正常销毁和失败回滚均按依赖反序调用 destroy_vm。组件必须停止任务并释放资源，销毁不执行托管代码。服务与依赖在清理回调期间仍有效；禁止运行中切换实现或热卸载。

## 6. 配置与装配

host/assembly 将 RuntimeConfig 转为 core 拥有的 VmConfig、JitConfig、StackAllocPolicy 与惰性 FusionInputs；core 不解析门面环境变量。主上下文通过 RuntimeInputs 接收引擎、builtin 目录/表、对象协议、采样和扩展；worker 仅共享已有 Core 并建立线程视图。

BuiltinCatalog 只含名称/ID 与折叠属性，callable 表属于 exec，provider 由门面装配。纯派发缓存归 runtime/dispatch，加载、初始化、等待与异常物化编排归 exec/ops。监视器机制与争用探针属于 core，标准库适配属于 corelib。

LOH 覆盖、native 扩展加载、固定地址登记和 TLAB/SATB 顺序保持原契约。核心库隔离应用执行与退出报告通过冻结的服务函数指针接回 host；采样器实现由门面提供，关闭时保留原快路。

## 7. 验证与后续组件

```bash
./xtask test
./xtask package sdk --verify
./xtask package runtime --linkage dynamic --profile debug --verify
./xtask package runtime --linkage dynamic --profile release --verify
```

动态包 verify 将库搬移到独立目录，清除动态链接搜索环境，执行实际生产依赖图的 engine_components 集成测试，覆盖解释/JIT、worker/GC/异常、VM 隔离、缺库/禁用/未静态链接、接口租约和正常释放，以及从 host 动态库调用 C 嵌入接口。测试必须实际执行，零用例不算通过。通用静态/动态 ABI 夹具另覆盖描述符拒绝、依赖、回滚与库保活。

后续拆 profiler/debugger：core 仅保留观察者、采样和诊断契约，独立组件拥有工具实现、线程和重型状态。AOT 按其后端计划接入。每项都需要完整运行时、SDK、平台与性能验证；不得把通用框架或某一平台通过当作全部组件已交付。

## 关联文档

- [VM 总体架构](vm-architecture.md)
- [嵌入机制](embedding.md)
- [分层执行与 OSR](tiered-execution.md)
- [运行时/包分发](../toolchain/workload-distribution.md)
- [诊断与跟踪](diagnostics.md)

Component protocol ownership: z42::components::api defines descriptors, C-layout interfaces and execution registration. Core build.rs generates the shared build identity. Host re-exports the public embedding types; execution libraries depend on the core, preserving an acyclic Cargo graph. Generic ABI fixtures use only protocol values and callbacks; actual execution components still link the single shared core.
