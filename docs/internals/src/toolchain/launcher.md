# launcher：`z42` 命令怎么落到进程上

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/toolchain/launcher/core/`、`src/toolchain/workload/desktop/platform/apphost/src/`
>
> 命令与旗标怎么敲 → [工具链参考](../../../reference/src/toolchain/README.md)。

`z42` 是用户唯一敲的那个命令，但它几乎不干活：它是一个原生 stub 加一个 z42 程序，
负责**找到 z42vm、认出动词、把活派给 z42c / z42b / z42i，或者自己起一个子进程跑 app**。
**改命令面、改 SDK 目录布局、查「为什么跑到了另一个 z42vm」时读这页。**

## 两层结构

bootstrap 约束定了分界线：没有 VM 就跑不了 z42 代码，所以「找 VM、给 VM」这个最小核必须原生，
除此之外全部逻辑用 z42 写。

```
z42（apphost，Rust）
  │  探测 z42vm，payload 指向 programs/launcher/launcher.zpkg
  ▼
z42vm programs/launcher/launcher.zpkg -- <用户 argv 原样>
  │  解析动词 / 读 SDK 布局
  ▼
转发给 bin/z42c · programs/z42b · programs/z42i，或 spawn z42vm 跑用户 app
```

`z42` 与 per-app 的 apphost（`./xtask` 这类）**是同一个 stub、同一套解析逻辑**，只是内嵌的 payload
路径不同。stub 内嵌一段固定占位区：32 字节 MAGIC sentinel + 992 字节 payload（共 1024）；
patch 时按 MAGIC 定位，把 payload 覆写成「相对 exe 自身目录的 zpkg 路径 + NUL」。
patch 机制与 macOS 重签名见[部署模型](deployment-model.md)。

## SDK 目录布局

```
<SDK 根>/
├── z42                     # apphost；payload → programs/launcher/launcher.zpkg
├── bin/
│   ├── z42vm               # VM 主进程
│   ├── z42c                # apphost → programs/z42c/z42c.driver.zpkg
│   ├── z42b                # apphost → programs/z42b/z42.builder.zpkg
│   ├── z42d                # apphost → programs/z42d/z42.devtools.zpkg
│   └── z42i                # apphost → programs/z42i/z42.interactive.zpkg
├── libs/                   # stdlib + 工具链库 zpkg
├── programs/<tool>/        # 各工具的 zpkg 与其同址兄弟包
├── native/                 # 嵌入件：libz42.{a,dylib} + libz42_compression.* + include/
├── manifest.toml           # [package] version / rid / build-date + [contents] 清单
├── install.toml            # 安装脚本写的 version / rid / sha256
├── cache/                  # 单文件运行等的缓存根
└── runtimes/<ver>/workloads/<wl>/   # z42 workload install 装入的 workload
```

`z42` 在根而不在 `bin/`：它是包的统一入口，`bin/` 留给工具与 VM。
两处都进 PATH，于是 `z42`、`z42c`、`z42vm` 都能直接敲。

launcher 自己解析 SDK 根的顺序是 `$Z42_HOME` > 从 apphost 注入的 `$Z42_PORTABLE_VM` 反推 >
`$HOME/.z42`（Windows 回退 `%USERPROFILE%`）——与 apphost 和安装脚本同一顺序。

SDK 的安装与更新由安装脚本负责，launcher 不做自更新、也不管理多个运行时版本。

## z42vm 探测：所有原生入口共用一套

apphost 启动做两步。

**第一步 `ensure_portable_vm`（SDK 同址引导）**：若 `$Z42_PORTABLE_VM` 未设，且 exe 同址有 vm ——
`{exe_dir}/z42vm`（apphost 在 `bin/` 里，如 `bin/z42c`）或 `{exe_dir}/bin/z42vm`（apphost 在包根，
如 `z42`）—— 就把 `$Z42_PORTABLE_VM` 设成它。

为什么经环境变量而不是单开一档探测：① SDK 包里的工具必须用**自己那个包**的 vm，payload 与同包
`bin/z42vm` 的 zbc 版本必须配；② 环境变量会自动传给它 spawn 的子进程，于是 `z42` → launcher.zpkg
→ z42c/z42b 全链共用同一个 vm。exe 同址查找**只此一处**，范围收窄在 SDK 内部的程序上。

**第二步 `resolve_app_runtime`，most-local-wins：**

| 序 | 档 | 语义 |
|---|---|---|
| ① | `$Z42_PORTABLE_VM` | 显式钉死某个 vm（文件或其所在目录），也是上一步的载体 |
| ② | 从 exe 目录逐级上行的 `<d>/.z42` | 工程本地 SDK，必须压过任何全局设置 |
| ③ | `$Z42_HOME` | 用户显式指定的全局安装位置 |
| ④ | `$HOME/.z42` | 安装脚本的默认位置 |

每一档都当作 SDK 根用（`<root>/bin/z42vm` + `<root>/libs`）；全无 → 报错并列出已查路径、非零退出。
③ 必须排在 ④ 前面，否则装过默认位置之后 `$Z42_HOME` 就永远不生效。

角色分流全靠第一步有没有填上 ①：

- **`z42`**（包根，子 `bin/z42vm`）→ 命中 → 跑 `programs/launcher/launcher.zpkg`。
- **`bin/` 内的工具 apphost**（与 z42vm 同级）→ 命中 → 直接 `exec z42vm <自身 payload>`，
  **不经 launcher**；其包依赖经同址依赖搜索从 `programs/<tool>/` 解析、stdlib 从 `libs/`。
- **per-app apphost**（exe 近邻没有 vm）→ 不触发 → 落到 ② 的工程 `.z42`，venv 语义。

## 动词分派

命令树用 `Std.Cli` 的嵌套 router 建，`z42 <cmd> -h` 每层都带描述。分三类：

| 命令 | 谁实现 | 怎么到 |
|---|---|---|
| `run` / `version` / `help` | launcher 自己 | 直接处理 |
| `publish` / `export` / `workload` | launcher 自己 | router dispatch |
| `build` | **z42c** | `_forwardZ42c`：直接跑 `bin/z42c` apphost，argv 原样 |
| `new` / `test` / `bench` / `clean` | **z42b** | `_forwardZ42b`：`z42vm programs/z42b/z42.builder.zpkg -- <argv>` |
| `repl` | **z42i** | `_forwardRepl`：同上，另设 `ShareProcessGroup` |

还有一条简写：`z42 <app.zpkg|.zbc|.z42> [-- args]` 等价于 `z42 run <app>`。

三点机制细节：

- **转发命令在 router 里只登记名字与一句话说明**（`AllowExtras` 的空 parser），参数与帮助由目标工具
  自己给——否则命令面会在两处各写一遍、各自漂移。
- **转发都显式传 `Z42_LIBS`**，指向 SDK 的 `libs/`。用单个目录而不是拼多段路径：Windows 上
  `:` 分隔会被盘符冒号截断。
- **`repl` 必须 `ShareProcessGroup`**：REPL 要驱动 tty，留在新建的后台进程组里拿不到控制终端
  （没有提示符、读 tty 触发 `SIGTTIN`）。`z42 repl -h` 也必须由 launcher 拦下自己打印，
  否则 z42i 会忽略它直接进入交互。

`publish` 的转发多一步：apphost stub 的解析留在 launcher（它才管已装 workload），
解析结果经 `Z42_APPHOST_TEMPLATE` 传给 z42b，于是 z42b 的 publish 不含任何 runtime/workload 解析逻辑。

⚠️ **转发 publish 的 argv 是手工重建的**（`_publishForwardArgv`）：launcher 注册了却没重建进 argv 的
旗标会被**静默吞掉**。这条不是理论风险——`--self-contained` 曾整条缺失，导致只有直接敲 `z42b publish`
才能走到嵌入路径。加一个 publish 旗标 = 改两处。

## `z42 run`：四种目标，一条构建路径

`run` 不走严格的 `ArgParser`（`-- <program args>` 尾巴过不了），自己扫 argv：认 `--bin` /
`--mode` / `--config` / `--set`，遇到 `--` 就把剩下的全当程序参数，其余以 `-` 开头的一律报错不猜。

目标形态：

| 目标 | 做什么 |
|---|---|
| `<app.zpkg>` / `<app.zbc>` | 直接跑 |
| 目录 / `z42.toml` / 不给目标 | 定位清单 → `bin/z42c build --quiet` → 从 `BuildLayout` 解析 dist → 跑产物 |
| `<file>.z42` | 合成清单 → 走上面同一条工程路径 |

工程有多个 `[[exe]]` 目标时必须 `--bin` 指名，否则报错并列出目标。

### 单文件运行：合成清单

`z42 run hello.z42` **不另写一条单文件编译路径**，而是在缓存目录里合成一份最小清单，
之后完全复用工程构建路径——增量缓存、入口检测、运行配置侧车、`--mode` / `--set` / `--config`
透传全部照旧。语义一致由构造保证，而不是靠测试去追平两套实现。

缓存目录：`<缓存根>/run/<源文件绝对路径的 SHA-256>/`（`Z42_CACHE_DIR` 可覆盖缓存根，
缺省 `<SDK 根>/cache`）。按**路径**而不是内容取哈希——同一文件反复改仍落同一条目，增量才生效。
清单每次重写，避免用户移动文件后清单陈旧。

🔴 **源文件绝不复制进缓存目录**：复制后所有诊断的位置都会指向缓存路径，读者回不到自己的文件。
清单里 `include` 的是源文件**绝对路径**，诊断呈现再由 z42c 相对化。

合成出的清单只有两段：

```toml
[project]
name = "<文件名规范化>"   # 小写，非 [a-z0-9._-] 换 -，首字符非字母数字则前置 z
version = "0.0.0"
kind = "exe"

