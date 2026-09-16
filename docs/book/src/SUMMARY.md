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
  - [static 类（只容纳静态成员）](language/static-classes.md)
  - [实例构造器与初始化子句（`: base()` / `: this()`）](language/constructors.md)
  - [静态构造函数（惰性、按类型）](language/static-constructors.md)
  - [静态成员的名字解析（裸名 ≡ `C.x`）](language/static-members.md)
  - [target-typed new（省略构造类名）](language/target-typed-new.md)
  - [泛型总体设计（类型系统）](language/generics.md)
  - [泛型方法（方法级类型参数）](language/generic-methods.md)
  - [泛型约束（`where` 子句）](language/generic-constraints.md)
  - [属性与索引器（成员访问器）](language/member-accessors.md)
  - [`[Record]` attribute 与主构造器](language/record-attribute.md)
  - [模式匹配（Rust 风格结构化模式）](language/pattern-matching.md)
  - [`available!()` 符号可用性探测](language/available-macro.md)
  - [元组（值元组 `(a, b)`）](language/tuples.md)
  - [enum（枚举）](language/enums.md)
  - [命名实参](language/named-arguments.md)
  - [`methodof`（方法引用表达式）](language/methodof.md)
  - [`[Forward]`（成员转发）](language/member-forwarding.md)
  - [FFI / interop 表面]()

# 第二部分 · 编译与构建（Compiler & Build）

- [概览](compiler/README.md)
  - [项目构建与发布编排（z42b）](compiler/project-build.md)
  - [CLI 与诊断工具](compiler/tools.md)


# 第四部分 · 标准库（Standard Library）

- [概览](stdlib/README.md)
  - [三层架构与包边界]()
  - [核心包索引]()
  - [JSON serde（对象 ↔ JSON）](stdlib/json-serde.md)
  - [`Std.Runtime.RuntimeConfig`（只读查询运行时设置）](stdlib/runtime-config.md)
  - [`Std.Runtime.AppProperties`（应用自定义配置）](stdlib/app-properties.md)

# 第五部分 · 工具链（Toolchain）

- [概览](toolchain/README.md)
  - [z42 命令参考](toolchain/cli.md)
  - [workload 与平台发行]()
  - [SDK 与发行包布局]()
  - [测试流水线（两层模型）](toolchain/test-pipeline.md)
  - [部署模型（正交轴：自包含/apphost/single-file/AOT）](toolchain/deployment-model.md)
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
