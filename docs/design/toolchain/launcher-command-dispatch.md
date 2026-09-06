# launcher 命令分发架构

> ⚠️ **前瞻设计草案（未实施）**。为把 `z42` 从"固定几条内建命令的 muxer"演进成可扩展命令分发器（new/build/publish/test/workload/平台打包…）铺路。落地开 spec。

## 已实施的当前路由（move-publish-to-z42b, 2026-07-01）

下面是**当前真实**的命令归属（baked 进 `launcher_cli.z42`，非上述前瞻发现机制）：

| 命令 | launcher 行为 | 实现 owner |
|------|--------------|-----------|
| `z42 build` | 转发 `bin/z42c` apphost（`_forwardZ42c`） | **z42c**（编译器） |
| `z42 test` / `bench` / `clean` | 转发 `programs/z42b/z42.builder.zpkg`（`_forwardZ42b`） | **z42b** |
| `z42 publish` | 解析 rid + 预解析 desktop workload apphost stub → 经 `Z42_APPHOST_TEMPLATE` env 转发 z42b（`_forwardZ42bEnv`） | **z42b**（`builder_publish.z42`） |
| `z42 export ios/android/wasm` | launcher 直接处理（`launcher_export.z42`） | launcher（workload 库） |
| `z42 run` / `install` / `list` / … | launcher 直接处理 | launcher |

- **build = 编译 → z42c**；**publish = 部署编排 → z42b**；launcher 是 muxer + runtime 解析。
- **publish 自带编译（build-always，fix-publish-stale-payload 2026-09-06 修正）**：z42b publish **每次**都先经 z42c 编一遍再产 apphost → `z42 publish` 一步完成 build+deploy，且**源码改了就重出产物**。「新旧判定」整个交给 z42c 的文件级内容哈希增量缓存（未变即空转、零字节重写），publish 自己不作任何判断。
  - 此前是 **build-if-needed**：「zpkg 文件存在」就当「已最新」直接跳过编译——改完源码再 publish 会静默把**旧 payload** 重新签进 apphost（退出码 0、无提示）。这是所有 publish 路径共有的洞，非某个工程特有。
  - **`--no-build`**：显式声明「就用现成的字节」，给自己已用特定编译器编好、要求字节不被动的调用方（xtask 的 SDK 组装 / 自举不动点路径）。旧行为的显式化版本。
  - 依赖 / native / payload 的**拷贝**同样改为 overwrite（此前 `if (!File.Exists(dst))` → dst 在就不覆盖，同一类陈旧）。
- **launcher 的 publish 旗标必须逐个转发**（`_publishForwardArgv` 手工重建 z42b 的 argv）：注册在
  `launcher_cli.z42` 却没重建进 argv 的旗标会**被静默吞掉**。`--self-contained` 曾整条缺失
  （launcher 连注册都没有 → `z42 publish --self-contained` 直接报未知旗标，只有 `z42b publish`
  能走到嵌入路径，与 embed-workload-publish「开箱即用」的目标不符），fix-publish-self-contained-forward
  2026-09-06 补齐。
  > 嵌入三件套（embed apphost / stdlib libs / libz42）来自工程的 `[build] hooks` 或 `Z42_EMBED_*` env
  > —— **desktop workload 只带 spawn 用的 apphost stub**，不带嵌入件；把它们打进 workload 是尚未做的缺口。
- **apphost stub 解析留 launcher**（它管已装 runtime/workload），经 `Z42_APPHOST_TEMPLATE` 传 z42b，故 z42b 的 publish **不含任何 runtime/workload 解析**，也**不依赖 z42.project/z42.build**（不触发自举串味）。

## 问题

`z42` 要分发越来越多命令：创建项目、编译、导出平台工程、打包发布、下载 workload/runtime…… 全 baked 进 launcher 会让它臃肿、与编译器/平台发布节奏强耦合、且每加一个平台就要重编 launcher。需要一套"目录发现 + 命令行注册"并存的机制。

## 调研结论（主流工具怎么做）

