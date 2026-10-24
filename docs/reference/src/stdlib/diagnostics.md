<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.diagnostics/ -->

# z42.diagnostics —— 日志与运行时自省。

> 包路径 `src/libraries/z42.diagnostics/`；命名空间 `Std.Diagnostics`

三组互不相干的能力：

1. **日志**（`Log` / `LogLevel` / `LogFields`）—— 全局静态门面，按最低级别过滤，
   写 **stderr**，logfmt 风格的结构化字段可选。
2. **运行时计数**（`RuntimeStats` / `RuntimeCounters`）—— 在脚本里读 VM 的 alloc / GC /
   异常 / JIT 累计计数快照。
3. **堆保留诊断**（`Heap` / `Retainer` / `RootRef` / `RootKind`）—— 回答「这个对象为什么
   还活着、被谁钉住」。

没有 tracing / span、没有 metrics 导出、没有计时器（计时用 `Std.Time.Stopwatch`，见
[Stopwatch](core.Stopwatch.md)）。

## 不支持

- **named logger**：只有一个全局 `Log`，没有 `Log.Get("module.x")`、没有按模块独立的
  最低级别。要分模块就自己在 message 或 `LogFields` 里带前缀。
- **可插拔 sink**：目的地硬编码 stderr，不能加文件 / 网络 / OTel collector。
- **JSON 日志**：输出是人类可读文本 + logfmt 字段，没有整行 JSON 模式。
- **自定义格式 / 时间戳格式**：`[LEVEL ISO8601] msg` 固定，只能通过 `FormatHeader`
  自己拼一行再写 stderr。
- **异步 / 批量缓冲**：每条直接写 stderr。
- **tracing / span / metrics**：没有。计时用 `Std.Time.Stopwatch`。
- **完整引用链**：`RetainingRoots` 只给根的**类别**，不给「哪个 static 字段 / 哪个局部」
  的名字，也不给 target → root 的整条路径。

## Heap

