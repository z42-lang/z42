# 运行时设置（旋钮）

> 代码：
> `src/runtime/src/config/knob_table.rs`（旋钮全表）、`src/runtime/src/config/knobs.rs`（类型）、
> `src/runtime/src/config/render.rs`（三个查询命令的渲染）
>
> 命令行怎么设见 [`z42` 命令面](cli-z42.md)（`z42 run --set` / `--config` / `--mode`）；
> 在 z42 代码里读生效值见
> [`Std.Runtime.RuntimeConfig`](../stdlib/runtime-config.md)。

z42vm 的行为由一组**旋钮**控制：GC 算法、执行模式、日志过滤、采样频率、模块搜索路径……
本页给出旋钮清单、每个旋钮的取值语义与默认值，以及查询它们的三个命令。

## 怎么设一个旋钮

一个旋钮有四种设法，同一个 key 只有最高的那一层生效（其余被记为 `ignored`，查得到）：

| 层 | 怎么写 |
|---|---|
| 命令行 | `z42vm --set gc-mode=concurrent`（也可经 `z42 run --set` / `z42 repl --set` 透传） |
| 环境变量 | `Z42_GC_MODE=concurrent`（每个旋钮的环境变量名见下表） |
| 用户配置文件 | `Z42_CONFIG` 或 `z42 run --config <file>` 指向的 TOML，写在 `[runtime]` 表里 |
| 应用侧车 | `<app>.runtimeconfig.toml` 的 `[runtime]` 表，由 `z42c build` 从清单的 `[profile.<n>.runtime]` 烤出、随产物分发 |

**命令行最高、内置默认最低**，中间依次是环境变量、用户配置、应用侧车。逐 key 独立——
环境里设了 `Z42_LOG`、侧车里写了 `gc-mode`，两个都生效。

配置文件**只认 TOML**，且只读 `[runtime]` 这一张表：

```toml
[runtime]
gc-mode = "stw"
log     = "z42=error"
```

`--set` 的 key 只接受下表的旋钮名（kebab-case），**不接受环境变量名**：

```console
$ z42vm --set Z42_GC_MODE=stw app.zpkg
z42: unknown runtime knob `Z42_GC_MODE` in --set.
     Run `z42vm --list-knobs` (or `--list-knobs --all`) to see every knob.
```

### 设错了会怎样

| 来源 | 未知 key | 本 build 不可用 / 类型非法 |
|---|---|---|
| 命令行 `--set` | **报错，退出 2**（附最近邻建议） | **报错，退出 2** |
| 环境变量 / 配置文件 / 应用侧车 | 环境变量不检测；配置文件里的未知 key 警告 | 警告，回落默认后继续 |

```console
$ z42vm --set gc-mdoe=1 app.zpkg
z42: unknown runtime knob `gc-mdoe` in --set; did you mean `gc-mode`?
$ echo $?
2
```

`--strict-config`（等价 `Z42_STRICT_CONFIG=1`）把非命令行层的警告升级为致命错误，
退出码 2——给 CI 当配置漂移门。

环境变量层**不检测未知 key**：`Z42_` 前缀是全生态共享的（`Z42_HOME`、`Z42_LIBS`、
测试框架的 `Z42_TEST_*`），对着它们喊「未知旋钮」是纯误报。

### 更早一层：构建期的名字校验

侧车里的旋钮名来自工程清单的 `[profile.<n>.runtime]`。`z42c build` 在编译前先查一遍
**两个 profile** 的全部旋钮名：

```console
$ z42 build
z42c build: warning: [profile.release.runtime] 未知运行时旋钮 `gc-mdoe`——是不是 `gc-mode`？
  它仍会被烤进侧车，目标机 VM 每次启动都会警告并忽略它；`z42vm --list-knobs --all` 列出全部旋钮。
```

是 warning 不是 error（用旧工具链为新 VM 构建时，新旋钮还不在旧表里），且只查名字、
不查可用性——可用性取决于目标机的 build 与平台。

## 查询命令

三个命令都不需要 `<FILE>` 参数，打印完就退出。

