<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IEquatable

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Protocols/IEquatable.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IEquatable.z42#L8)

自相等接口：实现方提供 `Equals(Self)` 与 `GetHashCode()`，使其可作为字典键 / 哈希集合元素。

`Self` 指实现方自身，故本接口非泛型：约束位写 `where T : IEquatable` 即可，不必
再写 F-bounded 样板 `where T : IEquatable<T>`。
要比较两个不同类型，用 IEqualityComparer<T>（外部比较器形态）——那正是它与本接口的分工。

```z42
public interface IEquatable {
    bool Equals(Self other)
    int GetHashCode()
}
```

成员：[`Equals`](#Std.IEquatable.Equals(Self)) · [`GetHashCode`](#Std.IEquatable.GetHashCode())

<a id="Std.IEquatable.Equals(Self)"></a>
### bool Equals(Self other)

[`src/libraries/z42.core/src/Protocols/IEquatable.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IEquatable.z42#L10)

与另一个同类值做相等比较。

<a id="Std.IEquatable.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Protocols/IEquatable.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IEquatable.z42#L12)

返回哈希码，与 Equals 保持一致（相等对象哈希码必须相同）。

