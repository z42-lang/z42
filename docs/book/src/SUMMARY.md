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
  - [FFI / interop 表面]()

# 第二部分 · 编译与构建（Compiler & Build）

- [概览](compiler/README.md)
  - [架构总览](compiler/architecture.md)
  - [源代码编译流程（z42c）](compiler/source-compile.md)
  - [工程模型、依赖解析与工作区编译]()
  - [项目构建与发布编排（z42b）]()
  - [编译产物：zpkg / zbc 格式]()
  - [CLI 与诊断工具]()
  - [错误码体系]()

# 第三部分 · 运行时（Runtime / VM）

- [概览](runtime/README.md)
  - [执行模型（interp / jit / aot）]()
  - [IR 与 zbc 二进制格式]()
  - [GC]()
  - [嵌入与跨平台（PAL）]()
  - [native interop ABI]()

# 第四部分 · 标准库（Standard Library）

- [概览](stdlib/README.md)
  - [三层架构与包边界]()
  - [核心包索引]()

# 第五部分 · 工具链（Toolchain）

- [概览](toolchain/README.md)
  - [launcher（z42 命令）]()
  - [workload 与平台发行]()
  - [SDK 与发行包布局]()
  - [编辑器集成（VSCode）](toolchain/editor-integration.md)

# 第六部分 · 开发基础设施（Dev Infrastructure）

- [概览](dev/README.md)
  - [xtask：自举 dev CLI](dev/xtask.md)
  - [构建编排（build / regen）](dev/build.md)
  - [测试门禁（test gate）](dev/test-gate.md)
  - [打包引擎（packages.toml）](dev/packaging.md)

---

# 附录（Appendix）

- [独立主题](appendix/README.md)
