# z42.io —— 子进程与终端着色

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.io/`；命名空间 `Std.IO`（异常类型在 `Std`）

`Process` 是 builder 形态的子进程执行入口：argv 数组进出、**不经过 shell**、
stdio 三路各自可配。同步跑完拿结果用 `Run()`，要一边跑一边喂 / 读用 `Spawn()`。
同包里的 `Ansi` 负责终端 ANSI 着色，`Console.IsTerminal()` 与 `NO_COLOR` 决定默认开关。

本页的类型都在 **`z42.io`**，不是隐式加载的 `z42.core`——**单文件 `z42 run x.z42` 里
解析不到**（会抛 `Std.MissingSymbolException: type Std.IO.Process could not be resolved`）。
要用就建工程，并在 manifest 里声明依赖，见[工程清单 z42.toml](../toolchain/z42-toml.md)。

同命名空间下 `Console` / `File` / `Path` / `Environment` 在 `z42.core`，见
[控制台与文件](io-file.md)；`Stream` 家族与子进程管道流的 `Stream` 侧语义见[流](io-stream.md)。

## Process

```z42
public class Process {
    public Process(string program);

    // argv（program 本身不算 arg）
    public Process Arg(string a);
    public Process Args(string[] xs);

    // 执行环境
    public Process WorkingDirectory(string d);
    public Process Env(string k, string v);
    public Process EnvRemove(string k);
    public Process ClearEnv();

    // stdio
    public Process Stdin(Stdio s);
    public Process Stdout(Stdio s);
    public Process Stderr(Stdio s);
    public Process StdinBytes(byte[] b);

    // 其它旋钮
    public Process Timeout(long milliseconds);
    public Process ShareProcessGroup();

    // 执行
    public ProcessResult Run();
    public ProcessHandle Spawn();

    public static string? Which(string name);
}
```

所有配置方法都返回 `this`，可以链式写。

| 成员 | 说明 |
|---|---|
| `Process(program)` | `program` 是可执行文件名或路径；名字不含分隔符时按 `$PATH` 查找 |
| `Arg(a)` / `Args(xs)` | 逐个追加 argv。**不经过 shell**：`*`、空格、`;` 都是字面量，不会被展开或再切分 |
| `WorkingDirectory(d)` | 子进程的 cwd；不设则继承调用方 |
| `Env(k, v)` | 在继承的环境之上覆盖 / 新增一项 |
| `EnvRemove(k)` | 从继承的环境里删掉一项 |
| `ClearEnv()` | 完全不继承调用方环境（子进程仍可能拿到 OS / shell 提供的兜底 `PATH`） |
| `Stdin(s)` / `Stdout(s)` / `Stderr(s)` | 见 [Stdio](#stdio)。**默认 stdin = `Null`，stdout / stderr = `Pipe`** |
| `StdinBytes(b)` | 一次性喂给子进程的 stdin 负载，写完即关（子进程读到 EOF）。⚠ 必须配 `Stdin(Stdio.Pipe())`，见下 |
| `Timeout(ms)` | 毫秒；`-1` 表示不限时（默认）。超时抛 `ProcessTimeoutException`。**只对 `Run()` 生效** |
| `ShareProcessGroup()` | 让子进程留在调用方的进程组里。交互式 tty 透传（`Stdin(Stdio.Inherit())` 且子进程要抢终端）时才需要；代价是超时的树杀不再能连坐孙进程 |
| `Run()` | 同步跑到结束，返回 [`ProcessResult`](#processresult) |
| `Spawn()` | 立刻返回 [`ProcessHandle`](#processhandle)，子进程在后台跑 |
| `Which(name)` | 按 `$PATH`（Windows 上还按 `PATHEXT`）定位可执行文件，返回解析到的路径，找不到返回 `null`。名字里带路径分隔符时跳过 `$PATH` 直接判存在性 |

⚠ **`StdinBytes` 单独用是静默无效的**。默认 stdin 是 `Stdio.Null()`，这时负载会被**悄悄丢弃**，
没有报错也没有警告：

```z42
new Process("cat").StdinBytes(bytes).Run();                       // stdout 是空的
new Process("cat").Stdin(Stdio.Pipe()).StdinBytes(bytes).Run();   // ✓ 这样才喂进去
```

`Run()` 与 `Spawn()` 的差别不止同步 / 异步：

| | `Run()` | `Spawn()` |
|---|---|---|
| 返回 | `ProcessResult`（已含全部 stdout / stderr） | `ProcessHandle` |
| `Timeout(...)` | 生效 | **不生效** |
| `StdinBytes(...)` | 生效（配 `Stdio.Pipe()`） | **不生效**——改用 `WriteStdin` / `GetStdinStream()` 流式写 |
| `ShareProcessGroup()` | 生效 | 不生效 |
| 输出背压 | 输出被全程收走，子进程不会因管道写满而卡死 | 由你自己读；不读就可能把子进程堵在管道上 |

## ProcessResult

`Run()` 与 `ProcessHandle.Wait()` / `TryWait()` 的返回值。**字段是公开可写的普通字段**，不是属性。

```z42
public class ProcessResult {
    public string Program;
    public int    ExitCode;
    public string Stdout;
    public string Stderr;
    public byte[] StdoutBytes;
    public byte[] StderrBytes;