[`src/libraries/z42.diagnostics/src/Heap.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Heap.z42#L4)

堆保留诊断：回答「某对象为什么没被回收、被谁钉住」。

```z42
public static class Heap {
    static extern Retainer[] DirectReferrers(object target)
    static extern RootRef[] RetainingRoots(object target)
}
```

成员：[`DirectReferrers`](#Std.Diagnostics.Heap.DirectReferrers(object)) · [`RetainingRoots`](#Std.Diagnostics.Heap.RetainingRoots(object))

<a id="Std.Diagnostics.Heap.DirectReferrers(object)"></a>
### static extern Retainer[] DirectReferrers(object target)

[`src/libraries/z42.diagnostics/src/Heap.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Heap.z42#L7)

直接持有 `target` 引用的堆对象（object / array）列表；非堆对象返回空。

<a id="Std.Diagnostics.Heap.RetainingRoots(object)"></a>
### static extern RootRef[] RetainingRoots(object target)

[`src/libraries/z42.diagnostics/src/Heap.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Heap.z42#L11)

从 `target` 反向可达的 GC 根（类别级，去重）；非堆对象返回空。

## Log

[`src/libraries/z42.diagnostics/src/Log.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L32)

全局单例日志门面，无 named logger、无 sink 配置。

```z42
public static class Log {
    static void SetMinLevel(int level)
    static int GetMinLevel()
    static bool IsEnabled(int level)
    static void Trace(string msg)
    static void Debug(string msg)
    static void Info(string msg)
    static void Warn(string msg)
    static void Error(string msg)
    static void Trace(string msg, LogFields fields)
    static void Debug(string msg, LogFields fields)
    static void Info(string msg, LogFields fields)
    static void Warn(string msg, LogFields fields)
    static void Error(string msg, LogFields fields)
    static string FormatHeader(int level, DateTime now)
}
```

成员：[`SetMinLevel`](#Std.Diagnostics.Log.SetMinLevel(int)) · [`GetMinLevel`](#Std.Diagnostics.Log.GetMinLevel()) · [`IsEnabled`](#Std.Diagnostics.Log.IsEnabled(int)) · [`Trace`](#Std.Diagnostics.Log.Trace(string)) · [`Debug`](#Std.Diagnostics.Log.Debug(string)) · [`Info`](#Std.Diagnostics.Log.Info(string)) · [`Warn`](#Std.Diagnostics.Log.Warn(string)) · [`Error`](#Std.Diagnostics.Log.Error(string)) · [`Trace`](#Std.Diagnostics.Log.Trace(string,LogFields)) · [`Debug`](#Std.Diagnostics.Log.Debug(string,LogFields)) · [`Info`](#Std.Diagnostics.Log.Info(string,LogFields)) · [`Warn`](#Std.Diagnostics.Log.Warn(string,LogFields)) · [`Error`](#Std.Diagnostics.Log.Error(string,LogFields)) · [`FormatHeader`](#Std.Diagnostics.Log.FormatHeader(int,DateTime))

<a id="Std.Diagnostics.Log.SetMinLevel(int)"></a>
### static void SetMinLevel(int level)

[`src/libraries/z42.diagnostics/src/Log.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L36)

设置全局最低输出级别，低于该级别的调用被整条丢弃。

<a id="Std.Diagnostics.Log.GetMinLevel()"></a>
### static int GetMinLevel()

[`src/libraries/z42.diagnostics/src/Log.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L41)

返回全局最低输出级别。

<a id="Std.Diagnostics.Log.IsEnabled(int)"></a>
### static bool IsEnabled(int level)

[`src/libraries/z42.diagnostics/src/Log.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L46)

该级别当前是否会输出；构造昂贵的 message 前先问一句。

<a id="Std.Diagnostics.Log.Trace(string)"></a>
### static void Trace(string msg)

[`src/libraries/z42.diagnostics/src/Log.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L51)

输出一条 TRACE 级别日志。

<a id="Std.Diagnostics.Log.Debug(string)"></a>
### static void Debug(string msg)

[`src/libraries/z42.diagnostics/src/Log.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L53)

输出一条 DEBUG 级别日志。

<a id="Std.Diagnostics.Log.Info(string)"></a>
### static void Info(string msg)

[`src/libraries/z42.diagnostics/src/Log.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L55)

输出一条 INFO 级别日志。

<a id="Std.Diagnostics.Log.Warn(string)"></a>
### static void Warn(string msg)

[`src/libraries/z42.diagnostics/src/Log.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L57)

输出一条 WARN 级别日志。

<a id="Std.Diagnostics.Log.Error(string)"></a>
### static void Error(string msg)

[`src/libraries/z42.diagnostics/src/Log.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L59)

输出一条 ERROR 级别日志。

<a id="Std.Diagnostics.Log.Trace(string,LogFields)"></a>
### static void Trace(string msg, LogFields fields)

[`src/libraries/z42.diagnostics/src/Log.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L62)

输出一条 TRACE 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

<a id="Std.Diagnostics.Log.Debug(string,LogFields)"></a>
### static void Debug(string msg, LogFields fields)

[`src/libraries/z42.diagnostics/src/Log.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L64)

输出一条 DEBUG 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

<a id="Std.Diagnostics.Log.Info(string,LogFields)"></a>
### static void Info(string msg, LogFields fields)

[`src/libraries/z42.diagnostics/src/Log.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L66)

输出一条 INFO 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

<a id="Std.Diagnostics.Log.Warn(string,LogFields)"></a>
### static void Warn(string msg, LogFields fields)

[`src/libraries/z42.diagnostics/src/Log.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L68)

输出一条 WARN 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

<a id="Std.Diagnostics.Log.Error(string,LogFields)"></a>
### static void Error(string msg, LogFields fields)

[`src/libraries/z42.diagnostics/src/Log.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L70)

输出一条 ERROR 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

<a id="Std.Diagnostics.Log.FormatHeader(int,DateTime)"></a>
### static string FormatHeader(int level, DateTime now)

[`src/libraries/z42.diagnostics/src/Log.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Log.z42#L87)

单独生成 `[LEVEL 时间戳]` 头，供调用方 / 测试探查格式而不必绕 stderr。

## LogFields

[`src/libraries/z42.diagnostics/src/LogFields.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogFields.z42#L4)

logfmt 风格的键值附件，链式 `Add`。

```z42
public sealed class LogFields {
    LogFields()
    LogFields Add(string key, string value)
    int Count()
    string Format()
}
```

成员：[`LogFields`](#Std.Diagnostics.LogFields.ctor()) · [`Add`](#Std.Diagnostics.LogFields.Add(string,string)) · [`Count`](#Std.Diagnostics.LogFields.Count()) · [`Format`](#Std.Diagnostics.LogFields.Format())

<a id="Std.Diagnostics.LogFields.ctor()"></a>
### LogFields()

[`src/libraries/z42.diagnostics/src/LogFields.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogFields.z42#L11)

构造一个空的字段集。

<a id="Std.Diagnostics.LogFields.Add(string,string)"></a>
### LogFields Add(string key, string value)

[`src/libraries/z42.diagnostics/src/LogFields.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogFields.z42#L19)

追加一个键值对，返回 `this` 以便链式调用。

<a id="Std.Diagnostics.LogFields.Count()"></a>
### int Count()

[`src/libraries/z42.diagnostics/src/LogFields.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogFields.z42#L28)

返回已添加的字段数量。

<a id="Std.Diagnostics.LogFields.Format()"></a>
### string Format()

[`src/libraries/z42.diagnostics/src/LogFields.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogFields.z42#L31)

格式化为 ` key1="v1" key2="v2"`（非空时带前导空格），无字段时返回空字符串。

## LogLevel

[`src/libraries/z42.diagnostics/src/LogLevel.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L4)

日志级别：普通 int 常量（非 enum），越大越严重。

```z42
public static class LogLevel {
    static int Trace
    static int Debug
    static int Info
    static int Warn
    static int Error
    static string Name(int level)
}
```

成员：[`Trace`](#Std.Diagnostics.LogLevel.Trace) · [`Debug`](#Std.Diagnostics.LogLevel.Debug) · [`Info`](#Std.Diagnostics.LogLevel.Info) · [`Warn`](#Std.Diagnostics.LogLevel.Warn) · [`Error`](#Std.Diagnostics.LogLevel.Error) · [`Name`](#Std.Diagnostics.LogLevel.Name(int))

<a id="Std.Diagnostics.LogLevel.Trace"></a>
### static int Trace

[`src/libraries/z42.diagnostics/src/LogLevel.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L6)

TRACE 级别常量（0，最低）。

<a id="Std.Diagnostics.LogLevel.Debug"></a>
### static int Debug

[`src/libraries/z42.diagnostics/src/LogLevel.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L8)

DEBUG 级别常量（1）。

<a id="Std.Diagnostics.LogLevel.Info"></a>
### static int Info

[`src/libraries/z42.diagnostics/src/LogLevel.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L10)

INFO 级别常量（2，默认最低输出级别）。

<a id="Std.Diagnostics.LogLevel.Warn"></a>
### static int Warn

[`src/libraries/z42.diagnostics/src/LogLevel.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L12)

WARN 级别常量（3）。

<a id="Std.Diagnostics.LogLevel.Error"></a>
### static int Error

[`src/libraries/z42.diagnostics/src/LogLevel.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L14)

ERROR 级别常量（4，最高）。

<a id="Std.Diagnostics.LogLevel.Name(int)"></a>
### static string Name(int level)

[`src/libraries/z42.diagnostics/src/LogLevel.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/LogLevel.z42#L19)

返回 level 的大写名字。未知 level 返回 "LEVEL<n>"。

返回值保留 SCREAMING_SNAKE 作业界 log 格式约定；与字段大小写无关。

## Retainer

[`src/libraries/z42.diagnostics/src/Retainer.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Retainer.z42#L4)

直接引用 target 的堆对象（`Heap.DirectReferrers` 的元素，字段由 VM 写入）。

```z42
public sealed class Retainer {
    string TypeName
    long Id
}
```

成员：[`TypeName`](#Std.Diagnostics.Retainer.TypeName) · [`Id`](#Std.Diagnostics.Retainer.Id)

<a id="Std.Diagnostics.Retainer.TypeName"></a>
### string TypeName

[`src/libraries/z42.diagnostics/src/Retainer.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Retainer.z42#L6)

引用者类型名（数组以 "[]" 结尾，如 "int[]"）。

<a id="Std.Diagnostics.Retainer.Id"></a>
### long Id

[`src/libraries/z42.diagnostics/src/Retainer.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/Retainer.z42#L8)

引用者的堆身份（稳定 id，用于区分同类型的不同实例）。

## RootKind

[`src/libraries/z42.diagnostics/src/RootKind.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RootKind.z42#L4)

GC 根类别。

```z42
public enum RootKind {
    StaticField
    StackFrame
    Pinned
}
```

## RootRef

[`src/libraries/z42.diagnostics/src/RootRef.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RootRef.z42#L4)

保留 target 的 GC 根（类别级；`Heap.RetainingRoots` 的元素）。

```z42
public sealed class RootRef {
    RootKind Kind
}
```

成员：[`Kind`](#Std.Diagnostics.RootRef.Kind)

<a id="Std.Diagnostics.RootRef.Kind"></a>
### RootKind Kind

[`src/libraries/z42.diagnostics/src/RootRef.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RootRef.z42#L6)

根类别（由 VM 写入）。

## RuntimeCounters

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L4)

VM 运行时累计计数快照（由 VM 直接填充，用户不能 new）。

```z42
public class RuntimeCounters {
    long BuiltinCalls { get; }
    long NativeCalls { get; }
    long JitMethodsCompiled { get; }
    long JitCompileUsTotal { get; }
    long JitNativeFromInterp { get; }
    long ExceptionsThrown { get; }
    long ExceptionsCaught { get; }
    long Allocations { get; }
    long MinorCollections { get; }
    long MajorCollections { get; }
    long ReclaimedBytes { get; }
}
```

成员：[`BuiltinCalls`](#Std.Diagnostics.RuntimeCounters.BuiltinCalls) · [`NativeCalls`](#Std.Diagnostics.RuntimeCounters.NativeCalls) · [`JitMethodsCompiled`](#Std.Diagnostics.RuntimeCounters.JitMethodsCompiled) · [`JitCompileUsTotal`](#Std.Diagnostics.RuntimeCounters.JitCompileUsTotal) · [`JitNativeFromInterp`](#Std.Diagnostics.RuntimeCounters.JitNativeFromInterp) · [`ExceptionsThrown`](#Std.Diagnostics.RuntimeCounters.ExceptionsThrown) · [`ExceptionsCaught`](#Std.Diagnostics.RuntimeCounters.ExceptionsCaught) · [`Allocations`](#Std.Diagnostics.RuntimeCounters.Allocations) · [`MinorCollections`](#Std.Diagnostics.RuntimeCounters.MinorCollections) · [`MajorCollections`](#Std.Diagnostics.RuntimeCounters.MajorCollections) · [`ReclaimedBytes`](#Std.Diagnostics.RuntimeCounters.ReclaimedBytes)

<a id="Std.Diagnostics.RuntimeCounters.BuiltinCalls"></a>
### long BuiltinCalls { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L8)

调用的 builtin 函数次数（`__str_length` / `__print` / …）。

<a id="Std.Diagnostics.RuntimeCounters.NativeCalls"></a>
### long NativeCalls { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L11)

派发的 native FFI 调用次数（用户 `[Native]` extern 方法）。

<a id="Std.Diagnostics.RuntimeCounters.JitMethodsCompiled"></a>
### long JitMethodsCompiled { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L14)

JIT 编译的方法数（tier-up 编译触发）。

<a id="Std.Diagnostics.RuntimeCounters.JitCompileUsTotal"></a>
### long JitCompileUsTotal { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L17)

累计 JIT 编译墙钟时间（微秒）；per-compile span 计时未接线时为 0。

<a id="Std.Diagnostics.RuntimeCounters.JitNativeFromInterp"></a>
### long JitNativeFromInterp { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L20)

mixed-mode：interp 帧把已编译 callee 路由到其 native code 的次数（>0 = mixed-mode 生效）。

<a id="Std.Diagnostics.RuntimeCounters.ExceptionsThrown"></a>
### long ExceptionsThrown { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L23)

抛出的用户异常数（`throw` + VM 抬升的算术/类型错误）。

<a id="Std.Diagnostics.RuntimeCounters.ExceptionsCaught"></a>
### long ExceptionsCaught { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L26)

被 `try/catch` 捕获的异常数。

<a id="Std.Diagnostics.RuntimeCounters.Allocations"></a>
### long Allocations { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L31)

自堆创建以来累计的对象 alloc 次数（HeapStats.allocations SoT）。

<a id="Std.Diagnostics.RuntimeCounters.MinorCollections"></a>
### long MinorCollections { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L34)

generational minor（young-gen）回收次数。

<a id="Std.Diagnostics.RuntimeCounters.MajorCollections"></a>
### long MajorCollections { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L37)

major（whole-heap）回收次数。

<a id="Std.Diagnostics.RuntimeCounters.ReclaimedBytes"></a>
### long ReclaimedBytes { get; }

[`src/libraries/z42.diagnostics/src/RuntimeCounters.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeCounters.z42#L40)

累计回收字节数。

## RuntimeStats

[`src/libraries/z42.diagnostics/src/RuntimeStats.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeStats.z42#L4)

读取 VM 自身累计运行计数（alloc / GC / 异常 / JIT）的第一方入口。

```z42
public static class RuntimeStats {
    static extern RuntimeCounters Counters()
}
```

成员：[`Counters`](#Std.Diagnostics.RuntimeStats.Counters())

<a id="Std.Diagnostics.RuntimeStats.Counters()"></a>
### static extern RuntimeCounters Counters()

[`src/libraries/z42.diagnostics/src/RuntimeStats.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.diagnostics/src/RuntimeStats.z42#L7)

读取自 VM 启动以来的累计计数快照（各计数独立读取，彼此有微小 skew）。