| 命令 | 看什么 |
|---|---|
| `z42vm --list-knobs` | **有哪些旋钮**：环境变量名 / 类型 / 可设置层 / 本 build 可用性 / 默认值 / 谁读它 / 说明 |
| `z42vm --show-config` | **旋钮当前是什么值**、来自哪一层，以及某一层的值为什么没生效 |
| `z42vm --info` | 构建信息（版本 / target / arch / profile / features / exec modes）+ 完整旋钮快照——提 bug 时贴这个 |

两个修饰旗标：

- `--all`：让 `--list-knobs` / `--show-config` 连 `unsupported` 与 `internal` 档一起列出；
- `--json`：改输出 JSON 而非文本。

### 默认只看得到 14 个

`--list-knobs` 默认**只列 `public` 档的 14 个**，`--all` 才列出全部 47 个：

```console
$ z42vm --list-knobs
runtime knobs (14 of 47; pass --all for unsupported + internal knobs)
…
$ z42vm --list-knobs --all
runtime knobs (47 of 47)
```

`--show-config` 同样默认 14 行、`--all` 47 行。从 z42 代码里调
`Std.Runtime.RuntimeConfig.Names()` 拿到的是**全部 47 个**——脚本侧不分档。

那 47 个里有 5 个是**元旋钮**（`Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG` /
`Z42_HOME` / `Z42_PORTABLE_VM`）：它们决定读哪个文件、诊断多严格、SDK 装在哪，只收命令行与
环境变量，写进配置文件会自指（或者根本不是「这个应用的」设置），所以没有 kebab 形式的 key。

`--show-config` 的输出每行是 `key = 值  [来源层]`，被压过的层缩进列在下面：

```console
$ Z42_GC_MODE=stw z42vm --set gc-mode=concurrent --show-config
gc-mode = concurrent  [cli]
  ignored [env] "stw"  (overridden by a higher layer)
```

### `probing-paths` —— 依赖的额外搜索目录

zpkg **依赖**在运行期按文件名解析，搜索序是：

```
[entry-zpkg 所在目录] → [probing-paths 展开后的各目录] → [libs]
```

放在中间意味着：挨着 exe 放的那份仍然优先，共享的那份又优先于框架自带的。**这就是「声明为
共享就不复制过去」的后半段** —— 构建期不把它拷进 dist，运行期从这里找到。

```toml
# 工程清单 <name>.z42.toml
[profile.release.runtime]
probing-paths = ["../shared", "../vendor"]   # 多条写数组（推荐，跨平台无歧义）
```

`z42c build` 把它烤进 `dist/<app>.runtimeconfig.toml`，目标机上无需任何环境变量。

| 规则 | 行为 |
|---|---|
| 相对路径 | 相对 **entry zpkg 所在目录**，**不是 cwd** —— 同一个安装从任何工作目录启动结果一致 |
| 绝对路径 | 原样使用 |
| 通配符 | `*` 匹配一层、`**` 递归；展开结果是**目录**（依赖再按文件名到每个目录里找）|
| 展开时机 | 运行期 —— 安装之后新增的插件目录无需重新构建也能被发现 |
| 顺序 | 按声明序；同一模式的展开结果按路径排序（不依赖目录读取的偶然顺序）|
| 不存在的目录 | 静默跳过（可选的插件目录不该让启动失败）|
| 同名 zpkg 出现在多个目录 | 取搜索序里第一个命中的，不做版本比较 |
| `${Z42_HOME}` | 展开成本机 SDK 根（见下）；未知或未闭合的 `${…}` 让**整条**失效 |

**`${Z42_HOME}` 占位符** —— 指回 SDK 而不烤死路径（**由你写在清单里**：侧车里的值就是清单里那一行的逐字拷贝，
所以构建输出里不会出现具体路径。唯一例外是 [`deploy = "sdk"`](z42-toml.md)：有这类依赖时 z42c 会**自动补**下面这一条——
同样只是占位符）：

```toml
probing-paths = ["${Z42_HOME}/programs/z42c"]
```

侧车随产物分发，所以里面写绝对路径换台机器就失效，而相对路径对「装在任意位置的应用要指回
SDK」无解。占位符由 VM 在**运行期**替换，候选根按此序（都不成立则这一条跳过）：