    public ProcessResult(string program, int exitCode, string stdout, string stderr,
                         byte[] stdoutBytes, byte[] stderrBytes);
    public ProcessResult EnsureSuccess();
}
```

| 成员 | 说明 |
|---|---|
| `ExitCode` | 正常退出是退出码；被信号杀死时是 **`128 + 信号号`**（`SIGTERM` → `143`，`SIGKILL` → `137`） |
| `Stdout` / `Stderr` | UTF-8 **lossy** 解码——非法字节变成替换字符，不抛异常 |
| `StdoutBytes` / `StderrBytes` | 原始字节，二进制协议 / 非 UTF-8 工具输出用这个 |
| `EnsureSuccess()` | `ExitCode != 0` 时抛 `ProcessExitException`，否则返回 `this`（可以直接 `.Run().EnsureSuccess()` 串起来） |

`Run()` 本身**永远不会**因为退出码非零而抛错——`git diff --exit-code` 的 `1` 是有用信息不是
错误。要 fail-fast 就显式调 `EnsureSuccess()`。

对应路没有走 `Pipe` 的那一路（`Null` / `Inherit` / `ToFile`），`Stdout` / `Stderr` 是空串，
`StdoutBytes` / `StderrBytes` 长度为 0。

## Stdio

三路标准流各自的配置。是四个模式常量 + 可选路径，不是 `enum`。

```z42
public sealed class Stdio {
    public static int ModeNull    = 0;
    public static int ModeInherit = 1;
    public static int ModePipe    = 2;
    public static int ModeFile    = 3;

    public Stdio(int mode, string path);

    public static Stdio Inherit();
    public static Stdio Pipe();
    public static Stdio Null();
    public static Stdio ToFile(string path);

    public int    GetMode();
    public string GetPath();      // 非 File 模式返回 ""
}
```

| 工厂 | 语义 |
|---|---|
| `Stdio.Null()` | `/dev/null`：stdin 立刻 EOF，输出丢弃 |
| `Stdio.Inherit()` | 直接用调用方的那个 fd（子进程输出直接落到你的终端，不被捕获） |
| `Stdio.Pipe()` | 建管道。stdout / stderr 上意味着「捕获到 `ProcessResult`」；stdin 上意味着「可以喂数据」 |
| `Stdio.ToFile(path)` | 重定向到文件 |

默认值：**stdin = `Null()`，stdout = `Pipe()`，stderr = `Pipe()`**。

⚠ **`Stdio.ToFile(path)` 目前只能用在 stdout / stderr 上**。用在 stdin 上会抛
`Std.Exception`（`stdin Stdio.ToFile missing path`）——路径没有被传到下层。要从文件喂
stdin，先 `File.ReadAllBytes` 再 `Stdin(Stdio.Pipe()).StdinBytes(...)`。

## ProcessHandle

`Spawn()` 返回的活子进程句柄。实现 `Std.IDisposable`。

```z42
public class ProcessHandle : IDisposable {
    public ProcessHandle(string program, long slotId);

    public long           Pid();
    public ProcessResult  Wait();
    public ProcessResult? TryWait();       // 还在跑返回 null
    public void           Kill();
    public void           KillForce();

    public void WriteStdin(byte[] bytes);
    public void WriteStdinString(string s);   // 等价 WriteStdin(Utf8.GetBytes(s))
    public void CloseStdin();

    public int ReadStdout(byte[] buffer, int offset, int count);   // 返回 0 = EOF
    public int ReadStderr(byte[] buffer, int offset, int count);

    public ProcessStdinStream  GetStdinStream();
    public ProcessOutputStream GetStdoutStream();
    public ProcessOutputStream GetStderrStream();