| 范式 | 代表 | 外部命令发现 | 父 help 带描述 | 版本作用域 | workload |
|------|------|------|:--:|:--:|:--:|
| PATH 约定 | git / cargo / kubectl | `tool-<cmd>` 丢 PATH | ❌ 只列名 | ❌ | ❌ |
| **manifest 驱动** | **dotnet** tools/workloads | 清单声明 | ✅ | ✅（SDK band）| ✅ 一等公民 |
| shim + toolchain | rustup | 代理 + override | — | ✅ | 近似 |

**决定性观察**：选纯 PATH 约定的（git/cargo/kubectl）全栽在"父级 `-h` 给不出外部命令描述"——kubectl 不得不在外面套 krew 补元数据。z42 刚把 launcher 迁到 `Std.Cli` 就是为了每层 help 带描述，纯约定会退回这个问题。z42 又自定位"dotnet muxer + rustup"，且现成基础设施（版本作用域 `runtimes/<ver>`、zpkg 包、`runtimeconfig.json` sidecar、项目本地 `.z42`）与 dotnet 模型几乎 1:1 → **采 dotnet 的 manifest + 版本作用域 + workload，保留 cargo 裸约定作 fallback**。

## 三层命令（按"谁提供 / 怎么注册"分）

| 层 | 例子 | 提供者 | 注册方式 | 为何 |
|----|------|--------|---------|------|
| **Core** | run / install / link / list / which / uninstall / info | launcher 自带 | **代码注册**（Std.Cli router，已有）| 引导关键——得先有 install/run 才能装 SDK |
| **SDK** | new / build / test / fmt | 随 SDK 装 | **目录发现**（`$Z42_HOME/programs/`，**跟 SDK 走**）| 与编译器同 SDK、可独立发布 |
| **Workload** | 平台 publish/export/工程生成 + 模板 + native 包（xcframework/AAR）+ **apphost（desktop publish 产物）**（desktop/ios/android/wasm）| 按需 `z42 workload install` | **目录发现**（`$Z42_HOME/workloads/`，**跟 SDK 走**，drop-in 即注册）| 平台按需、host 无关平台不强塞；ABI↔runtime 版本绑定暂不做 |

> **命令模型 + apphost 归属（define-cli-command-model, 2026-06-17）**：完整动词语义（字节码模型 / build-zpkg / run 双形态 / publish-deployable / export-IDE工程）见 [platform-export-lifecycle.md](platform-export-lifecycle.md)。要点：
> - **取消 `z42 apphost` 命令**。apphost 是 `[platform.desktop]` 配置驱动的 desktop **publish 部署件**，经 **`z42 publish`**（release，留存）产出；**`z42 run desktop`** 产 debug apphost 临时跑（预演部署启动）。`apphost.z42` 的 stub-patch 逻辑是这两者的实现，不再是独立 verb。
> - **`export` 仅 ios/android**（生成 Xcode/gradle 原生工程）；desktop 无 IDE 工程可导出 → 没有 `export desktop`。
> - **`run` 双形态**：`run`（无参）跑 zpkg 字节码于 host vm；`run <plat>` 以平台部署形态跑（desktop=apphost / mobile=on-device / wasm=浏览器）。
> - 门控不变：默认 `build`/`run`（无参）零 workload；`publish`/`export`/`run <plat>` 才下载对应平台 workload（desktop 亦 workload，仅 publish/run 维度；host runtime 仍经 `z42 install`）。
> 详见 [runtime-workload-distribution.md](runtime-workload-distribution.md) + `docs/spec/changes/consolidate-platform-into-workload/`。

判据：**Core 必须 baked**（鸡生蛋）；**SDK + workload 必须目录发现**（否则每加平台重编 launcher、无法按平台隔离）。

> **命令归属裁决（2026-06-20）**：**SDK 命令 + 平台 workload 都跟当前 SDK 走**，装在版本无关的 `$Z42_HOME/programs/`（命令）/ `$Z42_HOME/workloads/`（workload），**不进 `runtimes/<ver>/`**（`runtimes/<ver>/` 只放 app 运行时）。
> - **多版本命令作用域**暂不做（复杂、当前无需求，见 Deferred `cmd-future-version-scoped`）。
> - **workload↔runtime 版本绑定**暂不做：native 嵌入件 ABI 理论上须配 runtime，但当前为简化跟 SDK 走；多版本共存且 ABI 真冲突时再引入（见 Deferred `workload-future-version-scoped` + [runtime-workload-distribution.md](runtime-workload-distribution.md)）。

