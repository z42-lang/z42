<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IComparable

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Protocols/IComparable.z42:8`

自比较接口：返回负 / 0 / 正，表示实现方与另一个同类值的大小关系（C# / Rust 惯例）。

`Self` 指实现方自身，故本接口非泛型：约束位写 `where T : IComparable` 即可，不必
再写 F-bounded 样板 `where T : IComparable<T>`。
要比较两个不同类型，用 IComparer<T>（外部比较器形态）——那正是它与本接口的分工。

```z42
public interface IComparable {
    int CompareTo(Self other)
}
```

### int CompareTo(Self other)

`src/libraries/z42.core/src/Protocols/IComparable.z42:10`

返回负 / 0 / 正，表示 this 小于 / 等于 / 大于 other。

