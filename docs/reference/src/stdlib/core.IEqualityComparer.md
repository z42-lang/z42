<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IEqualityComparer

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Protocols/IEqualityComparer.z42:13`

外部相等性 + 哈希契约：比较器对外部对象判等并算哈希。

与 IEquatable 的「自比」模式区分：
  - IEquatable.Equals(other) / GetHashCode() — 实例自身相等性 + hash
  - IEqualityComparer<T>.Equals(x, y) / GetHashCode(obj) — 比较器对外部对象

用于 Dictionary / HashSet 的自定义键比较器（如大小写不敏感的字符串字典、
按结构相等比较的对象集合等）。

实现者必须保证：Equals(x, y) == true ⟹ GetHashCode(x) == GetHashCode(y)。

```z42
public interface IEqualityComparer<T> {
    bool Equals(T x, T y)
    int GetHashCode(T obj)
}
```

### bool Equals(T x, T y)

`src/libraries/z42.core/src/Protocols/IEqualityComparer.z42:15`

比较 x 与 y 是否相等。

### int GetHashCode(T obj)

`src/libraries/z42.core/src/Protocols/IEqualityComparer.z42:17`

返回 obj 的哈希码，与 Equals 保持一致。