    public void Dispose();
}
```

| 成员 | 说明 |
|---|---|
| `Pid()` | OS 进程号 |
| `Wait()` | 阻塞到子进程结束，返回 `ProcessResult`。**调用之后句柄即失效** |
| `TryWait()` | 还在跑返回 `null`；已结束返回 `ProcessResult` 并**使句柄失效** |
| `Kill()` / `KillForce()` | 两者当前**行为相同**（都是不可捕获的强制终止，Unix 上 `SIGKILL`）。被杀的子进程 `ExitCode` 是 `137` |
| `WriteStdin` / `WriteStdinString` | 写子进程 stdin；管道写满时阻塞 |
| `CloseStdin()` | 关闭 stdin，子进程读到 EOF。幂等 |
| `ReadStdout` / `ReadStderr` | 阻塞读，返回实际读到的字节数，**`0` 表示 EOF** |
| `GetXxxStream()` | 返回 `Std.IO.Stream` 包装，**惰性构造并缓存**：重复调用返回同一个对象 |
| `Dispose()` | 幂等。子进程仍活着时会被 kill + reap（不留僵尸） |

**句柄是一次性的**。`Wait()` / `TryWait()`（返回了结果时）/ `Dispose()` 之后再调任何方法，
抛 `ProcessHandleInvalidException`。

⚠ **对没有配 `Stdin(Stdio.Pipe())` 的句柄写 stdin 会连带作废整个句柄**：`WriteStdin` 抛
`ProcessHandleInvalidException`，并且**本地句柄同时被标记为已失效**，之后连 `Wait()` 都
抛同一个异常，尽管子进程其实还活着（进程本身仍由运行时负责回收）。要喂 stdin，spawn 时就
写上 `.Stdin(Stdio.Pipe())`。

三个 Stream 包装（`ProcessStdinStream` / `ProcessOutputStream`）的 `Stream` 侧行为——
能力谓词、`Close()` 的边界、越界参数——见[流](io-stream.md#子进程管道流)。

## 异常

四个异常都在 `Std` 命名空间，都继承 `Std.Exception`。

```z42
public class ProcessStartException : Exception {
    public ProcessStartException(string message);
}

public class ProcessExitException : Exception {
    public int    ExitCode;
    public string Program;
}

public class ProcessTimeoutException : Exception {
    public string Program;
    public long   TimeoutMs;
}

