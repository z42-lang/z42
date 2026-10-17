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
[time](time.md)）。

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

堆保留诊断：回答「某对象为什么没被回收、被谁钉住」。

```z42
public static class Heap {
    static extern Retainer[] DirectReferrers(object target)
    static extern RootRef[] RetainingRoots(object target)
}
```

### static extern Retainer[] DirectReferrers(object target)

直接持有 `target` 引用的堆对象（object / array）列表；非堆对象返回空。

### static extern RootRef[] RetainingRoots(object target)

从 `target` 反向可达的 GC 根（类别级，去重）；非堆对象返回空。

## Log

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

### static void SetMinLevel(int level)

设置全局最低输出级别，低于该级别的调用被整条丢弃。

### static int GetMinLevel()

返回全局最低输出级别。

### static bool IsEnabled(int level)

该级别当前是否会输出；构造昂贵的 message 前先问一句。

### static void Trace(string msg)

输出一条 TRACE 级别日志。

### static void Debug(string msg)

输出一条 DEBUG 级别日志。

### static void Info(string msg)

输出一条 INFO 级别日志。

### static void Warn(string msg)

输出一条 WARN 级别日志。

### static void Error(string msg)

输出一条 ERROR 级别日志。

### static void Trace(string msg, LogFields fields)

输出一条 TRACE 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

### static void Debug(string msg, LogFields fields)

输出一条 DEBUG 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

### static void Info(string msg, LogFields fields)

输出一条 INFO 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

### static void Warn(string msg, LogFields fields)

输出一条 WARN 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

### static void Error(string msg, LogFields fields)

输出一条 ERROR 级别日志，可附加结构化字段（`fields` 为 null 等同不带字段）。

### static string FormatHeader(int level, DateTime now)

单独生成 `[LEVEL 时间戳]` 头，供调用方 / 测试探查格式而不必绕 stderr。

## LogFields

logfmt 风格的键值附件，链式 `Add`。

```z42
public sealed class LogFields {
    LogFields()
    LogFields Add(string key, string value)
    int Count()
    string Format()
}
```

### LogFields()

构造一个空的字段集。

### LogFields Add(string key, string value)

追加一个键值对，返回 `this` 以便链式调用。

### int Count()

返回已添加的字段数量。

### string Format()

格式化为 ` key1="v1" key2="v2"`（非空时带前导空格），无字段时返回空字符串。

## LogLevel

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

### static int Trace

TRACE 级别常量（0，最低）。

### static int Debug

DEBUG 级别常量（1）。

### static int Info

INFO 级别常量（2，默认最低输出级别）。

### static int Warn

WARN 级别常量（3）。

### static int Error

ERROR 级别常量（4，最高）。

### static string Name(int level)

返回 level 的大写名字。未知 level 返回 "LEVEL<n>"。 返回值保留 SCREAMING_SNAKE 作业界 log 格式约定；与字段大小写无关。

## Retainer

直接引用 target 的堆对象（`Heap.DirectReferrers` 的元素，字段由 VM 写入）。

```z42
public sealed class Retainer {
    string TypeName
    long Id
}
```

### string TypeName

引用者类型名（数组以 "[]" 结尾，如 "int[]"）。

### long Id

引用者的堆身份（稳定 id，用于区分同类型的不同实例）。

## RootKind

GC 根类别。

```z42
public enum RootKind {
    StaticField
    StackFrame
    Pinned
}
```

## RootRef

保留 target 的 GC 根（类别级；`Heap.RetainingRoots` 的元素）。

```z42
public sealed class RootRef {
    RootKind Kind
}
```

### RootKind Kind

根类别（由 VM 写入）。

## RuntimeCounters

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

### long BuiltinCalls { get; }

调用的 builtin 函数次数（`__str_length` / `__print` / …）。

### long NativeCalls { get; }

派发的 native FFI 调用次数（用户 `[Native]` extern 方法）。

### long JitMethodsCompiled { get; }

JIT 编译的方法数（tier-up 编译触发）。

### long JitCompileUsTotal { get; }

累计 JIT 编译墙钟时间（微秒）；per-compile span 计时未接线时为 0。

### long JitNativeFromInterp { get; }

mixed-mode：interp 帧把已编译 callee 路由到其 native code 的次数（>0 = mixed-mode 生效）。

### long ExceptionsThrown { get; }

抛出的用户异常数（`throw` + VM 抬升的算术/类型错误）。

### long ExceptionsCaught { get; }

被 `try/catch` 捕获的异常数。

### long Allocations { get; }

自堆创建以来累计的对象 alloc 次数（HeapStats.allocations SoT）。

### long MinorCollections { get; }

generational minor（young-gen）回收次数。

### long MajorCollections { get; }

major（whole-heap）回收次数。

### long ReclaimedBytes { get; }

累计回收字节数。

## RuntimeStats

读取 VM 自身累计运行计数（alloc / GC / 异常 / JIT）的第一方入口。

```z42
public static class RuntimeStats {
    static extern RuntimeCounters Counters()
}
```

### static extern RuntimeCounters Counters()

读取自 VM 启动以来的累计计数快照（各计数独立读取，彼此有微小 skew）。

