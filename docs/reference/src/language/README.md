# 语言

z42 的语法规则：每页讲完一个主题，可跳读。

> 想按顺序学会写 z42 → [学习手册](https://z42-lang.github.io/z42/learn/)。
> 本部分只回答「规则是什么、怎么写、报什么错」。

## 各页

**基础**

- [源文件的顶层结构](syntax.md)
- [基本类型与字面量](types.md)
- [类型转换](conversions.md)
- [运算符](operators.md)
- [控制流](control-flow.md)
- [字符串](strings.md)
- [数组](arrays.md)
- [元组](tuples.md)
- [集合字面量](collection-literals.md)

**函数**

- [函数与方法](functions.md)
- [参数修饰符（`ref` / `params`）](parameter-modifiers.md)
- [命名实参](named-arguments.md)
- [委托与事件](delegates-events.md)
- [闭包与捕获](closures.md)
- [`methodof`（方法引用表达式）](methodof.md)

**类型定义**

- [类](classes.md)
- [继承与多态](inheritance.md)
- [结构体](structs.md)
- [接口](interfaces.md)
- [enum（枚举）](enums.md)
- [`[Record]` 与主构造器](record-attribute.md)
- [嵌套类型](nested-types.md)
- [partial 类型（跨文件类型定义）](partial-types.md)
- [sealed 修饰符](sealed.md)
- [static 类（只容纳静态成员）](static-classes.md)

**成员**

- [属性与索引器](properties-indexers.md)
- [readonly 字段](readonly-fields.md)
- [const 编译期常量](const.md)
- [实例构造器与初始化子句](constructors.md)
- [静态构造函数](static-constructors.md)
- [包级初始化（`[ModuleInit]`）](module-initializers.md)
- [静态成员的名字解析](static-members.md)
- [`[Forward]`（成员转发）](member-forwarding.md)
- [对象初始化器](object-initializers.md)
- [target-typed new（省略构造类名）](target-typed-new.md)

**泛型**

- [泛型约束（`where` 子句）](generic-constraints.md)
- [泛型方法（方法级类型参数）](generic-methods.md)

**语义**

- [所有权与内存模型](memory-model.md)
- [迭代（foreach）](iteration.md)
- [`using` 语句](using-statement.md)
- [模式匹配](pattern-matching.md)
- [异常](exceptions.md)

**组织与可见性**

- [Namespace 与 Using](namespaces.md)
- [访问权限控制](access-control.md)

**元数据**

- [特性（Attributes）](attributes.md)
- [`available!()` 符号可用性探测](available-macro.md)
