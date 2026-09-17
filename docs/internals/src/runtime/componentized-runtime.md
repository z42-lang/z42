# 组件化运行时（Componentized Runtime）

> **状态：DESIGN（目标架构，未实施）** · 创建 2026-06-21
>
> 本文是 z42 运行时**未来的组件化目标架构**：把执行后端、GC、调试等从单一 `z42` crate 中拆成可独立交付的组件，支持「独立 z42vm 运行时按需加载」与「嵌入按需链接」。
>
> **实施触发**：ROI 出现（如某分发渠道对 z42vm 单二进制体积敏感、或 interp/jit 需按需下发）。在此之前仅作为目标设计存在；`runtime-workload-distribution.md` 的 Deferred `runtime-future-jit-cdylib-split` 指向本文。
>
> 当前运行时架构（单 crate + feature gate）见 [vm-architecture.md](vm-architecture.md)；运行时/包分发见 [runtime-workload-distribution.md](../../../internals/src/toolchain/workload-distribution.md)。

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

### 4.1 唯一的潜在环
当前（单 crate）实测依赖：
- **后端 → core**：大量直接边（`crate::interp::exec_function` 回退、helpers 调 core、共享 `VmContext`）。
- **core → jit**：**只有一条**——[src/runtime/src/vm.rs](../../../../src/runtime/src/vm.rs) 的 `ExecMode::Jit => crate::jit::run(...)`（模式派发）。
- **interp → jit**：零（无 tiered 提升环）。

拆分后 `libz42`（含 vm.rs 派发）若仍直调 `jit::run` → `libz42 → libz42_jit → libz42` 成环。**Cargo 在 crate 层禁止循环依赖**，会直接编译失败，所以这条边**必须**反转。

### 4.2 反转：注册槽（即 ext.rs 既有模式）
core 不静态调后端，改持注册槽；后端在加载/初始化时把自己注册进来：

```rust
// libz42（core）—— 不引用任何后端 crate
static JIT: OnceLock<BackendApi> = OnceLock::new();
pub fn register_jit(api: BackendApi) { let _ = JIT.set(api); }

// vm.rs 派发永远只这么写：
ExecMode::Jit => match JIT.get() {
    Some(b) => (b.run)(ctx, module, entry),
    None    => bail!("JIT backend not available; dlopen libz42_jit or link --features jit"),
}
```
```rust
// libz42_jit —— 依赖 libz42，反向注册
pub fn register() -> BackendApi { BackendApi { run: jit_run, compile: jit_compile, /* … */ } }
#[no_mangle] pub extern "C" fn z42_register_jit() { z42::register_jit(register()); }
```

结果：**jit → core 直接；core → 后端走抽象槽（不静态依赖后端 crate）→ 无环**，Cargo 通过。

这与 [src/runtime/src/native/ext.rs](../../../../src/runtime/src/native/ext.rs) 现对 native 扩展的做法（core 持 `ExtBuiltinTable`、插件注册 fn 指针）**完全同构**；调试钩子复用 [src/runtime/src/observer.rs](../../../../src/runtime/src/observer.rs)（JIT 编译事件已 fire observer）。框架地基已有雏形。

> 将来若加 tiered（interp 把热函数提升到 JIT），那条 interp→jit 也走同一注册槽 → 依然无环。

---

## 5. 切换语义：可用集 vs 选用

两个独立维度，勿混：
- **可用集（availability）**：这个部署里**存在哪些组件**。
- **选用（selection）**：某次运行**用哪个**（`--mode interp|jit`、`--debug`），在模块入口处定，**不是函数执行中途切**（当前无 tiered/OSR）。

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
| **static 静态链接** | 启动时 linked-in 的 init 调 `register()`（无 dlopen） | 构建/链接期 | 只在已链入的之间按 `--mode` 选 | 槽 1 次间接（模块入口粒度，忽略） |
| **dynlink 动态链接（load-time dep）** | dylib 进程启动自动加载，其构造器/init 注册 | 链接期（独立 .dylib 文件） | 按设置选用；换同名 .dylib 即换实现 | 同上 + load-time 解析 |
| **dlopen 运行时按需** | 用到时 core `dlopen` + 查 `z42_register_*` → 注册 | 运行时（看磁盘有无） | 最灵活，部署后丢 dylib 即增减 | 首次 dlopen 开销，之后同上 |

**三种粒度的调用代码完全一样**（都走槽）；差别只在填槽触发点：linked-in init / 自动加载构造器 / 显式 dlopen。

### 6.1 「静态=直接调用」的诚实说明
因为 jit/aot 是**独立 crate**，core 不能写 `crate::jit::run` 字面直调（否则重现 §4 的环）。所以**即使静态链接，也经注册槽调**：
- 槽间接**只在模块入口粒度**（每次 `Vm::run`，非每指令）→ 可忽略；
- 真正热路径「编译代码 → helper」是 cranelift finalize 时 **baked 地址**，三种粒度下都是直调，不受影响。

故「静态直接调用」的实际收益是**省 dlopen / load-time 解析**，而非省那一次槽间接。要字面零间接只能把组件并回 core（放弃拆分），不划算。

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
2. **后端插件 ABI + z42vm dlopen JIT**：先用 JIT 验证整套框架（§4/§6），方案见 §7.1。
3. **模块化 staticlib/dylib + 嵌入 feature 矩阵 + 文档**：SDK 与 runtime pack 的 `native/` 改铺这套模块化库。
4. **gc 模块化**（编译期可插拔）、**debug 组件**（observer 挂载）、**aot**（M9 时同形状接入）。

每阶段独立可验证、可单独交付；先 JIT 打通框架再推 gc/debug/aot。

---

## 10. 与现有设计的关系

- **当前架构**（单 crate、`#[cfg(feature = "jit"/"aot"/"native-interop"/"interp-only"/"bundled-compression")]`）见 [vm-architecture.md](vm-architecture.md)：本文是它的演进目标，feature gate 是迈向组件化的第一步雏形。
- **嵌入 API / C ABI** 见 [embedding.md](embedding.md)：host 入口归 libz42 基座，不拆。
- **包分发** 见 [runtime-workload-distribution.md](../../../internals/src/toolchain/workload-distribution.md)：其 Deferred `runtime-future-jit-cdylib-split` 是本架构的第一个落地切口。
- **分层执行 / OSR / 回收 / hot-reload**（叠在本组件框架之上，引擎内部各自分层）见 [tiered-execution.md](tiered-execution.md)。
- **IR 优化 / 特化 / intrinsic / tier0 基线质量** 见 [ir-specialization.md](ir-specialization-design.md)。
- **zpkg 加载上下文 / 重载 / 卸载回收 / 保留根诊断**（ALC 式，复用 observer/注册基座）见 [load-context.md](load-context.md)。
- **诊断与跟踪**（事件 / 计数 / 时间；统一事件总线，`libz42_debug` 是其 sink）见 [diagnostics.md](diagnostics.md)。
- **AOT 后端**（libz42_aot 组件；cranelift-AOT 复用 JIT 翻译核）见 [aot.md](aot.md)。
