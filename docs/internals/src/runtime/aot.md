# AOT 后端（Ahead-of-Time Compilation）

> **状态：DESIGN（未实施，aot.rs 为 stub）** · 创建 2026-06-21
>
> 设计 z42 的 AOT 后端：把 `.zbc` 提前编译成目标原生码。**iOS/wasm 无运行时 JIT，性能命脉靠 AOT + interp 分层**；desktop/android 上 AOT 作 baseline 与 JIT 共存。
>
> **关键决策：AOT 复用 cranelift（不走 LLVM），与 JIT 共享翻译层。** 与 [tiered-execution.md](tiered-execution.md)（AOT 作一档）、[safepoint.md](safepoint-design.md)（精确 GC stack map）、[object-abi.md](object-abi.md)、[componentized-runtime.md](componentized-runtime.md)（libz42_aot 组件）对接。

---

## 1. 现状
- [aot.rs](../../../../src/runtime/src/aot.rs) = **stub**（`run()` 报未实现），原计划 **LLVM/inkwell（M9）**。
- 但 z42 **已用 cranelift 做 JIT**（codegen/frontend/jit/module/native）；`translate.rs` 把 zbc→cranelift IR。
- cranelift 有 **`cranelift-object`（ObjectModule）**，与 `JITModule` **共用 `cranelift_module::Module` trait** → 能发 `.o`。

## 2. 后端决策（D1）：cranelift-AOT，复用 JIT 翻译层（修订 M9 的 LLVM 计划）
把 `translate_function` 从硬绑 `JITModule` **泛化为 `M: cranelift_module::Module`** → **同一份 zbc→cranelift IR 翻译,两个输出**：
- **JIT**：`JITModule`（内存可执行页）
- **AOT**：`ObjectModule`（`.o` 对象文件）

理由：
- **复用** translate.rs（已验证翻译 + helper），不写第二套 codegen。
- **比 LLVM 轻**：无 LLVM 工具链重依赖、编译更快。
- **精确 GC 免费**：cranelift 支持 user stack map（[safepoint.md §7](safepoint-design.md)）→ AOT 也精确；LLVM statepoint 很痛。
- **接组件化**：libz42_jit + libz42_aot **共享 cranelift 翻译核**。

→ **roadmap M9 由 "LLVM/inkwell" 改为 cranelift-AOT**；LLVM 仅在将来追峰值性能时作第二后端。

## 3. 混合模式（D2）：AOT + JIT + interp，按平台

**可用集随平台变**（对接 [tiered-execution.md](tiered-execution.md) 切换语义 + [safepoint.md](safepoint-design.md) 后端注册槽）：

| 平台 | 后端组合 | 优化来源 |
|---|---|---|
| **iOS / wasm（无 JIT）** | **AOT + interp(内部分层)** | AOT 预烤 + interp quickening/特化 |
| **desktop / android（有 JIT）** | **AOT + JIT(分层) + interp** | AOT baseline + JIT 运行时再优化 + interp 兜底 |

**AOT 与 JIT 共存（关键）** —— 对标 **.NET ReadyToRun + Tiered JIT** / **Android ART(dex2oat + JIT + interp)**：
- **AOT = baseline/启动档**：随包静态 zbc 预编译,**无 warmup、启动快**。
- **JIT = 运行时按 profile 再优化**:把热的 AOT 函数**重编到更高 tier**(带运行时反馈);**deopt** 回退到 AOT 或 interp(复用 safepoint/OSR/deopt 基建)。
- **interp = 兜底 + 动态**(反射构造、运行时加载 zpkg、eval/REPL)。
- **前提**:AOT 码须携带 **GC stack map + (可选)deopt/OSR 元数据**,才能参与统一 safepoint/tier 机制——cranelift-AOT 能发这些(又一个不选 LLVM 的理由)。

## 4. 产出与时机（D3）
- **iOS 不能在设备 codegen** → AOT 是 **host 侧 build-time 交叉编译**(macOS host 产 target `.o`)。
- 产物**链进 app / framework**;接 `z42 export ios` + runtime pack 打包流程。
- desktop:AOT 可选(启动加速),产 `.o`/静态库随 SDK 或按需。

## 5. wasm（D4）
- z42vm-on-wasm 本身已是 AOT(Rust→wasm)跑 interp;**用户码 AOT-to-wasm 延后**。iOS 是 AOT 首要驱动。

## 6. 精确 GC（D5）
AOT 码发 **cranelift user stack map**(与 JIT 共享机制)→ AOT 路径精确 GC;派生指针契约同 [safepoint.md §7](safepoint-design.md)。

## 7. 代码复用（D6）
`translate.rs` 泛化 `M: cranelift_module::Module`;JIT(JITModule)/ AOT(ObjectModule) **共享一套翻译 + helper 声明**;差异仅"输出到内存 vs 对象文件" + 符号/重定位处理。

## 8. AOT × load-context
- AOT 化的是**随包静态 zpkg**;**运行时动态加载的 zpkg**([load-context.md](load-context.md))→ iOS 走 interp、desktop 走 JIT。AOT 不覆盖动态加载。
- AOT 码属某 context;context 卸载时其 AOT 码(若 dlopen 的 AOT 模块)随之卸载(组件化 dlopen)。静态链进 app 的 AOT 不卸载(随进程)。

## 9. 决策记录（2026-06-21）
| # | 决策 |
|---|---|
| D1 后端 | **cranelift-AOT**(复用 JIT 翻译,泛化 Module trait,cranelift-object 发 .o);**修订 M9 LLVM 计划**;LLVM 延后 |
| D2 混合 | **AOT + JIT + interp**(JIT 平台);iOS/wasm = AOT + interp。AOT baseline 与 JIT 运行时再优化共存(.NET R2R / ART 模型) |
| D3 产出 | host build-time 交叉编译 → 链进 app/framework(iOS 必须);接 export/打包 |
| D4 wasm | 用户码 AOT 延后,iOS 优先 |
| D5 精确 GC | cranelift stack map(与 JIT 共享) |
| D6 复用 | translate.rs 泛化 `M: Module`,JIT/AOT 共享翻译 |

## 10. 分阶段
1. translate.rs 泛化 `M: Module`(JIT 现状不变,抽出 trait 边界)。
2. cranelift-object AOT 路径:zbc → `.o`(host 交叉编译);最简全 AOT + interp 兜底跑通(iOS 模型)。
3. AOT GC stack map + 与统一 safepoint 接。
4. AOT × JIT 共存(desktop):AOT baseline + JIT 重优化 + deopt(依赖 tiered/safepoint 落地)。
5. export/打包集成(iOS framework / desktop 静态库)。

## 11. 交叉引用
- AOT 作 tier / 与 JIT 共存 / deopt：[tiered-execution.md](tiered-execution.md) · 精确 GC stack map / 后端注册槽：[safepoint.md](safepoint-design.md)
- 值/对象 ABI：[object-abi.md](object-abi.md) · 组件化(libz42_aot)：[componentized-runtime.md](componentized-runtime.md)
- 动态加载边界：[load-context.md](load-context.md) · 当前架构：[vm-architecture.md](vm-architecture.md)
- export/打包：[../toolchain/runtime-workload-distribution.md](../../../internals/src/toolchain/workload-distribution.md)
