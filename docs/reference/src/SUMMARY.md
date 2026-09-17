# Summary

[前言](README.md)

---

# 语言

- [概览](language/README.md)

  **基础**

  - [源文件的顶层结构](language/syntax.md)
  - [基本类型与字面量](language/types.md)
  - [类型转换](language/conversions.md)
  - [运算符](language/operators.md)
  - [控制流](language/control-flow.md)
  - [字符串](language/strings.md)
  - [数组](language/arrays.md)
  - [元组](language/tuples.md)
  - [集合字面量](language/collection-literals.md)

  **函数**

  - [函数与方法](language/functions.md)
  - [参数修饰符（`ref` / `out` / `in` / `params`）](language/parameter-modifiers.md)
  - [命名实参](language/named-arguments.md)
  - [委托与事件](language/delegates-events.md)
  - [闭包与捕获](language/closures.md)
  - [`methodof`（方法引用表达式）](language/methodof.md)

  **类型定义**

  - [类](language/classes.md)
  - [结构体](language/structs.md)
  - [接口](language/interfaces.md)
  - [enum（枚举）](language/enums.md)
  - [`[Record]` 与主构造器](language/record-attribute.md)
  - [嵌套类型](language/nested-types.md)
  - [partial 类型（跨文件类型定义）](language/partial-types.md)
  - [sealed 修饰符](language/sealed.md)
  - [static 类（只容纳静态成员）](language/static-classes.md)

  **成员**

  - [属性与索引器](language/properties-indexers.md)
  - [readonly 字段](language/readonly-fields.md)
  - [const 编译期常量](language/const.md)
  - [实例构造器与初始化子句](language/constructors.md)
  - [静态构造函数](language/static-constructors.md)
  - [静态成员的名字解析](language/static-members.md)
  - [`[Forward]`（成员转发）](language/member-forwarding.md)
  - [对象初始化器](language/object-initializers.md)
  - [target-typed new（省略构造类名）](language/target-typed-new.md)

  **泛型**

  - [泛型约束（`where` 子句）](language/generic-constraints.md)
  - [泛型方法（方法级类型参数）](language/generic-methods.md)

  **语义**

  - [所有权与内存模型](language/memory-model.md)
  - [迭代（foreach）](language/iteration.md)
  - [模式匹配](language/pattern-matching.md)
  - [异常](language/exceptions.md)

  **组织与可见性**

  - [Namespace 与 Using](language/namespaces.md)
  - [访问权限控制](language/access-control.md)

  **元数据**

  - [特性（Attributes）](language/attributes.md)
  - [`available!()` 符号可用性探测](language/available-macro.md)

# 标准库

- [概览](stdlib/README.md)

# 工具链

- [概览](toolchain/README.md)
  - [工程清单 z42.toml](toolchain/z42-toml.md)

# 嵌入

- [概览](embedding/README.md)
  - [原生互操作（native interop）](embedding/native-interop.md)

# 约定

- [命名约定](conventions/naming.md)

---

# 附录

- [概览](appendix/README.md)
  - [错误码全量表](appendix/error-codes.md)
