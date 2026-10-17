<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ArrayTypeMismatchException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/ArrayTypeMismatchException.z42:8`

往数组里存了元素类型装不下的值：`Array.CopyRange` 的源、目的元素类型互不兼容 （`int[]` → `Point[]`、`Point[]` → `Vector[]`、`Point[]` → `string[]` 这类）。

与 `InvalidCastException` 的分工（同 C#）：两侧**数组类型**本身就不兼容 → 本异常；
数组类型兼容、只是某个**元素**转不过去（`object[]` 里混了一个 string 拷进 `Point[]`）→ `InvalidCastException`。

```z42
public class ArrayTypeMismatchException : Exception {
    ArrayTypeMismatchException(string message)
}
```

### ArrayTypeMismatchException(string message)

`src/libraries/z42.core/src/Exceptions/ArrayTypeMismatchException.z42:10`

用消息构造。

