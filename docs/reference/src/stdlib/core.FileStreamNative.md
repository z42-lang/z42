<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.FileStreamNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.IO`

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L7)

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

成员：[`_FileOpen`](#Std.IO.FileStreamNative._FileOpen(string,int)) · [`_FileRead`](#Std.IO.FileStreamNative._FileRead(long,byte[],int,int)) · [`_FileWrite`](#Std.IO.FileStreamNative._FileWrite(long,byte[],int,int)) · [`_FileSeek`](#Std.IO.FileStreamNative._FileSeek(long,long,int)) · [`_FileLength`](#Std.IO.FileStreamNative._FileLength(long)) · [`_FilePosition`](#Std.IO.FileStreamNative._FilePosition(long)) · [`_FileFlush`](#Std.IO.FileStreamNative._FileFlush(long)) · [`_FileClose`](#Std.IO.FileStreamNative._FileClose(long))

<a id="Std.IO.FileStreamNative._FileOpen(string,int)"></a>
### static extern long _FileOpen(string path, int mode)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L10)

打开文件，返回 slot 句柄。

<a id="Std.IO.FileStreamNative._FileRead(long,byte[],int,int)"></a>
### static extern int _FileRead(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L14)

读入 `buffer[offset, offset+count)`，返回读到的字节数。

<a id="Std.IO.FileStreamNative._FileWrite(long,byte[],int,int)"></a>
### static extern void _FileWrite(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L18)

写出 `buffer[offset, offset+count)`。

<a id="Std.IO.FileStreamNative._FileSeek(long,long,int)"></a>
### static extern long _FileSeek(long slotId, long offset, int origin)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L22)

按 origin 定位，返回新位置。

<a id="Std.IO.FileStreamNative._FileLength(long)"></a>
### static extern long _FileLength(long slotId)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L26)

文件字节长度。

<a id="Std.IO.FileStreamNative._FilePosition(long)"></a>
### static extern long _FilePosition(long slotId)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L30)

当前读写位置。

<a id="Std.IO.FileStreamNative._FileFlush(long)"></a>
### static extern void _FileFlush(long slotId)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L34)

刷出缓冲。

<a id="Std.IO.FileStreamNative._FileClose(long)"></a>
### static extern void _FileClose(long slotId)

[`src/libraries/z42.core/src/IO/FileStreamNative.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/IO/FileStreamNative.z42#L38)

关闭 slot。

