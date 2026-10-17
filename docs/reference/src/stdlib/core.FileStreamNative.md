<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.FileStreamNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

`src/libraries/z42.core/src/IO/FileStreamNative.z42:7`

文件句柄的 native 原语，供 z42.io 的 `FileStream` 调用。

```z42
public static class FileStreamNative {
    static extern long _FileOpen(string path, int mode)
    static extern int _FileRead(long slotId, byte[] buffer, int offset, int count)
    static extern void _FileWrite(long slotId, byte[] buffer, int offset, int count)
    static extern long _FileSeek(long slotId, long offset, int origin)
    static extern long _FileLength(long slotId)
    static extern long _FilePosition(long slotId)
    static extern void _FileFlush(long slotId)
    static extern void _FileClose(long slotId)
}
```

### static extern long _FileOpen(string path, int mode)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:10`

打开文件，返回 slot 句柄。

### static extern int _FileRead(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:14`

读入 `buffer[offset, offset+count)`，返回读到的字节数。

### static extern void _FileWrite(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:18`

写出 `buffer[offset, offset+count)`。

### static extern long _FileSeek(long slotId, long offset, int origin)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:22`

按 origin 定位，返回新位置。

### static extern long _FileLength(long slotId)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:26`

文件字节长度。

### static extern long _FilePosition(long slotId)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:30`

当前读写位置。

### static extern void _FileFlush(long slotId)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:34`

刷出缓冲。

### static extern void _FileClose(long slotId)

`src/libraries/z42.core/src/IO/FileStreamNative.z42:38`

关闭 slot。

