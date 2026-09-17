# Summary

[前言 · 系统总览](README.md)

---

# 编译器

- [概览](compiler/README.md)
  - [架构总览](compiler/architecture.md)
  - [源代码编译流程](compiler/source-compile.md)
  - [工程模型、依赖解析与工作区编译](compiler/project-model.md)
  - [自举与种子](compiler/self-hosting.md)
  - [Binder 层次](compiler/binder-hierarchy.md)
  - [访问权限强制](compiler/access-control.md)
  - [构造器继承与隐式 base()](compiler/ctor-inheritance.md)
  - [错误码体系：分段与新增流程](compiler/error-codes.md)
  - [语法定制：三层配置机制（未实施）](compiler/syntax-customization.md)
  - [元编程 / 编译期代码生成（未实施）](compiler/metaprogramming.md)
  - [脚本化 charter（未实施）](compiler/scripting-charter.md)

# 运行时

- [概览](runtime/README.md)
  - [AOT（未实施）](runtime/aot.md)
  - [加载期可用性折叠](runtime/availability-folding.md)
  - [组件化运行时（未实施）](runtime/componentized-runtime.md)
  - [并发与 async（未实施）](runtime/concurrency.md)
  - [跨平台](runtime/cross-platform.md)
  - [诊断与性能分析](runtime/diagnostics.md)
  - [诊断事件模型（未实施）](runtime/diagnostics-design.md)
  - [嵌入宿主](runtime/embedding.md)
  - [逃逸分析与栈上分配](runtime/escape-analysis.md)
  - [执行模型（interp / jit / aot）](runtime/execution-model.md)
  - [GC 子系统与 safepoint 协议](runtime/gc.md)
  - [增量 major：SATB 屏障](runtime/gc-incremental-major.md)
  - [GC TLAB：线程本地分配](runtime/gc-tlab.md)
  - [GC 调参与诊断旋钮](runtime/gc-tuning.md)
  - [堆保留诊断（whyRetained）](runtime/heap-diagnostics.md)
  - [hot-reload](runtime/hot-reload.md)
  - [解释器 / JIT 标量语义单一真相源](runtime/interp-jit-semantics.md)
  - [IR intrinsic 特化（未实施）](runtime/ir-specialization-design.md)
  - [JIT 惰性逐函数编译](runtime/jit.md)
  - [JIT 后端（Cranelift）](runtime/jit-design.md)
  - [加载上下文（LoadContext）](runtime/load-context.md)
  - [加载上下文：强制清理（未实施）](runtime/load-context-design.md)
  - [缺符号不再静默](runtime/missing-symbol.md)
  - [Native 扩展加载机制](runtime/native-ext-loader.md)
  - [Native 扩展库范式](runtime/native-extensions.md)
  - [Native 库的布局与解析](runtime/native-libraries.md)
  - [native interop ABI](runtime/object-abi.md)
  - [优化管线](runtime/optimization-pipeline.md)
  - [PAL 平台抽象层](runtime/pal.md)
  - [反射 Type 身份](runtime/reflection-type-identity.md)
  - [运行时设置的实现](runtime/runtime-settings.md)
  - [safepoint 泛化（未实施）](runtime/safepoint-design.md)
  - [静态构造函数的按类型初始化](runtime/static-ctor-init.md)
  - [struct 值语义](runtime/struct-value-semantics.md)
  - [超级指令融合](runtime/superinstr-fusion.md)
  - [内联缓存（PIC）：一条 entry 一个原子量](runtime/inline-cache-publication.md)
  - [同步原语](runtime/sync-primitives.md)
  - [委托与事件的实现](runtime/delegates-events.md)
  - [分层执行（未实施）](runtime/tiered-execution.md)
  - [VM 总体架构](runtime/vm-architecture.md)

# 产物格式

- [概览](formats/README.md)
  - [zbc 字节码格式](formats/zbc.md)
  - [zpkg 包格式](formats/zpkg.md)
  - [IR 指令集](formats/ir.md)

# 标准库

- [概览](stdlib/README.md)

# 工具链

- [概览](toolchain/README.md)

# 测试体系

- [概览](testing/README.md)

# 开发基础设施

- [概览](devinfra/README.md)
  - [本仓命名与目录约定](devinfra/repo-conventions.md)
