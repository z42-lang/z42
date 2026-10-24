<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ModuleSearch

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

[`src/libraries/z42.core/src/Runtime/ModuleSearch.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleSearch.z42#L9)

本进程解析 zpkg 依赖时会去哪些目录找（**解析后的实际搜索序**）。

相对项已按 **entry zpkg 所在目录**解析（不是 cwd）、`probing-paths` 里的 `*` /
`**` 已展开、不存在的目录已剔除。与 `Std.Runtime.RuntimeConfig` 的分工：
`RuntimeConfig.Get("probing-paths")` 给原始配置串（未展开、不知道相对谁），本类给
解析后的实际搜索序。

```z42
public static class ModuleSearch {
    static extern string[] Dirs()
}
```

成员：[`Dirs`](#Std.Runtime.ModuleSearch.Dirs())

<a id="Std.Runtime.ModuleSearch.Dirs()"></a>
### static extern string[] Dirs()

[`src/libraries/z42.core/src/Runtime/ModuleSearch.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleSearch.z42#L13)

按序的搜索目录。非 app 路径（纯 host 嵌入）→ 空数组。空数组是**有意义的答案**： 说明这个进程没有任何 zpkg 搜索路径，不是错误。