## 两种注册机制 → 合并进同一棵 Std.Cli 树

```
launcher 启动：
  1. 代码注册 core            → router.Add("run", desc, runHandler)
  2. 扫命令目录 + 读 manifest  → 对每个发现的命令 router.Add("build", desc, spawnZpkg(build.zpkg))
  3. root.Resolve(argv)：
       命中 core handler → 直接执行
       命中发现的命令   → spawn `z42vm <cmd.zpkg> -- <剩余 argv>`（裸透传，同 `run`）
```

**目录发现产出的 router 注册项与代码注册项同形** → `z42 -h` 一棵树里同时列 core+SDK+workload（各带描述）、每层 `-h`、未知命令一致报错，全是 Std.Cli 现成的。外部命令自己再用 Std.Cli 解析其子命令/flag。这就是"目录 + 命令行注册"的统一点。

## 目录发现的设计

### 发现机制：sidecar manifest（首选）+ 裸约定（fallback）

每个命令带一个 sidecar `<cmd>.cmd.toml`：

```toml
[command]
name        = "build"
summary     = "compile a z42 project to a zpkg"
zpkg        = "z42c.driver.zpkg"   # 相对本目录
min-runtime = "0.3.0"
```

- launcher 扫目录读 manifest（廉价、不执行）→ 据此建带描述的 router 项。
- 无 manifest 时 fallback：裸 `z42-<cmd>.zpkg`（name-only，无描述）——保住第三方 drop-in 低门槛。

> 为何不学 dotnet 的单 `commands.toml` 索引：per-command sidecar 让"装/卸一个命令 = drop/删一对文件"，无需并发改一个共享索引文件（避免写竞争）。

### 目录布局（SDK 级 + 全局 + 项目本地），按优先级合并

```
<repo>/.z42/programs/<cmd>/                 项目本地（pin；复用 install-z42 的 .z42 隔离模式）   ← 最高
$Z42_HOME/programs/<cmd>/                   SDK 命令（跟 SDK 走，版本无关）
$Z42_HOME/workloads/<wl>/                   workload 装入（跟 SDK 走，版本无关；drop-in）
$Z42_HOME/tools/                            全局用户工具（类 ~/.cargo/bin）                  ← 最低
```

每个 `programs/<cmd>/` 含 `<cmd>.zpkg` + `<cmd>.cmd.toml`（子目录隔离，对齐 [launcher.md](../runtime/launcher.md) 的 `programs/` 布局）。

- **SDK 命令 + workload 跟 SDK 走**：new/build/test/fmt + 平台 workload 随当前 SDK，**不进 `runtimes/<ver>/`**；多版本命令作用域 + workload↔runtime ABI 绑定均暂不做（见 Deferred `cmd-future-version-scoped` / `workload-future-version-scoped`）。
- 优先级：core > 项目本地 > SDK programs > workloads > 全局 tools；同名 first-wins-by-precedence。
- ⚠️ **目录扫描必须显式排序**（[common-pitfalls.md §1](../../../.claude/rules/common-pitfalls.md)）：`Directory.Enumerate` 跨 OS 顺序不定，first-wins 注册前按稳定键 sort，否则 CI 跨平台炸。

## 分发与参数透传

- 外部命令 = 一个 zpkg，经 `z42vm <cmd.zpkg> -- <argv[1:]>` 跑（裸透传，含 `--` 尾参、退出码透传，同 `run`）。
- 命令自身链 `Std.Cli` 解析它的子命令/flag/positional → 它内部也享受每层 help。

## workload 安装

`z42 workload install ios` → 下载 workload 包（命令 zpkg + `.cmd.toml` + 模板 + native 资产如 xcframework）→ 解到 `$Z42_HOME/workloads/ios/`（跟 SDK 走）→ 下次 `z42 <cmd>` 自动发现。对标 `dotnet workload install`。卸载 = 删目录。

## Decisions