| 序 | 来源 | 说明 |
|---|---|---|
| ① | `$Z42_HOME` | 显式指定的安装位置（launcher 转发时已设）|
| ② | `$Z42_PORTABLE_VM` | 反推 SDK 根（`<root>/bin/z42vm` ⇒ 上两级）；apphost 启动前会设它 |
| ③ | 正在跑的 z42vm 自己的位置 | 同样上两级 —— 两个环境变量都没设时仍然有效 |

多个候选根都存在时按上表顺序各展开一次（都进搜索序，不是只取第一个）。展开结果必须是**真实
存在的目录**才会进搜索序，所以反推错了的档自然落空。

**解析不到时的提示**：含 `${Z42_HOME}` 的条目在所有候选根下都展开不出存在的目录（这台机器没装
SDK，或只装了 runtime 包、没有 `programs/z42c/`）时，该条照旧跳过；但之后若有依赖解析失败
（`MissingSymbolException` / 找不到某个 zpkg），报错会多一行：

```
probing 路径 ${Z42_HOME}/programs/z42c 无法解析 —— 是否没有安装 z42 SDK？（安装 SDK，或设置 Z42_HOME 指向 SDK 根目录）
```

普通条目（如可选插件目录 `../plugins`）不存在是正常的，不触发这条提示。

> ⚠️ **未知占位符是整条作废，不是字面保留**：`${FOO}/x` 不会变成一个名叫 `${FOO}` 的目录去找。
> 这与清单里那套**编译期**模板变量（`${workspace_dir}` 等小写名）刻意不同 —— 编译期有诊断通道、
> 拼错当场报错；运行期这里没有不污染程序输出的通道，于是选了「跳过」这个既有语义。
> 大小写本身就是分界：`${lower_snake}` = 清单模板变量（编译期），`${UPPER}` = 环境派生的根（运行期）。

**两种写法**：

| 写法 | 何时用 |
|---|---|
| 数组 `["../a", "../b"]` | **推荐** —— 跨平台无歧义 |
| 字符串 `"../a:../b"` | 环境变量 `Z42_PROBING_PATHS` 只能这么写（沿 `PATH` 的惯例），清单里也仍接受 |

> 📌 **为什么多条要写数组**：路径列表的分隔符是**平台相关**的（unix `:` / windows `;`），所以一份
> 写成字符串的多条路径必然在某个平台上是错的 —— 这不是理论风险，本仓库的 `package-host(windows-x64)`
> 就因此红过一次（整串被当成一条不存在的路径）。
>
> 数组写法把摊平推迟到**运行这个应用的那台机器**：清单里的数组原样进侧车
> （`[runtime] probing-paths = ["../a", "../b"]`），由 VM 用它自己的分隔符拼 —— 那是唯一有权做这个
> 平台假设的地方。
>
> 数组只对**路径列表**类旋钮有效；其他旋钮写数组仍是非法值（不会被悄悄摊成字符串）。

## 旋钮清单（public）

日常会用到的就是这 14 个。「默认」一列是**未设时**的行为。

| 旋钮 | 环境变量 | 类型 | 默认 |
|---|---|---|---|
| `mode` | `Z42_MODE` | enum(`interp`\|`jit`\|`aot`) | build 默认（编进了 jit 就 jit，否则 interp） |
| `log` | `Z42_LOG` | string | `z42=warn`（`--verbose` 下 `z42=info`） |
| `libs` | `Z42_LIBS` | path | `<z42vm 所在目录>/../libs/`；开发树里再回落到当前目录下的 `artifacts/intermediate/libraries/flat/release` |
| `probing-paths` | `Z42_PROBING_PATHS` | path-list | 无 —— 依赖搜索序就是 `[entry-zpkg 目录, libs]` |
| `native-path` | `Z42_NATIVE_PATH` | path-list | 包相对搜索 |
| `crash-dir` | `Z42_CRASH_DIR` | path | 不写文件，崩溃报告只进 stderr |
| `gc-mode` | `Z42_GC_MODE` | enum（见下） | `generational-mark-sweep` |
| `gc-max-bytes` | `Z42_GC_MAX_BYTES` | string（字节数或带后缀） | 无上限 |
| `gc-trace` | `Z42_GC_TRACE` | bool | 关（关时不装观察者，零开销） |
| `jit-profile` | `Z42_JIT_PROFILE` | bool | 关 |
| `sample-hz` | `Z42_SAMPLE_HZ` | int ≥1 | 关（不起后台线程） |
| `sample-out` | `Z42_SAMPLE_OUT` | path | `z42-samples.folded`（仅在 `sample-hz` 设了时写） |
| `thread-stack-bytes` | `Z42_THREAD_STACK_BYTES` | string（字节数或带后缀） | `16M` |
| `trace-out` | `Z42_TRACE_OUT` | path | 不写 trace |

