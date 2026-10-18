<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Enum

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Enum.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Enum.z42#L6)

enum 反射 API：读 enum 的成员名 / 值（取自 Type 的 enum 元数据块）。

值以底层 i64（`long`）返回，不截断；`Type.IsEnum` 判类型是否为 enum。

```z42
public static class Enum {
    static extern string[] GetNames(Type enumType)
    static extern long[] GetValues(Type enumType)
    static extern string GetName(Type enumType, long value)
    static extern long Parse(Type enumType, string name)
    static extern bool IsDefined(Type enumType, long value)
}
```

成员：[`GetNames`](#Std.Enum.GetNames(Type)) · [`GetValues`](#Std.Enum.GetValues(Type)) · [`GetName`](#Std.Enum.GetName(Type,long)) · [`Parse`](#Std.Enum.Parse(Type,string)) · [`IsDefined`](#Std.Enum.IsDefined(Type,long))

<a id="Std.Enum.GetNames(Type)"></a>
### static extern string[] GetNames(Type enumType)

[`src/libraries/z42.core/src/Enum.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Enum.z42#L9)

enum 成员名（声明序）。非 enum / 无句柄 Type → 空数组。

<a id="Std.Enum.GetValues(Type)"></a>
### static extern long[] GetValues(Type enumType)

[`src/libraries/z42.core/src/Enum.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Enum.z42#L13)

enum 成员值（声明序，底层 i64）。非 enum → 空数组。

<a id="Std.Enum.GetName(Type,long)"></a>
### static extern string GetName(Type enumType, long value)

[`src/libraries/z42.core/src/Enum.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Enum.z42#L17)

值 → 成员名；未命中返回 `""`。

<a id="Std.Enum.Parse(Type,string)"></a>
### static extern long Parse(Type enumType, string name)

[`src/libraries/z42.core/src/Enum.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Enum.z42#L22)

成员名 → 值（GetName 的逆）。未命中抛 Std.Exception（镜像 C# Enum.Parse）。 大小写敏感（z42 大小写敏感语言，无 ignoreCase 变体）。

<a id="Std.Enum.IsDefined(Type,long)"></a>
### static extern bool IsDefined(Type enumType, long value)

[`src/libraries/z42.core/src/Enum.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Enum.z42#L26)

值是否为已定义 enum 成员。非 enum / 无句柄 → false。