| # | 决定 | 理由 |
|---|------|------|
| 1 | core 只留运行时自管理，baked-in | 引导关键 + launcher 与编译器版本解耦 |
| 2 | SDK/workload 走 sidecar manifest 目录发现 + 裸约定 fallback | help 完整（manifest）+ drop-in 低门槛（约定）；避开纯约定的"无描述"病 |
| 3 | 全部合并进同一棵 Std.Cli router | 统一 help/分发/未知处理，复用既有库 |
| 4 | SDK 命令 `programs/` + workload `workloads/`（均 SDK 级）+ 项目本地 + 全局三层，显式排序 | 命令 + workload 跟 SDK 不进 runtime；多版本作用域 + workload ABI 绑定均 deferred；deterministic |
| 5 | 平台支持 = 可安装 workload | 基础包小、平台按需、host 无关平台不强塞 |

## Deferred / 待 spec 细化

### workload-future-command-discovery（B1，2026-06-19）
- **来源**：`impl-command-discovery` DRAFT + `build-workload-subsystem` B1
- **触发原因**：B1 单做"发现 0 个命令"，无 user-facing 价值；需先有 B2（workload 包格式 + install）产出可发现的命令
- **前置依赖**：B2 workload 打包 + `z42 workload install` 落地；`runtime-dynamic-load-call` VM 实现
- **触发条件**：B2 完成后（已有真实 workload 可被发现）
- **当前 workaround**：export/publish/workload 全部 baked 进 launcher core（`launcher_cli.z42`）；命令增减需重编 launcher
- 实施顺序应为 **B2 → B1**（B1 鸡蛋问题决策 Z）；Std.Cli 需扩"spawn 式叶子"（`stdlib` 锁）

### workload-future-b2-packaging（B2，2026-06-19）
- **来源**：`build-workload-subsystem` B2
- **触发原因**：workload 按需自动下载 + manifest `workloads` 段尚未实施；当前 workload 靠 `z42 workload install --from <dir>` 本地手装
- **前置依赖**：`runtime-dynamic-load-call` VM builtin 实现（`__load_zpkg`）；GitHub Releases workload 包格式定稿
- **触发条件**：反射 MVP + 编译器自举完成后

### workload-future-b4-test-driven（B4，2026-06-19）
- **来源**：`build-workload-subsystem` B4 / 原 `consolidate-platform-into-workload` S4
- **触发原因**：平台一致性测试（R1–R7）目前是手维护脚手架；改由 workload 生成/驱动需先有 B3（✅ 已完成）+ B2
- **前置依赖**：B2
- **触发条件**：B2 完成后

### workload-future-b5-lifecycle（B5，2026-06-19）
- **来源**：`build-workload-subsystem` B5
- **触发原因**：`z42 new/platform add/test/fmt` 等完整 export/publish 生命周期命令尚未实施
- **前置依赖**：B1–B4

---

### cmd-future-version-scoped（命令多版本作用域，2026-06-20）
- **来源**：2026-06-20 命令归属裁决
- **触发原因**：命令现跟 SDK 走（`$Z42_HOME/programs/`，版本无关）；"不同 runtime 配不同 SDK 命令/打包器"的版本作用域复杂且当前无需求
- **前置依赖**：多 SDK 版本共存的实际场景出现（如同机同时维护 0.3.x / 0.4.x 项目，且命令行为不兼容）
- **触发条件**：用户呼声 / 跨版本命令行为冲突实际发生
- **当前 workaround**：单 SDK 命令集；`--runtime <ver>` 只切 app 运行时，不切命令实现

### 原有 Deferred 条目

- `.cmd.toml` 完整 schema（arg 摘要、别名、min/max-runtime 区间）。
- workload 包格式（manifest + packs 布局、依赖/版本解析、签名）。
- 命令版本冲突/多版本共存策略；`z42 commands list` 自省。
- 与 `z42up`（roadmap 1.0 版本管理工具）的边界。
- **`launcher-future-self-update-windows`**：Windows 上 `z42 self-update` 时替换 `$Z42_HOME/programs/launcher/` + `bin/z42vm` 因 `z42.exe`（父进程等 z42vm）持有文件锁而失败。待 rename-then-copy 策略或 PowerShell 延迟替换实现。当前 workaround：Windows 用户改用 `install-z42.bat --system`。见 `docs/design/runtime/launcher.md` 中的 `launcher-future-self-update-windows` 条目。
