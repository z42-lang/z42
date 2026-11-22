# 组件化运行时（Componentized Runtime）

运行时主库组合直接编译引用的 VM 模块；解释器、JIT 和 profiler 是 VM component，压缩/终端等是独立运行时库。
VM 能力经 components 内部门面消费，通用 native 库经自身或 interop 接口消费。profiler 按需准备，debugger 尚未独立实现。

## 1. 库边界

| 库 | 类型 | 拥有的实现/依赖 |
|---|---|---|
| z42-zpkg | 直接编译引用的 VM 模块 | zpkg/zbc/zsym、静态 IR 和 PackageImage，不依赖 VM；不经过组件发现/实例管理 |
| z42（runtime/src/lib.rs） | VM 模块的组合根 | VM、对象、GC、PAL、配置、corelib、interop 和宿主服务，不生产依赖具体执行组件 |
| z42-interp | component | 基线 Engine 与工厂，消费同构建 Core 契约，不依赖 JIT 实现 |
| z42-jit | component | Cranelift、tier/OSR 与工厂，消费同构建 Core 契约，不依赖 interp 实现 |
| z42-profiler | component | 采样/堆分析与工厂，消费 Core 的诊断/GC 契约 |
| z42-abi / z42-rs / z42-macros | 直接引用的配套模块 | 共享协议、native SDK 和构建期宏，不依赖 VM 实现；宏按编译阶段使用 |
| z42-compression | 独立运行时库 | 压缩 API 和通用 native 模块，不依赖 Core/引擎，不实现 VM component 工厂 |
| z42-repl | 独立运行时库 | 通用输入与编辑器，经 native ABI 接入，不依赖 Core/引擎 |

crate 类别描述职责与消费者，链接形态描述代码如何进入进程，两者相互独立。普通 Rust 依赖由 Cargo/链接器处理；
运行时需要装入的是 zpkg、原生库等产物，不能把 rlib 或每个内部模块都当作可激活组件。
独立运行时库可作为普通 Rust 依赖复用，也可通过 native 模块描述符接入 VM，其业务实现不携带 VM 具体类型。

### 代码加载与组件实例分工

loader 统一拥有代码来源、路径/符号、装载依赖与映像保活；静态、链接共享库、运行时装入分别记录来源并检查允许策略。
components 通过 loader 请求已准备的代码，验证组件契约、绑定能力、创建并释放每 VM 实例。组件实例间的语义依赖顺序
与二进制/包的装载依赖是不同的图；保留前者不意味着组件层还要维护另一套开库和代码缓存。
interop 验证通用 native 描述符并管理编组/实例，独立运行时库不为此绕入 components 的 VM 工厂协议。

通用原生映像装入、缓存与进程保活归 `loader/code.rs`，默认原生路径归 `loader/native_paths.rs`。
`components/catalog.rs` 经 Loader 装入运行时组件并校验组件描述符。通用 native 库的装载事务由
`interop/install.rs` 编排。依赖关系见[模块边界](module-boundaries.md)。

VM 主实现属于 z42 这一个 Cargo 包，runtime/src 内的目录是它的模块；工作区另有上述独立 crate。
主库提供 rlib/dylib，后端各自提供 rlib/cdylib 和 init 入口。生产运行时不依赖具体后端；最终静态嵌入示例通过 dev-dependency 链接独立后端的 rlib，不形成生产依赖环。JIT 白盒与跨后端测试由 JIT crate 执行，使用测试辅助接口和显式装配；runtime 单测不再同源编译 JIT。

main.rs 是轻量 dlopen/LoadLibrary 入口，加载 Core 后调用 z42_main_v1；Core 中 cli feature 的私有命令行实现通过 host 加载和执行 zpkg。静态示例 z42vm_static 复用命令行实现。非 cli 平台不编译参数解析或日志策略。组件和独立库仍支持三种接入形态。

普通嵌入功能从 `z42::host` 提供，CLI 也调用相同宿主服务。VM 消费者通过内部 `components::api` 请求
准备、执行角色及分析服务。同构建组件协议由 `components::abi` 提供，链接集成所用的 C 组件声明由
`z42_components.h` 维护，随 SDK 的统一头发布。

