# 跨语言对标

> 代码：`scripts/xtask_bench_compare.z42`、`scripts/xtask_bench_compare_report.z42`、`src/bench/compare/`
>
> 命令与旗标以 `xtask bench compare -h` 为准；工作负载清单与加法见 [src/bench/compare/README.md](../../../../src/bench/compare/README.md)。

[性能基准与回归门禁](benchmarking.md)回答「这次改动让 z42 变慢了吗」；本页回答另一个问题：
**z42 和其它脚本 / 托管语言比，在每个领域差多少？** 目标是每个领域都不掉队，所以这里只记录、
不判红——一格明显落后，就是一条该去查的根因。**本页的「当前结果」是对标数字的唯一权威位置**。

## 1. 测什么

24 条工作负载，每条用 z42、Python 3、JavaScript（Node）、Ruby、C#（.NET）、Java 各写一遍，
算法与参数相同，程序打印 `RESULT <checksum>`，runner 逐格核对。

| 类别 | 工作负载 | 测的是 |
|---|---|---|
| startup | `hello` | 进程启动 + 运行时初始化 + 一行输出（看 wall 与 RSS） |
| cpu | `fib` · `nbody` · `spectral_norm` · `mandelbrot` · `fannkuch` | 递归调用；double 运算 + 字段读写；double 数组紧循环；带分支的浮点内循环；小 int 数组排列 |
| calls | `vcall_mono` · `vcall_poly` · `closures` | 单态虚调用；4 个接收者轮转的多态虚调用；每轮新建捕获闭包并经高阶函数调用 |
| strings | `str_builder` · `str_split_join` · `num_format` | 各语言惯用 builder 拼 300 万段再逐字符扫描；200 字段 CSV 的 split + join；整数 ↔ 十进制字符串往返 |
| collections | `list_ops` · `dict_ops` · `sort` | `List<int>` push + 下标遍历；`int → int` 字典插入 / 查找（半数落空）/ 覆盖；标准库排序 |
| json | `json` | 用标准库把 1 万条记录建成 DOM、序列化成紧凑串、再解析回来遍历 |
| memory | `density_objects` · `_arrays` · `_strings` · `_intlist` · `_map` | 每单位字节数：小对象链表、`long[8]`、短字符串、`List<int>` 元素、`Dictionary<string,int>` 条目 |
| gc | `binary_trees` · `large_heap` | Benchmarks Game binary-trees（年轻代吞吐）；大存活堆下的持续替换（同 `src/bench/scenarios/13_gc_large_heap`） |
| concurrency | `parallel_sum` | 4 个 OS 线程分段求和再合并（spawn / join + 并行加速） |

## 2. 口径

- **wall time = 整进程**（启动、运行时初始化都算在内），与 `xtask bench` 的 e2e 口径一致。
  每条负载的参数让 z42 跑 0.1–4 s，启动只占小头；但对 .NET / Java / Node 这类跑完只要几十毫秒的格子，
  比值里有相当一部分是**启动时间之比**——读 `z42 / best` 时记住这一点，`hello` 一行给出纯启动的基线。
- **峰值 RSS** 来自 `/usr/bin/time`（macOS `-l`、Linux `-v`）；wall time 由 runner 计时，等待子进程用阻塞
  `wait`——`Process.Timeout` 的等待是 20 ms 轮询，会把每个计时量化成 20 ms 台阶，所以 runner 不设超时。
- **内存密度** = (RSS(N) − RSS(0)) / N：同一程序以 N 和 0 各跑一次，差值摊到每单位。量 z42 的 RSS
  **不能带 `--stats`**——它会把全部存活对象快照进一个 `Vec<Value>`，每对象多出 16–32 B 峰值。
- **交错执行**：每条负载内按「第 k 轮 → 依次跑所有 runtime」排列，机器负载漂移落在所有 runtime 上，
  比值是背靠背测出来的；每格取 `--runs` 次（默认 3）的中位数。
- **z42 的设置**：`--opt-all` 编译（等同 release 全优化）、`--mode jit`、默认 `gc-mode=generational`；
  memory / gc 两类另出一列 `z42-stw`（`--set gc-mode=stw`）。`--runtimes z42-interp` 可另加解释器列。
- **公平性约定**：
  - 只用各语言的**惯用标准设施**，不引第三方包、不调运行时旗标（.NET 只开 `InvariantGlobalization`，
    Java 默认 G1，CPython 无 JIT，Ruby 不开 YJIT）。
  - 浮点负载的运算顺序逐语言一致，checksum 逐位相同（`trunc(x × 1e9)`）。
  - 没有对应惯用设施的格子记 n/a，而不是硬凑：JDK 没有标准 JSON API ⇒ `json` 无 Java 列；
    JS 没有 StringBuilder ⇒ `str_builder` 用 `+=`（V8 的 rope）；Python 用 list + `join`；
    Java 没有 `List<int>` ⇒ 用装箱的 `ArrayList<Integer>`；Python / Ruby 的线程受 GIL / GVL 串行化，
    `parallel_sum` 照样用线程——量的正是这一点。
