<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Console

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/Console.z42:5`

标准输出。所有方法都收 `object`——任何类型都能直接传（`Console.WriteLine(42)`、 `Console.Write(true)` 等，primitive 在 VM 层自动装箱）。

```z42
public static class Console {
    static extern void WriteLine(object value)
    static extern void Write(object value)
    static extern string ReadLine()
    static extern bool IsTerminal()
}
```

### static extern void WriteLine(object value)

`src/libraries/z42.core/src/IO/Console.z42:8`

写 `value` 并换行。

### static extern void Write(object value)

`src/libraries/z42.core/src/IO/Console.z42:12`

写 `value`，不换行。

### static extern string ReadLine()

`src/libraries/z42.core/src/IO/Console.z42:16`

从 stdin 读一行（去掉行尾的 `\n` / `\r`）。EOF 与空行都返回 ""。

### static extern bool IsTerminal()

`src/libraries/z42.core/src/IO/Console.z42:20`

stdout 是否连接 tty（重定向到管道或文件时 false）。

