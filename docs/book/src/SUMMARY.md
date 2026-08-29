# Summary

[前言 · 这本书是什么](README.md)

---

# 第一部分 · 语言（Language）

- [概览](language/README.md)
  - [语法与词法]()
  - [类型系统]()
  - [所有权与内存模型]()
  - [内置协议（object protocol）]()
  - [异常与错误处理]()
  - [命名空间与访问控制]()
  - [partial 类型（跨文件类型定义）](language/partial-types.md)
  - [readonly 字段](language/readonly-fields.md)
  - [const 编译期常量](language/const.md)
  - [sealed 修饰符](language/sealed.md)
  - [target-typed new（省略构造类名）](language/target-typed-new.md)
  - [泛型方法（方法级类型参数）](language/generic-methods.md)
  - [属性与索引器（成员访问器）](language/member-accessors.md)
  - [`[Record]` attribute 与主构造器](language/record-attribute.md)
  - [模式匹配（Rust 风格结构化模式）](language/pattern-matching.md)
  - [元组（值元组 `(a, b)`）](language/tuples.md)
  - [FFI / interop 表面]()

# 第二部分 · 编译与构建（Compiler & Build）

- [概览](compiler/README.md)
  - [架构总览](compiler/architecture.md)
  - [源代码编译流程（z42c）](compiler/source-compile.md)
  - [工程模型、依赖解析与工作区编译](compiler/project-model.md)
  - [项目构建与发布编排（z42b）](compiler/project-build.md)
  - [编译产物：zbc 字节码格式](compiler/zbc-format.md)
  - [编译产物：zpkg 包格式](compiler/zpkg-format.md)
  - [CLI 与诊断工具](compiler/tools.md)
  - [错误码体系](compiler/error-codes.md)
  - [类型转换分类器](compiler/type-conversion.md)
  - [访问权限强制](compiler/access-control.md)

# 第三部分 · 运行时（Runtime / VM）

- [概览](runtime/README.md)
  - [执行模型（interp / jit / aot）]()
  - [JIT 惰性逐函数编译](runtime/jit-lazy-compile.md)
  - [解释器 / JIT 标量语义单一真相源](runtime/interp-jit-semantics.md)
  - [优化管线（编译期 IR 优化 + 运行时分层）](runtime/optimization-pipeline.md)
  - [逃逸分析与栈上分配](runtime/escape-analysis-stack-alloc.md)
  - [struct 值语义（内联字节 blob）](runtime/struct-value-semantics.md)
  - [超级指令融合（interp）](runtime/superinstr-fusion.md)
  - [IR 与 zbc 二进制格式]()
  - [GC]()
    - [GC 调参与自动回收 / safepoint 协议](runtime/gc-tuning-and-safepoint.md)
  - [加载上下文（LoadContext / ALC 地基）](runtime/load-context.md)
  - [堆保留诊断（whyRetained）](runtime/heap-diagnostics.md)
  - [诊断与性能分析（采样 profiler / 火焰图 / perfetto）](runtime/diagnostics.md)
  - [嵌入与跨平台（PAL）]()
  - [native interop ABI]()
  - [Native 扩展库（独立 cdylib 机制）](runtime/native-extensions.md)

# 第四部分 · 标准库（Standard Library）

- [概览](stdlib/README.md)
  - [三层架构与包边界]()
  - [核心包索引]()
  - [JSON serde（对象 ↔ JSON）](stdlib/json-serde.md)

# 第五部分 · 工具链（Toolchain）

- [概览](toolchain/README.md)
  - [launcher（z42 命令）]()
  - [workload 与平台发行]()
  - [SDK 与发行包布局]()
  - [测试流水线（两层模型）](toolchain/test-pipeline.md)
  - [编辑器集成（VSCode）](toolchain/editor-integration.md)
  - [REPL 输入完整性判定](toolchain/repl-input-completeness.md)

# 第六部分 · 开发基础设施（Dev Infrastructure）

- [概览](dev/README.md)
  - [xtask：自举 dev CLI](dev/xtask.md)
  - [构建编排（build / regen）](dev/build.md)
  - [测试门禁（test gate）](dev/test-gate.md)
  - [性能基准与回归门禁（benchmark / bench gate）](dev/benchmarking.md)
  - [打包引擎（packages.toml）](dev/packaging.md)

---

# 附录（Appendix）

- [独立主题](appendix/README.md)