- **外部 runtime 都是可选的**：PATH 上找不到就跳过并打印原因，不是仓库构建的依赖。

## 3. 怎么跑

```bash
xtask bench compare                     # 全量：24 条 × 本机全部 runtime，每格 3 次取中位（本机约 6 分钟）
xtask bench compare --runs 5            # 本页结果用的设置（本机约 9 分钟）
xtask bench compare --quick             # 冒烟：quick 参数、每格 1 次（约 20 s），checksum 照样核对
xtask bench compare --only json,sort --runtimes z42,python,node
```

结果写到 `artifacts/reports/bench/compare.json`（`bench-compare-v1`：每格中位 wall、峰值 RSS、密度、
状态、z42 的 GC 最大停顿）和 `compare.md`（下面两张表就是它的原样输出）。任何一格崩溃或 checksum
不符 ⇒ 退出码 1。改了 `src/bench/compare/` 之后 `xtask test changed` 会跑 `bench compare --quick`。

更新本页：在空闲的机器上跑 `--runs 5`，把 `compare.md` 的两张表连同表头的环境信息替换进 §4，
再按新数字改写 §5。

## 4. 当前结果

- 日期：2026-10-07（UTC 02:50）；z42 commit `3a56823ed`（z42vm 0.6.0）
- 机器：Apple M2 Ultra，24 CPU，macOS（Darwin arm64）
- load average：开始 `4.30 8.84 13.52`，结束 `7.91 9.22 11.43`（机器上同时有别的构建，runtime 交错执行抵消了大部分漂移）
- `--runs 5`，每格取中位
- runtime：Python 3.9.6 · Node v26.7.0 · Ruby 2.6.10（无 YJIT）· .NET 10.0.102 · Java 21.0.2（G1）

**时间 / 密度**（越小越好；`z42 / best` 的 best 是除 z42 外最好的那个 runtime；`—` = n/a）：

| workload | unit | z42 | z42-stw | python | node | ruby | dotnet | java | z42 / best | z42 / python | z42 / node |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| hello | ms | 11.1 | — | 22.3 | 33.2 | 49.5 | 28.4 | 32.7 | 0.5× (python) | 0.5× | 0.3× |
| fib | ms | 277 | — | 440 | 48.6 | 265 | 37.6 | 43.6 | 7.4× (dotnet) | 0.6× | 5.7× |
| nbody | ms | 1431 | — | 3887 | 69.6 | 2396 | 56.7 | 75.6 | 25× (dotnet) | 0.4× | 21× |
| spectral_norm | ms | 177 | — | 4901 | 82.8 | 2634 | 54.4 | 70.7 | 3.3× (dotnet) | 0.04× | 2.1× |
| mandelbrot | ms | 132 | — | 4226 | 75.4 | 1652 | 78.2 | 80.3 | 1.8× (node) | 0.03× | 1.8× |
| fannkuch | ms | 257 | — | 731 | 57.8 | 829 | 49.4 | 65.6 | 5.2× (dotnet) | 0.4× | 4.4× |
| vcall_mono | ms | 532 | — | 1578 | 84.5 | 879 | 37.4 | 55.6 | 14× (dotnet) | 0.3× | 6.3× |
| vcall_poly | ms | 822 | — | 2436 | 159 | 1548 | 76.2 | 106 | 11× (dotnet) | 0.3× | 5.2× |
| closures | ms | 492 | — | 392 | 59.3 | 397 | 50.4 | 50.5 | 9.7× (dotnet) | 1.3× | 8.3× |
| str_builder | ms | 2152 | — | 273 | 200 | 241 | 55.0 | 87.5 | 39× (dotnet) | 7.9× | 11× |
| str_split_join | ms | 1894 | — | 146 | 187 | 435 | 114 | 190 | 17× (dotnet) | 13× | 10× |
| num_format | ms | 408 | — | 738 | 306 | 307 | 72.3 | 110 | 5.6× (dotnet) | 0.6× | 1.3× |
| list_ops | ms | 1416 | — | 2058 | 86.5 | 999 | 50.2 | 87.4 | 28× (dotnet) | 0.7× | 16× |
| dict_ops | ms | 1516 | — | 319 | 209 | 396 | 59.6 | 107 | 25× (dotnet) | 4.8× | 7.3× |
| sort | ms | 1859 | — | 501 | 252 | 258 | 88.7 | 266 | 21× (dotnet) | 3.7× | 7.4× |
| json | ms | 1861 | — | 66.8 | 46.7 | 108 | 90.5 | — | 40× (node) | 28× | 40× |
| density_objects | B/object | 108 | 100 | 81 | 71 | 80 | 31 | 25 | 4.4× (java) | 1.3× | 1.5× |
| density_arrays | B/long[8] | 269 | 247 | 203 | 202 | 151 | 91 | 89 | 3.0× (java) | 1.3× | 1.3× |
| density_strings | B/string | 134 | 102 | 73 | 64 | 119 | 74 | 104 | 2.1× (node) | 1.8× | 2.1× |
| density_intlist | B/element | 78 | 118 | 41 | 32 | 9 | 11 | 34 | 8.9× (ruby) | 1.9× | 2.5× |
| density_map | B/entry | 292 | 368 | 182 | 101 | 239 | 136 | 162 | 2.9× (node) | 1.6× | 2.9× |
| binary_trees | ms | 3343 | 2797 | 4770 | 143 | 2528 | 265 | 150 | 23× (node) | 0.7× | 23× |
| large_heap | ms | 3769 | 2056 | 2392 | 637 | 2205 | 959 | 293 | 13× (java) | 1.6× | 5.9× |
| parallel_sum | ms | 243 | — | 2548 | 799 | 1459 | 38.1 | 53.2 | 6.4× (dotnet) | 0.10× | 0.3× |

