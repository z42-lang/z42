<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IBasicCollection

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Protocols/IBasicCollection.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IBasicCollection.z42#L11)

基础集合契约：集合类型实现此接口表明它有标准的「大小 + 清空」操作。

成员说明：
  - `Count()` — 元素数
  - `IsEmpty()` — 等价于 `Count() == 0` 的快捷方式
  - `Clear()` — 清空所有元素；幂等
  - `AddOne(T)` — 契约层的统一单元素插入，各集合委托其自然 add（Enqueue / Push /
    AddLast / Add），顺序 / 去重策略由实现决定

```z42
public interface IBasicCollection<T> {
    int Count()
    bool IsEmpty()
    void Clear()
    void AddOne(T item)
}
```

成员：[`Count`](#Std.IBasicCollection%3CT%3E.Count()) · [`IsEmpty`](#Std.IBasicCollection%3CT%3E.IsEmpty()) · [`Clear`](#Std.IBasicCollection%3CT%3E.Clear()) · [`AddOne`](#Std.IBasicCollection%3CT%3E.AddOne(T))

<a id="Std.IBasicCollection<T>.Count()"></a>
### int Count()

[`src/libraries/z42.core/src/Protocols/IBasicCollection.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IBasicCollection.z42#L13)

元素数。

<a id="Std.IBasicCollection<T>.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.core/src/Protocols/IBasicCollection.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IBasicCollection.z42#L15)

是否为空（等价于 `Count() == 0`）。

<a id="Std.IBasicCollection<T>.Clear()"></a>
### void Clear()

[`src/libraries/z42.core/src/Protocols/IBasicCollection.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IBasicCollection.z42#L17)

清空所有元素；幂等。

<a id="Std.IBasicCollection<T>.AddOne(T)"></a>
### void AddOne(T item)

[`src/libraries/z42.core/src/Protocols/IBasicCollection.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Protocols/IBasicCollection.z42#L19)

插入一个元素；顺序 / 去重策略由实现决定。

