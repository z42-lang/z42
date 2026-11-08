# 组件化运行时（Componentized Runtime）

> 待办：设计已定，尚未实施（目标架构）
>
> 本文是 z42 运行时**未来的组件化目标架构**：把执行后端、GC、调试等从单一 `z42` crate 中拆成可独立交付的组件，支持「独立 z42vm 运行时按需加载」与「嵌入按需链接」。
>
> **实施触发**：ROI 出现（如某分发渠道对 z42vm 单二进制体积敏感、或 interp/jit 需按需下发）。在此之前仅作为目标设计存在；`runtime-workload-distribution.md` 的 Deferred `runtime-future-jit-cdylib-split` 指向本文。
>
> 当前运行时架构（单 crate + feature gate）见 [vm-architecture.md](vm-architecture.md)；运行时/包分发见 [runtime-workload-distribution.md](../toolchain/workload-distribution.md)。

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

### 4.1 当前单 crate 的注册边界

当前运行时保持一个 crate。门面 `assembly/engine.rs` 按 feature 装配 Engine 与优化入口；`VmCore.exec.engines` 持有每 VM 注册表和一次发布的 TierApi。VM 的模式分发、初始化执行及共享 ToString 调用均通过注册能力，不静态选择具体后端。

interp 的静态、虚、闭包和通用混合入口传入共享参数视图；TierBackend 的 JIT 实现拥有 JitRuntime、JitFrame、编译器与代码。OSR 的计数和阈值比较留在解释器热循环，实际交接经 TierApi。JIT 的冷/不可翻译调用经已注册 Engine 回落，两个后端的生产代码互不引用。

派发纯事实与缓存归 `vm/runtime/dispatch`；可能加载、初始化或物化异常的执行编排归 `vm/exec/ops`。同一帧链与栈读取面归 VM，具体后端只持自己的激活。代码与 worker 共享的注册状态由所属 VmCore 保活，各 VM 独立。VmContext 内联组合 exec/runtime/diagnostics/threads 的每线程状态，所属线程的帧、异常槽、缓存与暂停计数不会因共享 Core 而共享。

BuiltinCatalog 在 runtime 保存纯名称/ID 与折叠属性，exec 保存 callable 表并派发；门面从 corelib / interop 描述符装配两者，不让 runtime 引入执行 ABI 或具体实现。默认目录不可变，现有 native 符号按 interop 原时序加载。

JIT 的安装输入为后端拥有的 JitConfig，门面负责配置转换和注册适配；分层/编译/翻译仅使用已传入的值。该内部输入是同构建 Rust API，不是动态库 ABI。

VM 的内部构造输入为 `vm/config.rs` 的 `VmConfig`，由门面从已决议的 `RuntimeConfig` 捕获 GC、预算、采样与 trace 设置。内部构造消费纯值，不解析环境变量或门面旋钮；既有构造入口负责转换。`process_loh_bytes` 明确表示构造期应用的进程级 LOH 覆盖，仍在 core 建立后应用，不提供每 VM LOH 隔离；safepoint 与 stackalloc 的进程策略也保持独立。该输入不存入 VmCore，不改变线程视图布局。

Loader 拥有私有 `FusionInputs`，由门面的惰性函数指针在每次实际计算融合侧表时提供。它沿用进程级 `RuntimeConfig` 的首次读取语义。公开加载入口、可用性探测以及每 VM 的惰性加载和可回收程序集上下文使用同一配置来源。装配只传递指针，模块就绪判断、解码错误和融合识别顺序保持原样。

主 VM 构造由 crate 门面 `assembly/context.rs` 实现既有 `VmContext` 入口，负责配置捕获、组件选择与 core 接线。`vm/context/construct.rs` 保留线程状态访问、共享 core 的 worker 加入和 Drop；worker 不重新选择组件或重建扩展。主构造的弱引用扫描器、native 扩展加载、固定地址登记及 TLAB/SATB 顺序保持一致，扩展资源随最后一个持有 core 的上下文释放。