**峰值 RSS**（MB，中位）：

| workload | unit | z42 | z42-stw | python | node | ruby | dotnet | java |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| hello | MB | 12.0 | — | 7.4 | 42.0 | 24.1 | 35.7 | 36.1 |
| fib | MB | 12.2 | — | 7.4 | 45.6 | 24.1 | 35.6 | 37.2 |
| nbody | MB | 12.8 | — | 7.8 | 49.1 | 24.8 | 35.6 | 38.7 |
| spectral_norm | MB | 12.7 | — | 7.8 | 48.9 | 25.0 | 35.6 | 38.3 |
| mandelbrot | MB | 12.3 | — | 7.5 | 48.0 | 25.0 | 35.7 | 37.9 |
| fannkuch | MB | 13.0 | — | 7.6 | 47.4 | 25.5 | 35.7 | 39.8 |
| vcall_mono | MB | 13.7 | — | 7.5 | 47.1 | 25.0 | 35.6 | 37.5 |
| vcall_poly | MB | 13.8 | — | 7.5 | 47.4 | 24.5 | 35.6 | 37.5 |
| closures | MB | 98.8 | — | 7.5 | 48.2 | 26.2 | 41.6 | 40.3 |
| str_builder | MB | 348.9 | — | 110.0 | 218.5 | 41.3 | 80.3 | 119.3 |
| str_split_join | MB | 109.9 | — | 7.7 | 48.5 | 25.7 | 42.9 | 300.5 |
| num_format | MB | 69.7 | — | 7.5 | 66.0 | 24.7 | 42.2 | 92.3 |
| list_ops | MB | 141.8 | — | 76.9 | 112.8 | 47.6 | 50.1 | 96.6 |
| dict_ops | MB | 219.3 | — | 149.5 | 117.6 | 86.6 | 75.6 | 165.5 |
| sort | MB | 93.8 | — | 61.0 | 92.6 | 50.8 | 42.4 | 79.4 |
| json | MB | 261.2 | — | 26.6 | 54.5 | 46.1 | 69.8 | — |
| density_objects | MB | 115.2 | 107.3 | 85.0 | 110.0 | 101.3 | 65.2 | 60.5 |
| density_arrays | MB | 63.6 | 59.4 | 46.2 | 80.8 | 53.5 | 53.1 | 54.1 |
| density_strings | MB | 140.2 | 109.8 | 77.1 | 103.7 | 138.6 | 106.6 | 136.6 |
| density_intlist | MB | 754.0 | 1136.3 | 402.0 | 343.6 | 109.1 | 136.6 | 358.2 |
| density_map | MB | 292.3 | 364.9 | 180.6 | 138.1 | 253.0 | 165.2 | 191.3 |
| binary_trees | MB | 225.3 | 202.5 | 19.6 | 96.6 | 48.4 | 57.0 | 253.2 |
| large_heap | MB | 920.4 | 797.9 | 107.2 | 439.5 | 332.6 | 164.2 | 407.7 |
| parallel_sum | MB | 15.9 | — | 8.4 | 92.1 | 24.3 | 35.6 | 38.0 |

z42 两条 GC 负载里单次最大停顿（`GC.PauseStatsRaw`）：`binary_trees` generational 69 ms / stw 28 ms，
`large_heap` generational 73 ms / stw 86 ms。

