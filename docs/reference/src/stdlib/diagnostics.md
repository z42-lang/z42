# z42.diagnostics —— 日志与运行时自省

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
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

## LogLevel

级别是**普通 int 常量**，不是 enum；越大越严重，过滤判据是 `level >= minLevel`。

```z42
public static class LogLevel {
    public static int Trace = 0;
    public static int Debug = 1;
    public static int Info  = 2;
    public static int Warn  = 3;
    public static int Error = 4;

    public static string Name(int level)
}
```

`Name` 返回大写名（`"TRACE"` / `"DEBUG"` / `"INFO"` / `"WARN"` / `"ERROR"`）；未知级别
返回 `"LEVEL" + n`，例如 `Name(9)` → `"LEVEL9"`。

字段是 `static` 变量而非常量，访问必须带类名：`LogLevel.Debug`。

## Log

全局单例门面，无 named logger、无 sink 配置。

```z42
public static class Log {
    public static void SetMinLevel(int level)
    public static int  GetMinLevel()
    public static bool IsEnabled(int level)

    public static void Trace(string msg)
    public static void Debug(string msg)
    public static void Info(string msg)
    public static void Warn(string msg)
    public static void Error(string msg)

    public static void Trace(string msg, LogFields fields)
    public static void Debug(string msg, LogFields fields)
    public static void Info(string msg, LogFields fields)
    public static void Warn(string msg, LogFields fields)
    public static void Error(string msg, LogFields fields)

    public static string FormatHeader(int level, DateTime now)
}
```

| 成员 | 说明 |
|---|---|
| `SetMinLevel` / `GetMinLevel` | 全局最低输出级别，**默认 `LogLevel.Info`**（`2`）。低于它的调用被整条丢弃 |
| `IsEnabled` | 该级别当前是否会输出。构造昂贵的 message 前先问一句 |
| 五个级别方法 | 各自对应一个 `LogLevel`；`fields` 传 `null` 等同不带字段 |
| `FormatHeader` | 单独生成 `[LEVEL 时间戳]` 头，供调用方 / 测试探查格式而不必绕 stderr |

### 输出

每条日志一行，写入 `Std.IO.ConsoleError`（**stderr**），stdout 留给程序主输出：

```
[INFO 2026-09-17T09:09:04.558Z] hello info
[INFO 2026-09-17T09:09:04.562Z] with fields port="8080" quote="a\"b\\c"
```

时间戳是 `DateTime.UtcNow().ToIso8601()` 的 UTC ISO-8601（毫秒 + `Z`）。

级别名在 `Std.IO.Ansi.Enabled()` 为真时套 ANSI 颜色：TRACE→Dim、DEBUG→Cyan、
INFO→Green、WARN→Yellow、ERROR→Bold+Red。开关由 `Ansi` 决定：自动探测
`Console.IsTerminal()` 且环境变量 `NO_COLOR` 未设置，或由 `Ansi.SetEnabled(bool)` 显式
覆盖。注意自动探测看的是 **stdout** 是否 TTY，而日志写 stderr —— stdout 被重定向而
stderr 仍是终端时不会上色。

## LogFields

logfmt 风格的键值附件，链式 `Add`。

```z42
public sealed class LogFields {
    public LogFields()
    public LogFields Add(string key, string value)
    public int       Count()
    public string    Format()
}
```