组件框架集中在 `runtime/src/components/`：api 提供内部能力访问，execution_roles 保存不可变角色绑定，catalog 加载和校验工厂，options 接收装配输入，instance 管理每 VM 实例和租约，backends 选择执行来源与发现候选，linked 管理静态链接与共享链接工厂。
components::abi 定义同构建跨库 C 协议，components::bridge 检查核心映像与布局。components 只公开同构建 abi/bridge 子模块，内部 api 和管理子模块不对 crate 外开放。

GC 与值/对象布局仍属于 core，不能运行中切换；它们位于分配、屏障和 safepoint 热路径，机器码使用固定布局。解析符号和崩溃栈定位是基础能力，不依赖可选调试器。

### 组件三种使用形态

三种形态使用同一能力、版本、依赖和每 VM 生命周期契约。静态链接和共享链接各有登记入口，
`Registration::Linked` 保留相应的 `NativeOrigin`。运行时装入由 `Registration::Runtime` 保存库路径和入口符号。
装配在访问工厂缓存前检查每 VM 的 `NativeLoading`。普通能力消费者经已绑定的角色调用组件。

| 形态 | 代码/入口来源 | 准备与检查 |
|---|---|---|
| 静态链接 | 实现归档链接进最终宿主/运行时；装配胶水保持工厂入口可达 | 核对构建清单、显式登记与运行时启用策略；不依赖全局构造器隐式登记 |
| 共享链接 | 组件共享库是宿主/运行时的链接依赖，由系统链接/装载机制提供入口 | 核对链接清单、已绑定实现映像与运行时策略；保留依赖映像及回调寿命；不再按库名当作运行时加载来源搜索 |
| 运行时装入 | 明确请求后由 loader 统一请求 PAL 使用 dlopen/LoadLibrary 装库并取得入口 | loader 在开库前检查允许策略、平台能力、来源与依赖；能力适配器校验协议/核心身份，再创建实例与发布 |

“动态链接”不是运行时 `open_library` 的别名，构建 profile 的 release/debug 也不表示使用形态。
实际来源由构建/部署清单与装配适配器声明并核对，不能仅凭一个函数指针、文件存在或 Dynamic 名称推断。
运行时设置表达允许的来源形态、所需能力/角色及禁用项；这些是功能策略，不是向普通宿主暴露组件工厂、描述符或租约。

准备流程是：捕获运行时策略 → 核对构建/部署与平台能力 → 选择被允许的提供方/来源 → 获取入口 →
校验 ABI、构建/核心映像身份与接口版本 → 展开依赖闭包 → 按依赖创建实例 → 绑定能力并发布。
缺失、歧义、被禁用、不支持的形态及协议/依赖不匹配均显式失败，不回落到被禁止的来源或静默换执行模式。
同一 VM 可组合不同来源的组件，但共享核心身份与依赖契约保持一致；每个工厂的来源和代码保活信息随登记保存。

静态和链接动态代码可能先于 VM 配置解析进入进程，运行时禁用只能控制登记使用、实例化与能力启用；
要保证实现不被链接/装入，依靠构建与分发裁剪。链接动态库缺失时也可能由系统在 VM 启动前报告部署错误。
运行时装入可在调用 OS 装载前拒绝策略不允许的来源；原生库初始代码可能在装库时执行，仍要求来源可信。
iOS/WASM 等平台按可信运行时组件的实际装载能力判定，不能套用脚本 native 扩展的权限开关，也不能承诺所有平台三种形态均可用。

`xtask package runtime --verify` 使用真实产物验证静态链接、共享链接和运行时装入，具体组合取决于目标平台。
共享链接用例在禁用静态链接和运行时装入的配置下执行，并验证单独禁用共享链接会被拒绝。
组件测试另覆盖角色歧义、ABI 校验、失败回滚、只读查询和 worker 租约。

## 2. 引擎注册与选择

VmCore.exec.engines 持有基线 Engine、JIT 模式 Engine、TierApi 和后端配置。内部能力门面准备来源并绑定 Baseline/Optimizing 语义角色，
注册表在任何 create_vm 之前核对绑定唯一性、提供方存在及 ExecutionApi 的版本/大小/对齐。实际接口查询仍校验返回表头。
api 的装配协调按角色取得接口、验证核心身份后调用 ExecutionApi.attach；成功返回还须确认所需引擎已登记。core 只调用已注册能力。静态与动态后端使用同一 attach 路径，不依赖全局构造器。