## 5. 最大差距

按「离最好的 runtime 有多远」与「是否连 CPython 都不如」排序。z42 目前**启动最快**（11 ms、12 MB，
所有 runtime 里最好）、纯局部变量的 double 循环（`spectral_norm` `mandelbrot`）只差最好者 2–3 倍、
4 线程的 `parallel_sum` 能真正并行；下面这些是落后的领域。

1. **JSON：比 Node 慢 40×、比 CPython 慢 28×，峰值 RSS 261 MB 对 27 MB。** `Std.Json` 的 DOM 建树、
   序列化、解析三段都慢（1 万条记录约 0.13 / 0.6 / 1.1 s），且成本与规模线性——不是算法退化，是
   下面第 2 条字符串原语的成本被放大了。
2. **字符串：`str_builder` 比 CPython 慢 7.9×（RSS 349 MB），`str_split_join` 慢 13×。**
   `Std.Text.StringBuilder` 是纯脚本实现：每次 `Append` 存一个独立的 GC 字符串，`ToString` 时再合并；
   `Append(char)` 每次新建 `char[1]` + 字符串；逐字符 `s[i]` 每次都是一次完整调用（见
   `src/bench/scenarios/07_string_heavy` 头注）。
3. **集合：`dict_ops` 比 CPython 慢 4.8×、`sort` 慢 3.7×，`list_ops` 比 .NET 慢 28×。**
   泛型容器的 `T[]` 一律装箱、`List<int>` 每个元素一个 `Value`（[runtime-audit](../../../runtime-audit.md) §4.3）；
   `List.Sort` 是脚本里的归并排序，每次比较一次虚调用 `CompareTo`。
4. **调用：`fib` 7.4×、`vcall_mono` 14×、`vcall_poly` 11×（对 .NET）；`closures` 连 CPython 都不如（1.3×），
   且峰值 RSS 99 MB。** 调用协议与帧模型是解释器和 JIT 共同的天花板，闭包 / `CallIndirect` 每次按名字
   哈希（[runtime-audit](../../../runtime-audit.md) §4.1、§4.2）。
5. **GC：`binary_trees` 比 Node 慢 23×，`large_heap` 比 Java 慢 13×、峰值 RSS 920 MB（CPython 107 MB）；
   而且默认的 generational 在两条上都比 stw 慢（3343 对 2797 ms、3769 对 2056 ms）。** 根因是分代 + 增量的
   策略环与 TLAB 不复用死槽（[runtime-audit](../../../runtime-audit.md) §4.4）。
6. **字段密集的数值代码：`nbody` 比 .NET 慢 25×。** 同样是 double 运算，只用局部变量的
   `spectral_norm` / `mandelbrot` 只差 2–3 倍；差别在每次字段读写都要走句柄 → 对象锁 → 布局查表
   （[runtime-audit](../../../runtime-audit.md) §4.3）。
7. **内存密度：每单位字节数是 CPython 的 1.3–1.9 倍、JVM / CLR 的 3–4 倍**（小对象 108 B 对 Java 25 B；
   `List<int>` 元素 78 B 对 .NET 11 B）。对象头 72 B、负载单独 malloc（[runtime-audit](../../../runtime-audit.md) §4.3）。

写对标程序时撞到的语言 / 库缺口（不影响上面的数字，但都逼出了绕路写法）：

- **局部变量声明 `C[][] x = …`（元素是用户类）解析失败**（`int[][]` 与返回类型 `C[][]` 都可以），
  `new T[n][]` 这种交错数组分配也不支持 ⇒ `density_arrays` 只能用 `object[]` 存 `long[]` 再强转；
  runner 里只能改用 `List<CmpCell[]>`。
- **没有定点 / 格式化的 double → 字符串**（`double.ToString()` 只有最短往返形式，插值串不支持格式说明符），
  runner 自己写了 `_cmpFixed`。
- **`Std.IO.Process.Timeout` 把等待变成 20 ms 轮询**，子进程的退出时刻被量化成 20 ms 台阶（见 §2）。

## 6. 已知局限

- 结果只来自一台 macOS / arm64 机器，且测量时机器有其它负载；比值可信，绝对值可能偏高。
- 系统自带的 Python 3.9、Ruby 2.6 偏旧（新版 CPython 与带 YJIT 的 Ruby 更快），对比时把它们当「解释器下限」看。
- 跑得很短的格子（.NET / Java / Node 常在 40–100 ms）里启动时间占了大头，那些格子的比值偏保守。
- 没有 Lua：本机没有可用的 `lua`。runtime 列是在 `_cmpRuntimeFor` 里写死的一张表，加一种语言要改那里和每个工作负载目录。
