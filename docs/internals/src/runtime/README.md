# 运行时

z42vm —— Rust 写的虚拟机：加载 `.zpkg`、执行字节码、管理内存。

> 想知道**怎么调**运行时旋钮（`--set` / `Z42_GC_*`）→ [语言与库参考](https://z42-lang.github.io/z42/reference/)。
> 本部分只回答「怎么实现的、为什么这样、在哪改」。

代码在 `src/runtime/`（crate `z42`）；产物格式见[产物格式](../formats/README.md)。

## 从哪读起

1. [VM 总体架构](vm-architecture.md) —— 全景
2. [执行模型](execution-model.md) —— interp / jit / aot 三档怎么协作
3. [GC 子系统与 safepoint 协议](gc.md) —— **改任何涉及对象生命周期的代码前必读**

## 按主题

| 主题 | 页 |
|---|---|
| **执行** | [执行模型](execution-model.md) · [JIT 惰性逐函数编译](jit.md) · [解释器/JIT 语义单一真相源](interp-jit-semantics.md) · [超级指令融合](superinstr-fusion.md) · [优化管线](optimization-pipeline.md) · [加载期可用性折叠](availability-folding.md) |
| **内存与 GC** | [GC 子系统与 safepoint 协议](gc.md) · [GC 调参与诊断旋钮](gc-tuning.md) · [TLAB 线程本地分配](gc-tlab.md) · [增量 major：SATB 屏障](gc-incremental-major.md) · [逃逸分析与栈上分配](escape-analysis.md) · [堆保留诊断](heap-diagnostics.md) |
| **对象与类型** | [struct 值语义](struct-value-semantics.md) · [反射 Type 身份](reflection-type-identity.md) · [静态构造函数初始化](static-ctor-init.md) · [缺符号不再静默](missing-symbol.md) |
| **加载** | [加载上下文](load-context.md) · [运行时设置的实现](runtime-settings.md) |
| **并发** | [同步原语](sync-primitives.md) · [并发与 async](concurrency.md) |
| **原生互操作** | [native interop ABI](object-abi.md) · [Native 扩展加载机制](native-ext-loader.md) · [Native 扩展库范式](native-extensions.md) · [Native 库布局与解析](native-libraries.md) |
| **平台** | [PAL 平台抽象层](pal.md) · [跨平台](cross-platform.md) · [嵌入宿主](embedding.md) |
| **诊断** | [诊断与性能分析](diagnostics.md) |

## 设计已定 / 未实施

这些页描述**还没做**的设计，页头都标了状态——别当成当前行为：

[AOT](aot.md) · [分层执行](tiered-execution.md) · [组件化运行时](componentized-runtime.md) ·
[热重载](hot-reload.md) · [safepoint 泛化](safepoint-design.md) · [诊断事件模型](diagnostics-design.md) ·
[加载上下文强制清理](load-context-design.md) · [IR intrinsic 特化](ir-specialization-design.md) ·
[JIT Cranelift 后端](jit-design.md)