核心库的隔离应用执行与退出报告经 `corelib/application.rs` 的私有服务接入，门面 `app/services.rs` 负责应用选项与报告策略。冻结前注入的服务只含函数指针，不拥有 VM 或托管值；并行用例只复制服务值与 Rust 路径/VFS。核心库仍负责 stdout sink 栈、按输入排序和 join 时 park，进程退出仍先报告再终止且不运行析构。

值的无上下文格式化与盒装 struct 哈希由 `object/value` 的 crate 内部叶子模块定义，VM、解释器、JIT 与异常展示直接消费。`corelib::convert` 的既有公开格式化路径直接再导出同一定义；typed builtin 保留参数检查、错误和装箱编排。64 字节栈缓冲、UTF8 完整片段、spill、标量文本、FNV 顺序与盒装值借用边界保持一致；用户 ToString 协议和托管对象物化不属于这些纯值原语。

执行层的对象协议由 `vm/exec/object_protocol.rs` 定义，门面 `assembly/object_protocol.rs` 将核心库既有 GetType 和默认 ToString 函数直接绑定到每 core 的不可变服务。服务只存函数指针，不持有 VM 或托管值，worker 共享同一注册。装箱 GetType 在原派发分支内传入完整接收者，反射实现仍在调用时物化托管对象；引用对象的默认 ToString 返回 Rust String，执行调用方仍在原位置分配托管字符串并处理 BuiltinError。用户 override、纯值格式化和普通 builtin 的派发路径不经此默认回落。

### 4.2 独立交付仍是目标设计

注册边界见 [VM 总体架构](vm-architecture.md)「引擎注册与调用边界」。物理拆 crate、跨动态库的组件 ABI、自动加载和卸载尚未实现；当前 Rust trait/注册对象不是可跨版本或跨动态库使用的 ABI。以后独立交付须在这一每 VM 所有权边界上设计同构建组件 ABI，不能改成进程全局 JIT 单例。

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

## 6. 绑定粒度（static / dynlink / dlopen）× 统一调用面

三种绑定粒度，**调用面统一成一套（注册槽）**，差别只在「槽何时、怎么被填」。core 里只有一条调用路径，组件库不必知道自己被怎么消费。

组件库永远只暴露一个注册入口，两形态共用同一注册逻辑：
- Rust `fn register() -> BackendApi`（静态路径直接调）
- `extern "C" fn z42_register_<c>()`（动态/dlopen 路径按名查）

| 粒度 | 槽何时/如何填 | 可用集定于 | 可切？ | 调用成本 |
|---|---|---|---|---|
| **static 静态链接** | 启动时 linked-in 的 init 调 `register()`（无 dlopen） | 构建/链接期 | 只在已链入的之间按 `--mode` 选 | 入口及跨后端 / OSR 边界的注册调用，开销以 A/B 验收 |
| **dynlink 动态链接（load-time dep）** | dylib 进程启动自动加载，其构造器/init 注册 | 链接期（独立 .dylib 文件） | 按设置选用；换同名 .dylib 即换实现 | 同上 + load-time 解析 |
| **dlopen 运行时按需** | 用到时 core `dlopen` + 查 `z42_register_*` → 注册 | 运行时（看磁盘有无） | 最灵活，部署后丢 dylib 即增减 | 首次 dlopen 开销，之后同上 |

**三种粒度的调用代码完全一样**（都走槽）；差别只在填槽触发点：linked-in init / 自动加载构造器 / 显式 dlopen。

### 6.1 「静态=直接调用」的诚实说明
因为 jit/aot 是**独立 crate**，core 不能写 `crate::jit::run` 字面直调（否则重现 §4 的环）。所以**即使静态链接，也经注册槽调**：
- 注册调用用于模式入口、跨后端回落与 tier / OSR 交接；原生同后端调用继续自己的 ABI；
- 真正热路径「编译代码 → helper」是 cranelift finalize 时 **baked 地址**，三种粒度下都是直调，不受影响。

故「静态直接调用」的实际收益是**省 dlopen / load-time 解析**，而非消除注册边界的调用。要字面零间接只能把组件并回 core（放弃拆分），不划算。