[sources]
include = ["<源文件绝对路径>"]
```

**没有 `[dependencies]` 段**（`_synthSingleFileManifest`）。这正是单文件模式只能用到 stdlib 的
「那一半」的原因：跨包命名空间在编译期过得去、运行期抛 `MissingSymbolException`。要声明依赖就得
建工程。缓存条目目前只增不减，清理靠手工删 `run/` 目录。

## 运行配置：两条独立通道

launcher 自己不解析旋钮，只负责把两个文件路径交给 z42vm，由 VM 按
[五层优先级链](../runtime/runtime-settings.md)归并：

- `--config <file>` → `Z42_CONFIG`，**用户配置层**。
- `<app>.runtimeconfig.toml`（与 app 同目录、同 stem）→ `Z42_APP_CONFIG`，**应用侧车层**。
  这个侧车由 `z42c build` 从清单的 `[profile.*]` 烤出，随产物分发。
- `--mode` 直接变成 z42vm 的 `--mode`；`--set k=v` 可重复，**原样透传**——
  key 合法性与类型诊断全归 z42vm，旋钮登记表在那边。

两者是**独立通道、逐 key 叠加、用户层赢**：用户 `export Z42_CONFIG=my.toml` 不会整份丢掉应用自带的
其余配置。per-app apphost 走的是自己的侧车发现（同目录同 stem），不经 launcher。

## 代码地图

| 组件 | 位置 |
|---|---|
| SDK 布局解析、`run`、单文件清单合成、`version` | `src/toolchain/launcher/core/launcher.z42` |
| 命令树、dispatch、三个转发函数 | `src/toolchain/launcher/core/launcher_cli.z42` |
| `export` 入口、`publish` 转发、apphost stub 解析、清单/产物定位 | `src/toolchain/launcher/core/launcher_export.z42` |
| `workload install/list/uninstall` | `src/toolchain/launcher/core/launcher_workload.z42` |
| `release-index.json` 读取、下载校验、解包 | `src/toolchain/launcher/core/launcher_network.z42` |
| apphost stub（MAGIC 占位区、运行时探测、exec） | `src/toolchain/workload/desktop/platform/apphost/src/{main.rs,hostrun.rs}` |

## 边界与限制

- **launcher 不做版本管理**：不选运行时版本、不自更新，SDK 单版本由安装脚本装/更新。
- **MAGIC 字符串有三处副本**（Rust stub 是权威，z42b 内联 patcher、desktop workload 的 patcher
  各一份），改它要三处同步。
- **publish 旗标必须逐个手工转发**，见上。
- **单文件清单无依赖段**，见上。