逐条取值语义：

- **`mode`** — 默认执行模式。命令行 `--mode` 压过它，它压过 build 默认。值 `aot` 会被
  收下，但没有 `aot` feature 的 build 会警告并回落 build 默认；`--mode` 旗标本身只接受
  `interp` / `jit`。
- **`log`** — `tracing-subscriber` 的 EnvFilter 指令串，如 `z42::jit=debug,z42=warn`。
- **`libs`** — 标准库 zpkg 的搜索目录。**单个路径**，不是路径列表。设了它就压过所有内置
  探测档，**经 apphost 启动的程序也一样**——这包括 SDK 自己的 `bin/z42c` / `bin/z42b` 等
  （它们都是 apphost）。`Z42_PORTABLE_VM` 只改 apphost 强制成哪个 libs，不影响 `Z42_LIBS` 的优先级。
- **`native-path`** — native `.dylib` / `.so` / `.dll` 模块的搜索路径，平台分隔符分隔。
- **`crash-dir`** — panic / 信号崩溃报告文件的落盘目录。
- **`gc-mode`** — GC 算法，六个取值：`stw` / `concurrent` / `generational`，以及三者各自的
  `-mark-sweep` 别名（`stw-mark-sweep` / `concurrent-mark-sweep` /
  `generational-mark-sweep`）。
- **`gc-max-bytes`** — 软堆上限。接受纯字节数，或带 `K` / `KB` / `M` / `MB` / `G` / `GB`
  后缀（`512MB`、`2G`）。它**不是**回收器的开关——回收器的阈值是相对增长量，不设上限
  照样工作；设了则同时收紧回收配额并加一道近上限跳闸。
- **`gc-trace`** — 每次回收往 stderr 打一行：种类、回收前后堆用量、回收字节数、停顿毫秒，
  外加近上限 / 超预算的边界事件。
- **`jit-profile`** — 打开 JIT 编译剖析。bool 接受 `true`/`false`、`1`/`0`、`yes`/`no`、
  `on`/`off`。
- **`sample-hz`** — safepoint 采样剖析器频率（Hz），任何 ≥1 的值即开启 z42 级 CPU 采样。
- **`sample-out`** — 采样火焰图的 folded-stacks 输出路径（inferno 格式）。
- **`thread-stack-bytes`** — VM 创建的线程（`Std.Threading.Thread`、嵌入式 app 运行线程）的
  原生栈大小，写法同 `gc-max-bytes`。z42 调用在原生栈上递归，栈用完是**致命错误**（打印 z42
  调用栈后退出，`catch` 拦不住），所以它决定了程序能递归多深。主线程的栈由操作系统决定，不受它影响。
- **`trace-out`** — chrome / perfetto 采样时间线 JSON 的输出路径；设了它就额外录一份
  逐采样时间线。


## 已退役的旋钮