### 6.2 每组件独立选粒度（构建配置）
```
interp = static            # 基线，独立 z42vm 与嵌入皆内置
jit    = static | dynlink | dlopen
aot    = static | dynlink | dlopen
debug  = dynlink | dlopen
gc     = static  仅此一种   # 不参与 dynlink/dlopen，见 §3
```

---

## 7. 不重复包含 + 嵌入双版本

「拆分组件之外都编进 libz42，且不重复」：

- **libz42** = 基座（core + interp + native-interop + host C ABI）。**发 `.a`（静态）+ `.dylib`（动态）两版**。
- **libz42_jit / libz42_aot / libz42_debug** = 薄库，**依赖 libz42、不含 core**（不重复）。各出 `.a`（嵌入静态链）+ `.dylib`（dlopen / 嵌入动态）。
- native 扩展（compression…）维持现有 dlopen 插件。

### 7.1 独立 z42vm（分平台，实现「不重复 + 按需」）

> **此方案在 Rust 下不可行，不按此实施。** 原因：
> - **导不出 core 符号**：LTO 会把 Rust 符号内部化，依赖 crate 里没人引用的 `#[no_mangle]` 也会被剥掉（z42vm 里实际就没有 `z42_register_type`）；导出可执行文件符号的 `-Zexport-executable-symbols` 是 unstable。
> - **core 不止一份**：Rust cdylib（libz42_jit）依赖 z42 crate 时会**静态嵌入自己的一份 z42**，`runtime_config` 的 `OnceLock`、TLAB / SATB 的 `thread_local`、GC 注册表都会各有两份——这是正确性问题，不只是体积。本节和 §7 开头「薄库不含 core」的前提因此都不成立（Rust 的 staticlib 同样会打包全部依赖）。
> - 只有 Rust `dylib` crate-type 能真正共享 core，但它 ABI 不稳定，要 `-C prefer-dynamic` 并随包发 libstd。
> - JIT 是默认模式，拆出去也省不了常规运行的内存。
>
> Rust 下可行的形态只有三种：单体；全部 Rust 代码放进一个 cdylib + 薄 exe；叶子 C-ABI 插件（经函数表互调，不靠全局符号）。若「JIT 可选」成为硬需求，优先出「静态 interp-only」和「静态 full」两种 z42vm。下表保留原方案供对照。

| 平台 | 做法 |
|---|---|
| **Unix（mac/linux）** | z42vm 静态链 `libz42.a` + `-rdynamic`（导出符号）→ `--mode jit` 时 dlopen `libz42_jit.dylib`，其对 core 的未定义符号**回解析到 z42vm 自身**。core 只一份（在 z42vm），JIT 按需，**零重复**。 |
| **Windows** | exe 无 `-rdynamic` 等价 → 用共享 `libz42.dll`，`z42vm.exe` 与 `libz42_jit.dll` 都链它（仍不重复）。 |

### 7.2 嵌入（两版都给，core 只出现一次）
- **动态**：app 链 `libz42.dylib`（要 JIT 再加 `libz42_jit.dylib`，它依赖 libz42）。
- **静态**：app 链 `libz42.a`（要 JIT 再加 `libz42_jit.a`）——`libz42_jit.a` 只含 JIT 代码 + 对 libz42 的未解析引用，最终链接时 core **只出现一次**。

### 7.3 取舍（认账）
1. 独立 z42vm 不再是「单文件含 JIT」：interp 自包含（静态 core），JIT 需 `libz42_jit` 在旁（发行包 `native/` 里有，rpath 指 `../native`）；Windows 另需 `libz42.dll` 在旁。
2. native-interop（libffi ~0.6–1M）默认归 libz42（不单拆）；要更细的「不用 FFI 就不带」可后续再议。

---

## 8. ABI 版本

后端/组件与 libz42 **必须同构建**（非第三方，随包同发）。libz42 导出一个 ABI 版本符号；z42vm dlopen 组件时校验 `组件.abi == 自身.abi`，不一致**拒绝 + 清晰报错**，不做跨版本兼容。

