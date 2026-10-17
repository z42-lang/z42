<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IEnumerable

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Protocols/IEnumerable.z42:11`

可迭代契约：实现此接口表明类型能产出 IEnumerator<T>。

foreach 的可迭代协议是有序回落链：数组 → 整数索引面（`get_Item` + `Count`/`Length`）→
`GetEnumerator()`。所以实现本接口不是 foreach 的前提（`List<T>` 自身仍走索引快路径、
不经 enumerator）。本接口的用途是：
  1. 泛型约束 `where T : IEnumerable<U>` — 算法在抽象层依赖「可迭代」
  2. 显式类型契约 — `class List<T> : IEnumerable<T>` 自我声明
  3. LINQ / iterator chain 的协议层入口

```z42
public interface IEnumerable<T> {
    IEnumerator<T> GetEnumerator()
}
```

### IEnumerator<T> GetEnumerator()

`src/libraries/z42.core/src/Protocols/IEnumerable.z42:13`

返回迭代器。

