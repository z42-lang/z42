<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ConsoleError

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/IO/Console.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Console.z42#L24)

标准错误。与 `Console` 并列的静态类（等价 C# 的 `Console.Error`，语义相同）。

```z42
public static class ConsoleError {
    static extern void WriteLine(object value)
    static extern void Write(object value)
    static extern bool IsTerminal()
}
```

成员：[`WriteLine`](#Std.IO.ConsoleError.WriteLine(object)) · [`Write`](#Std.IO.ConsoleError.Write(object)) · [`IsTerminal`](#Std.IO.ConsoleError.IsTerminal())

<a id="Std.IO.ConsoleError.WriteLine(object)"></a>
### static extern void WriteLine(object value)

[`src/libraries/z42.core/src/IO/Console.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Console.z42#L27)

写 `value` 到 stderr 并换行。

<a id="Std.IO.ConsoleError.Write(object)"></a>
### static extern void Write(object value)

[`src/libraries/z42.core/src/IO/Console.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Console.z42#L31)

写 `value` 到 stderr，不换行。

<a id="Std.IO.ConsoleError.IsTerminal()"></a>
### static extern bool IsTerminal()

[`src/libraries/z42.core/src/IO/Console.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/Console.z42#L35)

stderr 是否连接 tty（重定向到管道或文件时 false）。