| 旋钮 | 环境变量 | 原因 |
|---|---|---|
| `path` | `Z42_PATH` | 散装 `.zbc` 模块的搜索路径。**解析出来只进日志** —— 五个生产调用方全传空，它承诺的「`.zbc` 覆盖 `libs/` 里的 zpkg」从未生效过。该能力已被 zpkg 打包格式取代：VM 的惰性加载器不再按命名空间路由，改按 zpkg 文件名。要给 zpkg **依赖**加搜索目录，用 [`probing-paths`](#probing-paths--依赖的额外搜索目录) |

> 退役 = 从旋钮登记表删除。设了也不会有任何效果，`--list-knobs` 也不再列出它。
> 编号/名字空出来了，但**不复用**——复用会让老脚本里的配置突然有了新含义。

## 其余 29 个

`--list-knobs --all` 还会列出两档，**都不是稳定命令面**，取值语义以 `--list-knobs --all`
的实时输出为准：

- **`unsupported`**（17 个）——GC / JIT 的调参旋钮：`gc-adaptive-promotion`、
  `gc-incremental`、`gc-loh-bytes`、`gc-minor-threshold`、`gc-near-limit-ratio`、
  `gc-nursery-bytes`、`gc-pause-window`、`gc-pressure-ratio`、`gc-promotion-age`、
  `gc-slice-ms`、`gc-soft-threshold`、`gc-throttle-ratio`、`jit-interp-tierup`、
  `jit-threshold`、`osr-threshold`、`safepoint-throttle`、`stackalloc`。
  它们能设、会生效，但默认值随版本调整，不承诺稳定。
- **`internal`**（14 个）——机制内部件、保留位与五个元旋钮：`fusion-debug`、`no-fusion`、
  `no-typed-fusion`、`jit-debug-promote`、`gc-phases`、`repl-native`、
  `spawn-env-delay-ms`、`stress-iters`（只在 debug build 存在，且只收环境变量）、
  `target`（保留，尚未实现），以及 `Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG` /
  `Z42_HOME` / `Z42_PORTABLE_VM`。

一个旋钮的值要生效需四项全通过：允许的来源层 → build 类型（有的只在 debug build 存在）
→ 本二进制编进了它需要的 feature → 当前平台允许。任一不满足则值被丢弃并给一条诊断，
诊断里直接写清是哪一项：

```
z42: knob `jit-profile` (Z42_JIT_PROFILE, from [env]) is unavailable in this build:
     requires feature `jit`; this z42vm was built with: interp-only, native-interop.
     -> value ignored; using default (unset; JIT profiling off).
     Run `z42vm --list-knobs --all` to see every knob's availability.
```

## 应用侧车

`z42c build` 在产 `dist/<name>.zpkg` 的同时，把清单里生效 profile 的
`[profile.<n>.runtime]` 表烤成 `dist/<name>.runtimeconfig.toml`：

```toml
# 工程清单
[profile.debug.runtime]
mode = "interp"
```

```toml
# dist/<name>.runtimeconfig.toml（生成，勿手改）
# generated by z42c build — do not edit
# source: [profile.debug] of <name>

[runtime]
mode = "interp"
```

三条边界：

- profile 里没声明运行时旋钮 → **不产这个文件**；
- 目标路径已存在且不含生成标记头 → build 报错，不覆盖手写的侧车；
- `[profile.<n>]` 下**直接写键**（不放进 `.runtime` / `.properties` 子表）是清单结构错误，
  build 直接失败。

**运行时不需要任何人指路**：VM 按「与 app 文件同目录、同 stem」自己推出侧车路径。
`z42vm <app.zpkg>` 直跑、`z42 run`、publish 出的（非自包含）apphost 都是这样。找不到侧车是常态
（多数工程没有 `[profile.*]` 旋钮），安静跳过。

> **已知限制**：自包含桌面 app、iOS / Android、wasm 目前**不读侧车**，只认环境变量以及
> `Z42_CONFIG` / `Z42_APP_CONFIG` 显式指向的文件。在这些形态上，清单 `[profile.<n>.runtime]` 里的设置不会生效。

`Z42_APP_CONFIG` 只是**可选的显式覆盖**：它指向的文件解析出了内容就用它，指向一个不存在的
路径会警告并**回落到 app 自己的侧车**（指向一个**坏**文件——非法 TOML / `.json`——仍是硬错误）。
⚠️ 别把它 `export` 成全局变量：它描述的是**某一个** app，而环境变量会被子进程继承，
那会让子 app 拿到别人的侧车。要表达「这台机器上都这样」请用 `Z42_CONFIG`（用户层）。

侧车的另一半 `[properties]` 表装的是应用自己的配置，VM 原样搬运、不校验，运行时经
[`Std.Runtime.AppProperties`](../stdlib/app-properties.md) 只读。

## 诊断输出

`z42vm --stats` 在程序正常退出后把运行时计数器打到 stderr：`--stats` 是人读的文本块，
`--stats=json` 是单行 JSON（供工具抓取）。

`z42vm --verbose` / `-v` 等价 `--set log=z42=info`。
