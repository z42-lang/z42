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

# 第二部分 · 编译器（Compiler）

- [概览](compiler/README.md)
  - [自举与种子（self-hosting）]()
  - [pipeline 各阶段]()
  - [zpkg 产物与依赖索引]()
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
  - [xtask：自举 dev CLI](toolchain/xtask.md)
  - [构建编排（build / regen）](toolchain/build.md)
  - [测试门禁（test gate）]()
  - [发行打包（packages.toml）]()
  - [launcher 与 workload]()

---

# 附录（Appendix）

- [独立主题](appendix/README.md)