interp 的静态、虚、闭包和混合调用通过共享参数视图进入 Engine；JIT 的冷/不可翻译调用通过注册的基线 Engine 回落。解释器热循环保留 OSR 计数及阈值比较，交接经 TierApi。两个后端不互相引用实现，不使用进程全局 JIT 单例。

LoadOptions.mode 表达执行模式，native 表达搜索目录与静态/共享链接/运行时装入许可，allow_profiling 控制可选分析能力。私有装配优先使用允许的已链接提供方，再按允许策略搜索运行时库；JIT 请求仍同时需要基线解释器，缺失或不支持时明确报错。

原生库搜索共用 loader/native_paths：显式配置（含 Z42_NATIVE_PATH）、可执行文件旁 native/、../native/、可执行文件目录和已加载核心所在目录。iOS 也查询相邻 Framework 与应用的 Frameworks 目录；Android 最后交给系统加载器按库名查询，以支持 APK 内未解压的原生库。VM 在冻结的搜索目录中按固定候选顺序查询，来源许可在探测或复用代码之前检查。可用性查询只读候选文件及 loader/code 的已装入映像缓存，不打开库或调用工厂；Android 未知裸库名在显式准备时才交给系统加载器。
C 宿主初始化核对 JIT 的构建/目标支持，不探测来源；缺库、入口与 ABI 错误由程序准备报告。真正装配仍须完成库加载和接口校验。

### 构建矩阵

| 构建 | 后端可用集 | 运行时选择 |
|---|---|---|
| 桌面 SDK / xtask build runtime | 动态 interp + jit | interp / jit；缺库报错 |
| Rust 静态宿主 / C 静态归档 | 按最终宿主链接并登记的后端 | 未登记的后端明确报错 |
| 不含 interp/jit 的普通静态构建 | 无后端 | 请求执行明确报错 |
| iOS 真机 runtime 包 | 独立静态 interp、compression；另提供动态 Framework | 不编入 JIT/Cranelift，请求 jit 报错 |
| iOS simulator runtime 包 | 独立静态与 Framework interp、JIT、compression | 默认 interp；显式选择 JIT，缺库报错 |
| Android arm64/x64 runtime 包 | 独立静态与动态 interp、JIT、compression | 默认 interp；显式选择 JIT 时加载组件，缺库报错 |
| browser-wasm runtime 包 | 默认最终 WASM；另提供 core、interp、compression 静态库 | 在链接时选择组件，生成新的最终 WASM |
| shared-core 动态桌面包 | 动态 interp / jit | 由部署与运行设置决定；缺库报错 |

JIT 的依赖仅在 Linux/macOS/Windows/Android 的 x64/arm64 target 与 iOS simulator（target_abi=sim） 选择；即便不支持的平台误传 jit feature，也不引入 Cranelift 实现。动态组件使用目标平台匹配的 Rust std；iOS 将它与核心、解释器、compression 一起封装为独立 Framework。WASM 没有原生 dlopen，只提供最终模块和可重链静态库。

### 跨平台编译与链接模型

模块自己拥有脚本接口描述、注册入口和实现。静态链接由宿主调用登记入口，动态装入由 loader 找到同一入口后登记；interop 负责校验和查询，不维护各模块的函数清单。链接形态不改变每 VM 状态、调用契约与资源所有权。

| 平台 | 静态编译 / 链接 | 动态形态与边界 |
|---|---|---|
| Linux / macOS / Windows | Rust 宿主链接 rlib，C 宿主链接目标平台的静态归档，再显式登记模块 | 系统动态库 `.so` / `.dylib` / `.dll`；loader 经 PAL 装入并校验登记入口 |
| Android | 将目标架构静态归档链接进宿主或 JNI 库，显式登记模块 | 使用随应用部署的 `.so`；动态能力取决于 PAL 和宿主提供的装载环境 |
| iOS | 静态链接所需模块并登记；压缩使用独立模块的静态归档 | 应用预先部署的可信运行时 Framework 与脚本请求的 native 扩展是两类契约；后者禁止，不能以组件装载绕过 |
| Browser WASM | 为 `wasm32-unknown-unknown` 编译 Rust 库或包含 WASM 对象的 `.a`，链接生成生产 release `.wasm`；需要额外能力时用静态库重新链接 | 默认产品只带运行所需模块，不包含 profiler/debug 等开发组件；独立动态 WASM 模块加载尚未实现 |