public class ProcessHandleInvalidException : Exception {
    public string Program;
}
```

| 类型 | 何时抛 | `Message` 形状 |
|---|---|---|
| `ProcessStartException` | 子进程**根本没启动**（可执行文件找不到、权限不足、fork/exec 失败）。`Run()` 与 `Spawn()` 都会抛 | `foo: No such file or directory (os error 2) (kind: NotFound)` |
| `ProcessExitException` | **只有** `ProcessResult.EnsureSuccess()` 在退出码非零时抛 | ``process `sh` exited with code 7`` ，非空 stderr 时再跟一行 `stderr: <前 1024 字节，超出加 “…<truncated>”>` |
| `ProcessTimeoutException` | `Run()` 超过 `Timeout(ms)`。子进程已被杀并回收，调用方不需要再清理 | ``process `sh` did not finish within 300ms`` |
| `ProcessHandleInvalidException` | 对已 `Wait` / `TryWait`（有结果）/ `Dispose` 的句柄再操作；或对没有 pipe stdin 的句柄写 stdin | ``process handle for `cat` is no longer valid (already waited, killed, or disposed)`` |

「启动失败」与「退出码非零」是两件事：前者是 `ProcessStartException` 且**总是**抛出，
后者是 `ProcessExitException` 且**只在你要求时**抛出。

## Ansi

终端 ANSI SGR 着色。启用时把文本包成 `ESC[<code>m … ESC[0m`，**未启用时原样返回输入**，
所以调用点不需要写条件分支。

```z42
public static class Ansi {
    public static bool Enabled();
    public static void SetEnabled(bool b);

    // 前景色（30–37）
    public static string Black(string s);   public static string Red(string s);
    public static string Green(string s);   public static string Yellow(string s);
    public static string Blue(string s);    public static string Magenta(string s);
    public static string Cyan(string s);    public static string White(string s);

    // 亮色前景（90–97）
    public static string BrightBlack(string s);   public static string BrightRed(string s);
    public static string BrightGreen(string s);   public static string BrightYellow(string s);
    public static string BrightBlue(string s);    public static string BrightMagenta(string s);
    public static string BrightCyan(string s);    public static string BrightWhite(string s);

    // 样式
    public static string Bold(string s);      public static string Dim(string s);
    public static string Italic(string s);    public static string Underline(string s);
    public static string Reverse(string s);

    public static string Strip(string s);
}
```

| 成员 | 说明 |
|---|---|
| `Enabled()` | 首次调用做自动检测：`Console.IsTerminal()` 为真 **且** 环境变量 `NO_COLOR` 未设置或为空。之后返回缓存值 |
| `SetEnabled(b)` | 显式覆盖。一旦调用过，自动检测对本进程**永久失效**（无论传 `true` 还是 `false`） |
| 各着色函数 | `Enabled()` 为假时**原样返回 `s`**，不加任何字节 |
| `Strip(s)` | 移除所有 CSI 转义序列（`ESC [` 直到 `0x40..0x7E` 范围内的终结字节）——SGR、光标移动、清屏都会被清掉。用于算显示宽度或写无色日志 |

- **嵌套是安全的**：`Ansi.Bold(Ansi.Yellow("W"))` 正常工作，终端把重复的 `ESC[0m` 当空操作。
- `Strip` 遇到**没有终结字节的残缺序列**时，会把从 `ESC[` 起的剩余内容全部丢弃。
- 只有 8 色 + 8 亮色 + 5 种样式，没有 256 色 / truecolor，也没有背景色。
- 光标定位 / 清屏之类没有**生成**接口（`Strip` 能清掉它们，但不提供构造它们的方法）。

## 用法

```z42
using Std;
using Std.IO;
using Std.Encoding;

void Main() {
    // 1. 探测命令是否存在
    if (Process.Which("git") == null) {
        ConsoleError.WriteLine(Ansi.Red("git not found"));
        Environment.Exit(1);
    }

    // 2. 同步跑一条命令，失败即抛
    ProcessResult r = new Process("git")
        .Arg("rev-parse").Arg("--short").Arg("HEAD")
        .WorkingDirectory(".")
        .Timeout(10000)
        .Run()
        .EnsureSuccess();
    Console.WriteLine(Ansi.Green($"HEAD = {r.Stdout.Trim()}"));

    // 3. 退出码是信息不是错误——不调 EnsureSuccess
    ProcessResult d = new Process("git").Arg("diff").Arg("--exit-code").Run();
    Console.WriteLine(d.ExitCode == 0 ? "clean" : "dirty");

    // 4. 一次性喂 stdin（记得 Stdio.Pipe）
    ProcessResult h = new Process("shasum")
        .Arg("-a").Arg("256")
        .Stdin(Stdio.Pipe())
        .StdinBytes(Utf8.GetBytes("hello\n"))
        .Run();
    Console.WriteLine(h.Stdout);

    // 5. 后台跑 + 流式喂 + 收尾
    ProcessHandle p = new Process("sort").Stdin(Stdio.Pipe()).Spawn();
    ProcessStdinStream sin = p.GetStdinStream();
    byte[] payload = Utf8.GetBytes("b\na\nc\n");
    sin.Write(payload, 0, payload.Length);
    sin.Close();                     // 子进程看到 EOF
    ProcessResult sorted = p.Wait(); // 之后 p 即失效
    Console.WriteLine(sorted.Stdout);

    // 6. 超时
    try {
        new Process("sleep").Arg("60").Timeout(500).Run();
    } catch (ProcessTimeoutException e) {
        ConsoleError.WriteLine($"{e.Program} 超过 {e.TimeoutMs}ms");
    }

    // 7. 把输出直接落到文件，不经内存
    new Process("tar").Arg("-cf").Arg("-").Arg("src")
        .Stdout(Stdio.ToFile("src.tar"))
        .Run()
        .EnsureSuccess();
}
```

## 不支持

- **不经 shell，也没有 shell 模式**：没有 `"cmd arg1 arg2"` 这种整串形式，没有管道操作符、
  重定向、通配符展开。要用 shell 语法就自己 `new Process("sh").Arg("-c").Arg("…")`。
- **`Timeout(...)` 只管 `Run()`**：`Spawn()` 出来的句柄没有超时，要自己 `TryWait()` 轮询再 `Kill()`。
- **`Kill()` 与 `KillForce()` 当前没有区别**，都是不可捕获的强制终止；没有「先礼后兵」的
  `SIGTERM` 入口，也没有发送任意信号的 API。
- **`Stdio.ToFile` 不能用于 stdin**（会抛 `Std.Exception`）；也没有「追加到文件」的模式。
- **`Spawn()` 忽略 `StdinBytes` 与 `ShareProcessGroup`**。
- **没有 `using (...)` 语句**：`ProcessHandle` 虽然实现 `Std.IDisposable`，仍要自己
  `try` / `finally` 里调 `Dispose()`（或者走 `Wait()`——它同样会终结句柄）。
- **没有 async / 事件回调**：不存在 `OutputDataReceived` 之类；流式读写是阻塞的。
- **拿不到资源统计**：没有 CPU 时间、内存峰值、启动 / 结束时间戳。
- **没有进程枚举 / attach**：只能管自己启动的子进程，无法按 pid 找一个已有进程。
- **`ProcessResult` 的字段是公开可写字段**，不是只读属性——改了不会影响子进程，但也没人拦你。
- **`Ansi` 没有 256 色 / truecolor / 背景色**，也没有光标定位、清屏、隐藏光标的生成接口。
- **`Ansi.SetEnabled` 不可撤销**：调过之后就再也回不到自动检测。
