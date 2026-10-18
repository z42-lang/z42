<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Object

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Object.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Object.z42#L7)

对象协议基类：每个 z42 引用类型都隐式继承这些成员。

结构体类型**不**继承 Object——编译器直接为其合成值语义的 Equals / GetHashCode / ToString
（无装箱、无 vtable）。

```z42
public class Object {
    extern Type GetType()
    static extern bool ReferenceEquals(Object? a, Object? b)
    virtual extern bool Equals(Object? other)
    virtual extern int GetHashCode()
    virtual extern string ToString()
}
```

成员：[`GetType`](#Std.Object.GetType()) · [`ReferenceEquals`](#Std.Object.ReferenceEquals(Object?,Object?)) · [`Equals`](#Std.Object.Equals(Object?)) · [`GetHashCode`](#Std.Object.GetHashCode()) · [`ToString`](#Std.Object.ToString())

<a id="Std.Object.GetType()"></a>
### extern Type GetType()

[`src/libraries/z42.core/src/Object.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Object.z42#L10)

返回本对象的运行时类型描述符。

<a id="Std.Object.ReferenceEquals(Object?,Object?)"></a>
### static extern bool ReferenceEquals(Object? a, Object? b)

[`src/libraries/z42.core/src/Object.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Object.z42#L14)

`a` 与 `b` 是否为同一堆对象（或同为 null）。

<a id="Std.Object.Equals(Object?)"></a>
### virtual extern bool Equals(Object? other)

[`src/libraries/z42.core/src/Object.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Object.z42#L18)

值相等——默认退化为引用相等。类型有值语义时与 GetHashCode() 一起覆盖。

<a id="Std.Object.GetHashCode()"></a>
### virtual extern int GetHashCode()

[`src/libraries/z42.core/src/Object.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Object.z42#L22)

默认基于身份的哈希码。覆盖 Equals() 时必须一并覆盖。

<a id="Std.Object.ToString()"></a>
### virtual extern string ToString()

[`src/libraries/z42.core/src/Object.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Object.z42#L26)

人类可读表示——默认为未限定的类型名。