WASM 的 `.a` 是供链接器消费的归档，不能由浏览器实例化；`.rlib` 是 Rust 构建输入，也不能当作对外 C 静态库。静态链接的最终交付物仍是 `.wasm`，所需 core、解释器和模块代码已在链接期确定。`wasm-pack` / wasm-bindgen 生成的 JS 加最终 `.wasm` 包，不表示运行时已经支持独立动态库。LLVM 的 [WASM 链接器说明](https://lld.llvm.org/WebAssembly.html)区分了对象链接、imports/exports 与实验性的动态链接约定。

WASM 最终产品的默认发布契约是**最小生产 release**：只链接应用运行必需的 core、选定执行后端和所需库。profiler、debugger、开发测试宿主等能力不进入默认依赖闭包，也不通过导出或注册入口意外保活。`--release` 只选择构建优化配置，不能代替组件裁剪；把开发组件链接进去后仅关闭运行期开关，不满足这一契约。

有诊断或调试需求时，使用对应目标的静态库重新链接，显式选入所需组件并登记，生成专用 `.wasm`。这是同一份模块源码与注册协议的另一种最终链接配置，默认产品不必预留全部开发实现，也不要求先实现动态 WASM 加载。静态库是重链接的支持产物，与默认浏览器产品分开交付；profiler 提供同批静态归档用于堆分析；浏览器 CPU 定时采样不支持。debugger 尚未独立提供归档。

发行验证应分别检查默认 release 和按需重链接配置：前者的依赖、导出和注册集合不含开发组件，后者只增加显式选入的能力，并验证注册、执行和资源释放。不能只凭 profile 名称或文件后缀判断裁剪完成。

独立动态 WASM 模块的目标模型是：模块单独编译为 `.wasm`，由 JS 或其他 WASM 宿主读取、校验、补齐 imports 并实例化，再通过 exports 取得注册入口，把能力接入已有 VM。模块通过导入的宿主服务访问运行时，不重复创建一份 core、GC 或类型身份系统。WebAssembly 的 [JS API](https://webassembly.org/getting-started/js-api/)提供实例化和 import/export 连接机制；这些机制与操作系统 dlopen 不同。

动态 WASM 的逻辑注册契约与静态模块一致，跨实例传输需要独立适配：各实例拥有自己的线性内存时，通过复制缓冲、句柄和宿主 trampoline 交互，不能直接传递另一实例的地址或函数表下标。如果采用共享 memory/table，则必须先定义并验证重定位、分配器、表索引和生命周期约定，不能仅凭导出函数同名就复用原生 C 指针布局。Rust `cdylib` 产出 `.wasm` 本身不保证这些条件成立。

因此，当前 PAL 报告 WASM 不支持 native 动态库，禁止的是 OS native 装载路径；未来的 WASM 模块加载由宿主适配层提供独立能力，不能把该开关改成 true 后继续调用 dlopen。Repl 的终端编辑和用户历史仍属于 tier1 桌面模块，WASM 编译/求值与静态 compression 不依赖这些能力。

## 3. 描述符、身份与边界

ComponentOptions 将启用根与提供方来源分别保存。现有 with_static / with_dynamic 同时登记来源并启用；with_static_provider / with_dynamic_provider 只登记来源。Catalog 先检查全部 ID 重复，再从启用根按稳定 ID 顺序递归加载所需工厂；未使用的提供方不调用 init 或打开动态库。来源只来自显式配置，不额外扫描或改变后端策略。接口查询读取冻结注册表，不触发组件加载。

组件 init 返回不可变 ComponentDescriptor；其头检查协议版本、结构大小、构建身份和预期 ID。依赖先排序，拒绝重复、缺依赖和环；接口表检查版本、大小与对齐。组件全部创建成功后才发布注册表，失败按依赖反序回滚。

构建身份来自 workspace Rust 源码、清单/锁文件、工具链、target 和 target features。ExecutionApi 还返回核心物理身份及编译期 Core ABI 摘要；摘要含核心布局、偏移和影响布局/行为的构建选项。传递借用的 VmContext 地址前必须全部匹配，拒绝把另一份静态 core 或不兼容 feature/profile 的 core 当作同一 VM。

通用协议只传 C 布局、函数指针与不透明句柄，不跨边界传 Rust 容器所有权或 unwind。真实执行后端的 attach 使用同构建共享 core 的 Rust 内部契约：借用指针仅在 attach 调用期间有效，后端通过该 core 注册 Engine/TierApi。这不是第三方或跨 Rust 版本插件 ABI。身份校验不能使不可信原生代码安全，原生工厂注册入口为 unsafe。

## 4. 单份核心与动态构建

运行时的单一主库直接提供 rlib/dylib。 Static builds use rlib; dynamic packs use prefer-dynamic, one runtime dylib and the matching Rust std. Backends directly provide rlib/cdylib and init entries.

动态宿主、Rust 执行后端、共享核心与验证可执行文件必须在同一 Cargo 图中构建。纯 C ABI 的 compression 实现在隔离的 target 图中构建，避免算法 feature 合并回只依赖接口声明的 core。分开运行 Cargo 可能因开发依赖或传递依赖 feature 合并不同而覆盖成不兼容的 Rust metadata/符号；打包器一次构建全部目标后，从同一组产物搬移并验证，不再用后续 Cargo 调用重写核心。

共享 std 的动态构建使用 System 分配器。自定义分配器和 dhat 位于最终静态宿主 z42vm_static，通过独立 target-dir 构建，不覆盖动态运行时。C 的 libz42.a 保留默认解释器初始化入口 z42_linked_components_init；自定义组合可通过 z42_component_register_static 登记实际链接的后端，Rust 宿主使用 components::abi::register_static_component。静态组合须使用同一次构建图产生的库，不能混用不同 feature/profile 的核心。静态登记仅保留进程寿命工厂，不保留 VM 实例。

SDK 的 native/ 包含统一 libz42、interp/JIT 组件、compression 和匹配的 Rust std。runtime 包另外提供 C 头文件和最终静态嵌入归档；Windows 另含导入库。macOS 使用 @rpath 标识和 @loader_path，ELF 使用 $ORIGIN；库与头文件、stdlib 一起搬移后可使用。
桌面 SDK 只安装 bin/z42vm[.exe]，它使用静态 std，native/ 不包含另一个 z42vm。显式 Z42_RUNTIME_LIBRARY 优先，否则定位可执行文件相对 native/ 或同目录 Core；不查 cwd/artifacts。Windows 以 DLL_LOAD_DIR 和 DEFAULT_DIRS 解析库旁依赖。Core 保活至进程退出，不创建转发子进程。Windows runtime 导入库仍供嵌入者动态链接使用。

### 按需压缩与 WASM 组合

compression 不属于核心的必需依赖。原生包中的独立动态库在首次调用压缩内置函数时才加载；单纯初始化 VM 或执行其他代码不会装载它。静态宿主链接 libz42_compression.a 后，通过 z42_native_register_static("z42_compression", z42_compression_get_symbol) 登记符号解析器；Rust 链接登记入口归 loader::linked。未链接或未部署时，实际压缩调用返回明确的组件不可用错误。

browser-wasm 默认最终模块显式启用 bundled-compression，保持现成 JavaScript 门面的能力。最终 WASM 的 Cargo 输出与静态组件图隔离，避免不同 workspace 的 LTO / bitcode 产物相互覆盖。随包提供的 libz42.a、libz42_interp.a、libz42_compression.a 则用于项目自行组合：选择需要的库，在最终链接阶段登记组件，生成另一份 .wasm。运行时开关只能选择已链接能力，不能凭空加载未链接的原生库。每个组件的原生链接参数记录在包内 manifest.toml 的 [components.<stem>].link_flags（静态链接配方含 init 调用与归档顺序）；静态库必须使用同一批发布产物。

生产与开发使用相同组合方式：开发构建以后可链接 profiler/debugger，生产构建可省略。profiler 已独立提供 rlib/cdylib 和发布用静态归档；debugger 尚未独立实现。

## 5. 每 VM 生命周期

工厂可在克隆的选项间共享缓存，每个 VM 的实例、JIT 状态和堆保持独立。create_vm 接收借用 RuntimeApi 和 VmHandle；依赖接口查询只允许已声明的依赖。注册表锁不跨组件回调持有，成功的无状态组件允许空实例。

worker 加入同一 Core，不重建组件或扩展。ComponentLease 保留 Core、实例、工厂和库，最后租约释放后才能销毁 VM。EngineRegistry 单独保留不反向持有 Core 的代码租约，避免循环引用；后端状态和代码页先于它们借用的模块/堆释放，代码租约最后释放。

动态代码映像统一由 loader/code 按规范化路径在共享 core 中缓存，组件和独立 native 库共用，保留到进程结束；重复装配复用同一原生句柄。Rust 或依赖库的 TLS 析构回调可能在线程退出时才执行，因此最后一个 VM/接口租约释放后仍不能卸载代码映像。即使描述符校验失败，已加载映像也须保留，因为库初始化器可能已注册 TLS。此缓存只保留代码，不保留 VM、堆或 JIT 实例，不解释消费者 ABI。开库和释放并发产生的冗余 OS 句柄均在缓存锁外执行。

正常销毁和失败回滚均按依赖反序调用 destroy_vm。组件必须停止任务并释放资源，销毁不执行托管代码。服务与依赖在清理回调期间仍有效；禁止运行中切换实现或热卸载。

## 6. 配置与装配

vm_assembly 统一程序发布顺序，vm_defaults 和 vm_services 提供默认 VM 输入，loader/artifact 拥有产物入口与 LazyLoader 构造。host 负责功能请求和 resolver 适配，worker 共享已有 Core。

BuiltinCatalog 只含名称/ID 与折叠属性，callable 表属于 exec，provider 由门面装配。纯派发缓存归 runtime/dispatch，加载、初始化、等待与异常物化编排归 exec/ops。监视器机制与争用探针属于 core，标准库适配属于 corelib。

LOH 覆盖、native 扩展加载、固定地址登记和 TLAB/SATB 顺序保持原契约。核心库隔离应用执行与退出报告通过冻结的服务函数指针接回 host；采样器实现由门面提供，关闭时保留原快路。

## 7. 验证与后续组件

```bash
./xtask test
./xtask package sdk --verify
./xtask package runtime --profile debug --verify
./xtask package runtime --rid iossim-arm64 --profile debug --verify
./xtask package runtime --linkage dynamic --profile debug --verify
./xtask package runtime --linkage dynamic --profile release --verify
```

动态包 verify 将库搬移到独立目录，清除动态链接搜索环境，执行实际生产依赖图的 engine_components 集成测试，覆盖解释/JIT、worker/GC/异常、VM 隔离、缺库/来源禁用、worker 在宿主释放后的执行及正常释放，以及从 host 动态库调用 C 嵌入接口。测试必须实际执行，零用例不算通过。通用静态/动态 ABI 夹具另覆盖描述符拒绝、依赖、回滚与库保活。

profiler 的算法与线程位于 z42-profiler，diagnostics 只持有中立契约和按需 provider。关闭采样时不实例化 Sampler；开启时在托管执行前加载并检查身份，堆分析在首次请求时加载。每 VM 的 provider 先于其组件代码租约释放，租约不反向持有 Core。

后续拆 debugger：core 仅保留观察者、采样和诊断契约，独立组件拥有工具实现、线程和重型状态。AOT 按其后端计划接入。每项都需要完整运行时、SDK、平台与性能验证；不得把通用框架或某一平台通过当作全部组件已交付。

## Deferred

### runtime-future-wasm-module-loading：独立 WASM 模块加载

- **来源**：原生模块自注册与跨平台链接设计（PR #1736）。
- **触发原因**：当前浏览器运行时采用最终单模块链接，没有独立模块的装载、注册与跨实例传输实现。
- **前置依赖**：版本化模块注册协议、宿主 import/export 适配、明确的内存/函数引用及状态寿命契约。
- **触发条件**：需要不重链接宿主即可按需分发 compression 等 WASM 能力时实施，并覆盖重复装载、ABI 拒绝、多 VM 隔离和释放。
- **当前 workaround**：将所需模块静态链接到最终 `.wasm`，实例化后显式登记。

## 关联文档

- [VM 总体架构](vm-architecture.md)
- [嵌入机制](embedding.md)
- [分层执行与 OSR](tiered-execution.md)
- [运行时/包分发](../toolchain/workload-distribution.md)
- [诊断与跟踪](diagnostics.md)

跨库组件协议由 z42::components::abi 定义，包含描述符、C 布局接口和执行注册；core build.rs 生成共享构建身份。
z42::components::bridge 提供最小核心身份/布局校验及只读对象服务借用，不包含组件管理。对象协议定义归 object，bridge 只将 Core 的已绑定服务交给经过同核心校验的执行组件，不做组件表查询。host 不再转出组件管理类型。生产执行库单向依赖 core，
通用 ABI 夹具只使用协议值和回调，实际执行组件仍链接同一份共享核心。
