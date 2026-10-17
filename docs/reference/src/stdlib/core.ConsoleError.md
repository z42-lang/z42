<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ConsoleError

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/Console.z42:24`

标准错误。与 `Console` 并列的静态类（等价 C# 的 `Console.Error`，语义相同）。

```z42
public static class ConsoleError {
    static extern void WriteLine(object value)
    static extern void Write(object value)
    static extern bool IsTerminal()
}
```

### static extern void WriteLine(object value)

`src/libraries/z42.core/src/IO/Console.z42:27`

写 `value` 到 stderr 并换行。

### static extern void Write(object value)

`src/libraries/z42.core/src/IO/Console.z42:31`

写 `value` 到 stderr，不换行。

### static extern bool IsTerminal()

`src/libraries/z42.core/src/IO/Console.z42:35`

stderr 是否连接 tty（重定向到管道或文件时 false）。

