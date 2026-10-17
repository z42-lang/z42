<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TypeVisibility

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/TypeVisibility.z42:8`

反射 `Type` 的声明可见性。经 `Type.Visibility` 读取；无句柄 Type（基元/数组）报 `Public`。 顶层类型只有 `Public` / `Internal`；嵌套类型可取四种任一。

```z42
public enum TypeVisibility {
    Public
    Private
    Protected
    Internal
}
```

