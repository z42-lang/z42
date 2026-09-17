# 嵌入 C ABI（`z42_host.h`）

> **对齐**：2026-09-17（change `restructure-docs-three-books`）｜ **代码**：`src/runtime/include/z42_host.h`、`src/runtime/src/host/`
>
> 反方向（native 代码把类型注册**进** z42）见[原生互操作契约](native-interop.md)；两边共用 `z42_abi.h` 的 `Z42Value` / `Z42Args` / `Z42Error`。

本页是**宿主开发者的契约面**：把 z42 VM 嵌进 C / C++ / Rust / Swift / Kotlin 程序时，
每个导出函数的签名、参数语义、返回码、生命周期与线程约束、以及**当前明确不支持什么**。
VM 内部怎么实现这套 ABI 不在本页。

调用序列永远是：`initialize` → `load_zbc` → `resolve_entry` → `invoke` → `shutdown`。
只想「跑一个打好包的 app，不要句柄」的话，直接用[一次性入口 `z42_host_run_app`](#一次性入口z42_host_run_app)。

```c
#include "z42_host.h"   /* 自带 #include "z42_abi.h" */
```

头文件随 SDK 包分发在 `native/include/`；仓内在 `src/runtime/include/`。
iOS / Android 的 facade 目录下各有一个同名**转发头**，内容只是 `#include` 上面那一份。

---

## 版本与句柄

```c
#define Z42_HOST_ABI_VERSION 1

typedef struct Z42Host*   Z42HostRef;     /* VM 实例 */
typedef struct Z42Module* Z42ModuleRef;   /* 已加载的 .zbc / .zpkg */
typedef struct Z42Entry*  Z42EntryRef;    /* 解析后的入口 */
```

三种句柄都是**不透明**的：非 NULL 即有效，NULL 即无效，不得解引用、不得比较大小、不得跨进程传递。
`z42_host_shutdown` 之后，该 VM 发出的所有 Module / Entry 句柄立即失效；拿失效句柄再调任何 API 返回 `ERR_NOT_INIT`。

**ABI 演进规则**：`abi_version` 永远在偏移 0；新字段只往结构体尾部追加，不重排；主版本号变更 = 显式 break。
用老头文件编译的调用方把 `Z42HostConfig` 整体清零即可安全地在新运行时上跑——新字段读到 0/NULL 就是「没配置」。

---

## 初始化配置

```c
typedef enum Z42ExecMode {
    Z42_EXEC_MODE_DEFAULT = 0,
    Z42_EXEC_MODE_INTERP  = 1,
    Z42_EXEC_MODE_JIT     = 2,
    Z42_EXEC_MODE_AOT     = 3
} Z42ExecMode;

typedef void (*Z42WriteSink)(const char* bytes, size_t length, void* user_data);

typedef int (*Z42ZpkgResolverFn)(
    const char*     namespace_name,
    const uint8_t** out_bytes,
    size_t*         out_length,
    void*           user_data);

typedef struct Z42HostConfig {
    uint32_t      abi_version;        /* 必须 == Z42_HOST_ABI_VERSION */
    uint32_t      reserved;

    Z42ExecMode   exec_mode;
    size_t        heap_initial_bytes;
    size_t        heap_max_bytes;

    Z42WriteSink  stdout_sink;        /* NULL = 真 stdout */
    Z42WriteSink  stderr_sink;        /* NULL = 真 stderr */
    void*         sink_user_data;

    const char* const* search_paths;  /* NULL 结尾的字符串数组；NULL = 只走内存 */

    Z42ZpkgResolverFn  zpkg_resolver;
    void*              zpkg_resolver_user_data;
} Z42HostConfig;
```

| 字段 | 语义 |
|---|---|
| `abi_version` | 不等于 `Z42_HOST_ABI_VERSION` → `ERR_BAD_CONFIG` |
| `exec_mode` | 只做合法性校验：值不在 0–3 → `ERR_BAD_CONFIG`；`JIT` / `AOT` 而运行时编译时没开对应 feature → `ERR_FEATURE_OFF`。**校验通过后，句柄式 `invoke` 一律走解释器**（见[当前不支持](#当前不支持)） |
| `heap_initial_bytes` / `heap_max_bytes` | **当前被忽略**，填 0 即可 |
| `stdout_sink` / `stderr_sink` | 见 [stdout / stderr 重定向](#stdout--stderr-重定向) |
| `sink_user_data` | 原样透传给上面两个回调。一份 config 只有一个，两个 sink 共用；要分开就 init 之后用 `z42_host_set_*_sink` 各设各的 |
| `search_paths` | 每个元素是一个**目录**。`z42.core.zpkg` 在哪个目录里被找到，哪个目录就是 stdlib 的 `libs/`，`using` 到的其他 zpkg 也从那里找。数组必须 NULL 结尾，且长度不超过 4096（否则 `ERR_BAD_CONFIG`）；路径必须是合法 UTF-8 |
| `zpkg_resolver` | 见 [zpkg resolver 回调](#zpkg-resolver-回调)。NULL = 只扫 `search_paths` |

`search_paths` 为空且没给 resolver 也是合法的——只是任何用到 stdlib 的 z42 代码会在 `invoke` 时失败。

> 用 C 写配置时**先 `memset(&cfg, 0, sizeof cfg)` 再填**：这是新字段向前兼容的前提。

---

## 状态码

除 `z42_host_run_app` 外，每个函数都返回 `Z42HostStatus`：

```c
typedef enum Z42HostStatus {
    Z42_HOST_OK                  = 0,
    Z42_HOST_ERR_ALREADY_INIT    = 1,
    Z42_HOST_ERR_NOT_INIT        = 2,
    Z42_HOST_ERR_BAD_CONFIG      = 3,
    Z42_HOST_ERR_FEATURE_OFF     = 4,
    Z42_HOST_ERR_BAD_ZBC         = 10,
    Z42_HOST_ERR_VERIFICATION    = 11,
    Z42_HOST_ERR_ENTRY_NOT_FOUND = 20,
    Z42_HOST_ERR_ARG_MISMATCH    = 21,
    Z42_HOST_ERR_VM_EXCEPTION    = 30,
    Z42_HOST_ERR_INTERNAL        = 99
} Z42HostStatus;
```

| 码 | 什么时候返回 |
|---|---|
| `OK` (0) | 成功。副作用：清空本线程的 last_error（`code` 归 0、`message` 变空串） |
| `ERR_ALREADY_INIT` (1) | 进程内已有活着的 VM 时再次 `initialize` |
| `ERR_NOT_INIT` (2) | 未初始化（或已 shutdown）时调用任何需要 VM 的 API，含拿失效句柄调用 |
| `ERR_BAD_CONFIG` (3) | `cfg == NULL`、`out_host == NULL`、`abi_version` 不匹配、`exec_mode` 越界、`search_paths` 非法（含非 UTF-8 路径、数组超 4096 项）、`search_paths` 里的 `z42.core.zpkg` 解析不了；另外 `resolve_entry` 的 `fqn == NULL` 或非 UTF-8、`shutdown` 的 `host == NULL`、`z42_zpkg_read_namespaces` 的 `visit == NULL` 也归这一档 |
| `ERR_FEATURE_OFF` (4) | 请求 `JIT` / `AOT`，但该运行时编译时没开对应 feature |
| `ERR_BAD_ZBC` (10) | 字节不是可解析的 `.zbc` / `.zpkg`；`bytes == NULL` 而 `length != 0`；依赖 zpkg 解析失败；**IR 约束校验失败也走这一档** |
| `ERR_VERIFICATION` (11) | **当前运行时从不返回**。枚举值保留，宿主不必为它写分支 |
| `ERR_ENTRY_NOT_FOUND` (20) | FQN 不在模块的函数表里；`module` 句柄为 NULL；`invoke` 的 `entry` 句柄为 NULL |
| `ERR_ARG_MISMATCH` (21) | 实参个数与入口签名不符；`args == NULL` 而 `n != 0`；实参或返回值的 `Z42Value` tag 不受支持 |
| `ERR_VM_EXCEPTION` (30) | z42 侧 `throw` 跨出入口顶层；静态初始化抛异常；调用到未解析的符号（stdlib 没找到时的典型表现） |
| `ERR_INTERNAL` (99) | Rust panic 被兜住、内部锁中毒，以及其他未归类失败 |

### 详细错误信息

```c
Z42Error z42_host_last_error(Z42HostRef host);
```

`Z42Error` 来自 `z42_abi.h`：`{ uint32_t code; const char* message; }`。

- **线程局部**：拿到的是**当前线程**上一次 host API 调用留下的错误。
- `host` 参数当前不被使用，传 NULL 也可以。
- `message` 由 VM 拥有，**不要 free**；有效期只到同一线程下一次 host API 调用为止——要留就自己拷。
- 没有待决错误时 `code == 0`、`message` 指向一个稳定的空串（不是 NULL）。
- 成功调用会清空它，失败调用会设置它。所以「先判返回值，再取 last_error」这个顺序是必须的。

---

## 生命周期 API

### `z42_host_initialize`

```c
Z42HostStatus z42_host_initialize(const Z42HostConfig* cfg, Z42HostRef* out_host);
```

**进程内同时只能有一个 VM。** 成功时 `*out_host` 被写成非 NULL 句柄；失败时先被写成 NULL，
所以调用方不需要自己预清零。`shutdown` 之后可以再 `initialize`（可以反复循环），
但两个 VM **不能同时**存在——第二次会拿到 `ERR_ALREADY_INIT`。

内部已串行化，从多个线程同时调用不会数据竞争；但语义上只有一个会赢。

### `z42_host_load_zbc`

```c
Z42HostStatus z42_host_load_zbc(Z42HostRef host,
                                const uint8_t* bytes, size_t length,
                                Z42ModuleRef* out_module);
```

字节可以是 `.zbc`，也可以是 `.zpkg`。**`bytes` 只需在本次调用期间有效**，返回后宿主可立即释放或复用缓冲。

加载时会顺带把依赖也解析进来：先请求 `z42.core`（即使用户代码一句 `using` 都没写），
再按用户产物记录的 import namespace 逐个请求；每个 namespace 先问 `zpkg_resolver`，miss 再扫 `search_paths`。
**依赖找不到不是错误**：`load_zbc` 照样返回 `OK`，直到 `invoke` 真的用到那个符号才报 `ERR_VM_EXCEPTION`。

`out_module` 可以为 NULL（只想验证字节能不能解析的话）。

### `z42_host_resolve_entry`

```c
Z42HostStatus z42_host_resolve_entry(Z42HostRef host, Z42ModuleRef module,
                                     const char* fqn, Z42EntryRef* out_entry);
```

`fqn` 是 NUL 结尾的 UTF-8 全限定名，两种写法都接受：

| 写法 | 例 |
|---|---|
| 点号（字节码里记录的形式） | `"Hello.Main"`、`"examples.hello.Greeter.Greet"` |
| 双冒号（可读形式，内部归一成点号） | `"examples.hello.Greeter::Greet"` |

顶层函数的 FQN 是「namespace + 函数名」，静态方法是「namespace + 类型名 + 方法名」。
空串或找不到 → `ERR_ENTRY_NOT_FOUND`。

### `z42_host_invoke`

```c
Z42HostStatus z42_host_invoke(Z42EntryRef entry,
                              const Z42Value* args, size_t n,
                              Z42Value* out_result);
```

同步调用，返回即执行完毕。注意这里**不传 `host`**——entry 句柄已经绑定了它的 VM。

- `n` 必须等于入口声明的形参个数，否则 `ERR_ARG_MISMATCH`（不做类型检查，只查个数和 tag 支持度）。
- `args` 在 `n == 0` 时可以为 NULL。
- `out_result` 可以为 NULL 表示丢弃返回值；不为 NULL 时**总会**被写入一个有定义的值——`void` 入口写入 NULL tag。
- 目前跨边界的值只支持 `Z42_VALUE_TAG_NULL` / `_I64` / `_F64` / `_BOOL`，见[当前不支持](#当前不支持)。

第一次对某个 module 的入口 invoke 时，会先跑一遍该模块合并后的静态初始化；
静态初始化失败是**粘性**的——之后每次 invoke 都报同一个 `ERR_VM_EXCEPTION`，不会拿半初始化的静态字段继续跑。

### `z42_host_shutdown`

```c
Z42HostStatus z42_host_shutdown(Z42HostRef host);
```

释放 VM 的全部状态（堆、模块表、入口表），并把配置进去的 stdout / stderr sink 摘掉。
`host == NULL` → `ERR_BAD_CONFIG`；VM 本来就没初始化 → `ERR_NOT_INIT`。

**必须保证调用 shutdown 时没有在途的 invoke。** 之后重新 `initialize` 是允许的。

---

## 生命周期与线程模型

- **单实例**：每进程同时一个 VM。
- **调用串行化由宿主负责**。运行时内部有锁保证不出现数据竞争，但**并发调用同一个 `Z42EntryRef` 的语义是未定义的**——要并发就自己加锁。
- **sink 回调在触发输出的那条线程上同步执行**，就在 `z42_host_invoke` 返回之前。回调里不要再调 host API。
- **只有执行 `invoke` 的那条线程的输出会进 sink**。z42 程序自己起的线程写的东西不走宿主 sink。
- **`user_data` 的线程安全归宿主**：运行时只把它原样传回去，从不解引用、从不加锁。
- **panic 不跨线**：Rust panic 被兜成 `ERR_INTERNAL`，不会展开到宿主的 C 栈。

---

## stdout / stderr 重定向

iOS / Android / wasm 没有真 stdout，必须重定向。

```c
Z42HostStatus z42_host_set_stdout_sink(Z42HostRef host, Z42WriteSink sink, void* user_data);
Z42HostStatus z42_host_set_stderr_sink(Z42HostRef host, Z42WriteSink sink, void* user_data);
```

对外行为：

- z42 侧 `Console.WriteLine` / `Console.Write` 及 stderr 对应物统一走 sink。
- **一次写出 = 一次回调**。`WriteLine` 的换行符拼在**同一个** buffer 里一起交付，不会单独回调。
- **顺序即写出顺序**：多次 `WriteLine` 严格按 z42 程序里的调用顺序触发 sink。
- **二进制安全**：`length` 不含 NUL，buffer **不保证** NUL 结尾。回调里不要用 `strlen`。
- 回调**不得留存 `bytes`**——返回之后那段内存就不再有效，要留就拷。
- `sink == NULL`（config 里或 setter 里）= 没有宿主 sink，输出回落到进程 stdout / stderr。
  注意 setter 传 NULL 是**清空**，不是「恢复成 `initialize` 时配的那个」——要换回去就再 set 一次。
- 两个 setter 各自带自己的 `user_data`，比 config 里那个共用的 `sink_user_data` 粒度细。

移动端 facade 的惯例是绑一个累积型 sink，invoke 结束后整体取出字符串。

---

## zpkg resolver 回调

桌面宿主可以用 `search_paths` 扫文件系统；移动端和 wasm 没有（或不便扫）文件系统，
就用回调告诉运行时「某个 namespace 的 zpkg 字节在这儿」。

```c
typedef int (*Z42ZpkgResolverFn)(
    const char*     namespace_name,   /* "z42.core" / "Std.IO" / ... */
    const uint8_t** out_bytes,
    size_t*         out_length,
    void*           user_data);
```

契约：

- **hit**：写 `*out_bytes` / `*out_length`，返回**非 0**。
- **miss**：返回 **0**，`*out_bytes` / `*out_length` 会被忽略，运行时继续回落到 `search_paths`。
  返回非 0 但 `*out_bytes == NULL` 或 `*out_length == 0` 同样按 miss 处理。
- 返回类型是 `int` 而不是 `Z42HostStatus`，因为 resolver 只有命中/未命中两种结果，没有错误空间。
- **字节生命周期 = 仅回调期间**。运行时在回调返回前就复制走需要的部分，所以宿主可以用栈缓冲、
  也可以在返回后立刻释放（Android JNI 的 `GetByteArrayElements` / `ReleaseByteArrayElements` 配对就是为此）。
- `zpkg_resolver` 与 `search_paths` **可以同时设**：resolver 优先，miss 才扫 `search_paths`。
- resolver 命中但字节解析不了 → `load_zbc` 返回 `ERR_BAD_ZBC`，错误消息里带 namespace 名。
- 两边都 miss 而用户代码确实引用了该 namespace → `load_zbc` 仍返回 `OK`，到 `invoke` 才报 `ERR_VM_EXCEPTION`。

### 从 zpkg 自己读出它答应哪些名字

一个 zpkg 常常同时提供多个 namespace（`z42.core.zpkg` 就同时 ship `z42.core` / `Std` / `Std.IO` …），
**不能假设 namespace == 文件名**。要建「namespace → 字节」表，就枚举手上的 zpkg，逐个问它：

```c
typedef void (*Z42NamespaceVisitor)(const char* ns, size_t len, void* user_data);

Z42HostStatus z42_zpkg_read_namespaces(const uint8_t* bytes, size_t length,
                                       Z42NamespaceVisitor visit, void* user_data);
```

- 对该 zpkg 的**每一个可解析键**回调一次：先是它的**包名**（`z42.core` 这种，prelude 就是按包名请求的），
  然后是它声明的每一个 namespace。
- `ns` 是 `len` 个 UTF-8 字节，**不 NUL 结尾**，只在本次回调期间有效——拷走再用。
- **无状态**：不需要先 `initialize`，也不改动任何 VM 状态。
- 字节不是可解析的 zpkg → `ERR_BAD_ZBC`；`visit == NULL` → `ERR_BAD_CONFIG`。

---

## 一次性入口：`z42_host_run_app`

```c
int z42_host_run_app(const char* app_zpkg, const char* entry,
                     const char* libs_dir, int argc, const char* const* argv);
```

加载一个打好包的 app（`.zbc` / `.zpkg`）、跑它的入口、然后整体拆掉。
**不用 `Z42HostRef`，也不用先 `initialize`**——它自建自拆一套 VM，和上面那套句柄 API 互不相干。

| 参数 | 语义 |
|---|---|
| `app_zpkg` | app 产物路径。必填，NULL 直接失败 |
| `entry` | 覆盖入口 FQN；NULL = 用产物里记录的入口 |
| `libs_dir` | stdlib `libs/` 目录（放着 `z42.core.zpkg` 及依赖）；可为 NULL |
| `argc` / `argv` | 转发给 app 的 `GetCommandLineArgs()` |

返回的是**进程风格的退出码**，不是 `Z42HostStatus`：

| 码 | 含义 |
|---|---|
| 0 | 正常跑完 |
| 1 | 加载或运行出错（消息已打到 stderr） |
| 2 | `app_zpkg` 为 NULL |
| 70 | VM 线程 panic / panic 跨过 FFI 线 |
| 71 | 起 VM 线程失败 |

这是各平台 test-host 与自包含 app 的共用入口：桌面 C shell、iOS Swift、Android JNI 调的都是这一个符号。
`Z42HostConfig` 里的 `exec_mode` 对它没有影响——它用运行时的默认执行后端。

---

## 完整例子：跑一个 Hello World

z42 一侧（`hello.z42`）：

```z42
namespace Hello;

using Std.IO;

void Main() {
    Console.WriteLine("hello, world");
}
```

编成 `hello.zbc`（入口 FQN 就是 `Hello.Main`）。C 宿主一侧：

```c
#include "z42_host.h"
#include <stdio.h>
#include <string.h>

static char   g_buf[65536];
static size_t g_len;

static void sink(const char* bytes, size_t length, void* user_data) {
    (void)user_data;
    if (g_len + length < sizeof g_buf) {
        memcpy(g_buf + g_len, bytes, length);   /* 不是 NUL 结尾，只能按 length 拷 */
        g_len += length;
    }
    g_buf[g_len] = '\0';
}

int main(void) {
    const char* paths[2] = { "/path/to/libs", NULL };   /* 放着 z42.core.zpkg 的目录 */

    Z42HostConfig cfg;
    memset(&cfg, 0, sizeof cfg);                        /* 向前兼容的前提 */
    cfg.abi_version  = Z42_HOST_ABI_VERSION;
    cfg.exec_mode    = Z42_EXEC_MODE_INTERP;
    cfg.stdout_sink  = sink;
    cfg.search_paths = paths;

    Z42HostRef host = NULL;
    if (z42_host_initialize(&cfg, &host) != Z42_HOST_OK) {
        fprintf(stderr, "initialize: %s\n", z42_host_last_error(NULL).message);
        return 1;
    }

    /* ...把 hello.zbc 读进 bytes / len... */
    Z42ModuleRef mod = NULL;
    Z42EntryRef  entry = NULL;
    Z42Value     result;

    if (z42_host_load_zbc(host, bytes, len, &mod) != Z42_HOST_OK ||
        z42_host_resolve_entry(host, mod, "Hello.Main", &entry) != Z42_HOST_OK ||
        z42_host_invoke(entry, NULL, 0, &result) != Z42_HOST_OK) {
        fprintf(stderr, "run: %s\n", z42_host_last_error(NULL).message);
        z42_host_shutdown(host);
        return 1;
    }

    printf("captured: %s", g_buf);      /* hello, world\n */
    z42_host_shutdown(host);
    return 0;
}
```

链接时带上 `libz42`（静态 `libz42.a` 或动态 `libz42.{dylib,so,dll}`），以及它依赖的平台库。

---

## 当前不支持

这些是**现在就会碰到的边界**，不是路线图：

| 事 | 现状 |
|---|---|
| 多 VM 实例 | 每进程一个。第二次 `initialize` 返回 `ERR_ALREADY_INIT`。`shutdown` 后可重来，但不能并存 |
| `invoke` 传 / 收 `string`、数组、对象 | 只支持 `Z42_VALUE_TAG_NULL` / `_I64` / `_F64` / `_BOOL`。其他 tag 一律 `ERR_ARG_MISMATCH` |
| 实参**类型**检查 | 只查个数和 tag 支持度，不比对声明类型 |
| `exec_mode` 真正选后端 | 句柄式 `invoke` 一律走解释器。`JIT` / `AOT` 的作用仅限于「feature 没开就在 `initialize` 报 `ERR_FEATURE_OFF`」 |
| `heap_initial_bytes` / `heap_max_bytes` | 被忽略。堆上限请用运行时设置那套旋钮 |
| 从宿主 catch z42 异常对象 | 只有 `ERR_VM_EXCEPTION` + `last_error` 里的消息串，拿不到异常对象本身 |
| 异步 / 协程式 invoke | 没有。`invoke` 一律同步；UI 线程切换由宿主自己做 |
| 运行时自动加锁 | 没有。并发调用要宿主自己串行化 |
| 卸载单个 module | 没有。只能整个 `shutdown` |
| `ERR_VERIFICATION` (11) | 枚举里有，运行时不产生。IR 校验失败走 `ERR_BAD_ZBC` |