---

## 9. 分阶段实施（增量，post-ROI）

1. **抽出 libz42 基座边界**：把后端现在 `crate::interp::*` / 共享件直够的部分，收敛成稳定的 `core::api` + 注册槽 + observer（最难、最该先做的解耦）。
2. **后端插件 ABI + z42vm dlopen JIT**：先用 JIT 验证整套框架（§4/§6）。§7.1 的「静态 core + `-rdynamic` + dlopen」形态在 Rust 下不可行，此步须改用 §7.1 列出的可行形态之一。
3. **模块化 staticlib/dylib + 嵌入 feature 矩阵 + 文档**：SDK 与 runtime pack 的 `native/` 改铺这套模块化库。
4. **gc 模块化**（编译期可插拔）、**debug 组件**（observer 挂载）、**aot**（M9 时同形状接入）。

每阶段独立可验证、可单独交付；先 JIT 打通框架再推 gc/debug/aot。

---

## 10. 与现有设计的关系

- **当前架构**（单 crate、`#[cfg(feature = "jit"/"aot"/"native-interop"/"interp-only"/"bundled-compression")]`）见 [vm-architecture.md](vm-architecture.md)：本文是它的演进目标，feature gate 是迈向组件化的第一步雏形。
- **嵌入 API / C ABI** 见 [embedding.md](embedding.md)：host 入口归 libz42 基座，不拆。
- **包分发** 见 [runtime-workload-distribution.md](../toolchain/workload-distribution.md)：其 Deferred `runtime-future-jit-cdylib-split` 是本架构的第一个落地切口。
- **分层执行 / OSR / 回收 / hot-reload**（叠在本组件框架之上，引擎内部各自分层）见 [tiered-execution.md](tiered-execution.md)。
- **IR 优化 / 特化 / intrinsic / tier0 基线质量** 见 [ir-specialization.md](ir-specialization-design.md)。
- **zpkg 加载上下文 / 重载 / 卸载回收 / 保留根诊断**（ALC 式，复用 observer/注册基座）见 [load-context.md](load-context.md)。
- **诊断与跟踪**（事件 / 计数 / 时间；统一事件总线，`libz42_debug` 是其 sink）见 [diagnostics.md](diagnostics.md)。
- **AOT 后端**（libz42_aot 组件；cranelift-AOT 复用 JIT 翻译核）见 [aot.md](aot.md)。

## VM 入口与实现归属

Rust 入口由 `vm::{Vm, VmContext, VmCore}` 提供。上下文与共享 Core 的实现位于 `vm/context`；引擎共享的异常构造与原生栈保护位于 `vm/exec`；计数、运行时事件和观察者位于 `vm/diagnostics`。门面负责主上下文的配置与提供方装配，worker 加入、线程注册和 Drop 留在上下文实现中。目录归属不改变状态字段、repr、偏移和同步协议。

门面的具体装配实现在 `src/runtime/src/assembly/`，应用执行与参数服务在 `src/runtime/src/host/`。CLI 与嵌入适配通过 `host::run_app` / `LoadedProgram` 共用加载与调用服务，`assembly` 仅在 crate 内可见；共享启动步骤由应用入口和宿主调用，worker 生命周期仍归 VM 上下文。

CPU 采样通过 `vm/diagnostics/sampling.rs` 的 crate 私有接口装配：VM 只持有采样能力，具体 `profiler/cpu::Sampler` 由门面按既有配置构造。关闭快路使用不可变布尔 gate，启用后才派发到 backend；采样仍发生在原 safepoint，输出与 backend Drop 生命周期保持在原诊断状态边界。

解释器栈分配策略由门面从已解析的 `RuntimeConfig` 转为 `StackAllocPolicy`，主构造存入 `VmCore.exec`，worker 共享同一策略。执行层的 `stack_alloc_enabled(ctx)` 与 `stats_enabled(ctx)` 只查询当前 VM 的不可变输入，不读取进程配置；on/off/stats 的语言行为保持一致，生产配置仍沿用门面原有单例读取时机。