`Format()` 返回 ` key1="v1" key2="v2"`（非空时**带前导空格**），无字段时返回 `""`。
值一律双引号包裹，值里的 `"` 和 `\` 反斜杠转义。key 原样输出 —— 别在 key 里放 `=`
或空格。

## RuntimeStats / RuntimeCounters

读 VM 自身的累计计数，用于脚本内 benchmark、自适应逻辑、给分配量加断言上界。

```z42
public static class RuntimeStats {
    public static extern RuntimeCounters Counters();
}
```

每次调用读当前累计值（自 VM 启动以来）。快照不是一致元组——各计数独立读取，彼此有微
小 skew。`RuntimeCounters` 由 VM 直接填充，用户不能 `new`：

```z42
public class RuntimeCounters {
    public long BuiltinCalls        { get; }
    public long NativeCalls         { get; }
    public long JitMethodsCompiled  { get; }
    public long JitCompileUsTotal   { get; }
    public long JitNativeFromInterp { get; }
    public long ExceptionsThrown    { get; }
    public long ExceptionsCaught    { get; }
    public long Allocations         { get; }
    public long MinorCollections    { get; }
    public long MajorCollections    { get; }
    public long ReclaimedBytes      { get; }
}
```

| 字段 | 含义 |
|---|---|
| `BuiltinCalls` | 调用的 builtin 函数次数 |
| `NativeCalls` | 派发的 native FFI 调用次数（用户 `[Native]` extern 方法） |
| `JitMethodsCompiled` | JIT 编译过的方法数 |
| `JitCompileUsTotal` | 累计 JIT 编译墙钟时间（微秒） |
| `JitNativeFromInterp` | 解释器帧路由到已编译 native code 的次数（`> 0` 即 mixed-mode 生效） |
| `ExceptionsThrown` / `ExceptionsCaught` | 抛出 / 被 `try-catch` 捕获的异常数 |
| `Allocations` | 自堆创建以来累计的对象分配次数 |
| `MinorCollections` / `MajorCollections` | minor（young-gen）/ major（全堆）回收次数 |
| `ReclaimedBytes` | 累计回收字节数 |

`Std.HeapStats`（`GC.GetStats()` 返回）是另一个更精简的类型；这里是含 JIT / 异常 /
分代信息的全景入口。

## Heap 保留诊断

回答「某对象为什么没被回收」。两层查询，每次查询**先触发一次 full GC**，所以遍历到的
都是真活对象，不会把浮动垃圾当成保留者。诊断用途，非热路径。

```z42
public static class Heap {
    public static extern Retainer[] DirectReferrers(object target);
    public static extern RootRef[]  RetainingRoots(object target);
}

public sealed class Retainer {
    public string TypeName;   // 引用者类型名（数组以 "[]" 结尾，如 "int[]"）
    public long   Id;         // 引用者的堆身份（稳定 id，区分同类型不同实例）
    override string ToString();   // "Holder#5794552168528"
}

public sealed class RootRef {
    public RootKind Kind;
    override string ToString();   // "root:StaticField"
}

public enum RootKind {
    StaticField,   // 某 static 字段（直接或经引用链）
    StackFrame,    // 某线程调用栈帧的局部 / 求值栈 / 栈上分配 arena
    FuncRefSlot,   // 方法组转换缓存槽
    Pinned,        // 宿主 pin / 帧 pin
}
```

| 方法 | 返回 |
|---|---|
| `DirectReferrers` | 直接持有 `target` 引用的堆对象（object / array）列表 |
| `RetainingRoots` | 从 `target` 反向可达的 GC 根，**类别级**（去重），不给具体根名 |

`target` 不是堆对象（传了 `int` 等值类型）时两者都返回空数组。
`Retainer` / `RootRef` 的字段由 VM 写入，用户不构造。

## 用法

```z42
using Std.IO;
using Std.Diagnostics;

void HandleRequest(int id) {
    Log.Info("request started", new LogFields()
        .Add("id", id.ToString())
        .Add("env", "prod"));
    try {
        // ... work ...
    } catch (Exception e) {
        Log.Error("request failed: " + e.Message);
    }
}

void Main() {
    Log.SetMinLevel(LogLevel.Debug);

    // 昂贵 message 先探一句
    if (Log.IsEnabled(LogLevel.Debug)) {
        Log.Debug("state dump: " + Dump(state));
    }

    HandleRequest(7);

    RuntimeCounters c = RuntimeStats.Counters();
    Console.WriteLine("allocs=" + c.Allocations.ToString()
        + " minor=" + c.MinorCollections.ToString());
}
```

查一个对象被谁钉住：

```z42
int[] buf = new int[4];
Cache.Root = new Holder(buf);          // 某处把它挂上了 static 字段

Retainer[] refs = Heap.DirectReferrers(buf);
// → 1 条：Holder#5794552168528

RootRef[] roots = Heap.RetainingRoots(buf);
// → 2 条：root:StaticField、root:StackFrame
```

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
