# 设计:嵌入式 test-host 成熟化 —— 全面计划(参照 dotnet/runtime)

> 状态:**DRAFT**。配套 [proposal.md](proposal.md)(问题/目标),本文件给**架构 + 参照 .NET 的全面计划**。
> 对齐 2026-08-05。

---

## A. 参照方案:dotnet/runtime 怎么做(逐项映射到 z42)

dotnet/runtime 是"同一套测试语料 × 多 runtime flavor × 桌面/移动/浏览器"跑得最全的工程。它的分工:

| .NET 组件 | 职责 | z42 对应 |
|-----------|------|---------|
| **xunit runner** | 发现 + 跑 `[Fact]`/`[Theory]`,出结果 | `Std.Test.Runner`(agent 内) |
| **App 内嵌 runner**(mobile/wasm 把程序集+runner 打进 self-contained app) | 设备/浏览器上**进程内**跑 | **test-agent** 经 `z42_host_run_app`(§G6 已建) |
| **XHarness** | 在 iOS/Android/WASM **装·启·收结果** | **test-host driver**(testhost.c / xtask / CI 折叠 job)—— 需正名为 harness 层 |
| **RemoteExecutor** | 把片段丢**独立子进程**跑(治进程级全局态污染) | **L2 多实例**(§C) |
| **xunit 并行 collections** | 进程内**多线程**并行,collection 内串行 | **L1 进程内并行**(fresh VmContext,§C) |
| **Helix** | work item 撒**多机/多队列** | **L3 分片**(shardK/shardN + 多进程) |
| **Runtime flavors**:CoreCLR JIT / Mono interp / Mono AOT / **full-AOT@iOS** | 同语料 × 多执行模式 | interp / jit / aot + `--rid`(iOS 禁 JIT = full-AOT 同款约束) |
| **dotnet workloads** + `publish -r <RID>` | 装平台工具 + 按 RID 出 app | **workload** + `z42 publish --rid`(§B) |
| `[Collection]` / `DisableParallelization` / `[Trait]` | 用例**自声明**并行/隔离 | 用例 manifest **isolation trait**(§C) |
| xunit results XML / XHarness result 协议 | 标准化结果回传 | JSON report(有)+ **JUnit**(CI 已接) |

### 从 .NET 踩过的坑里学到的 4 条(直接改我们的设计)

1. **托管隔离(AppDomain)不够 → 硬用例上真进程**。.NET Core **删掉了 AppDomain**,改用
   **RemoteExecutor 起子进程**处理"碰进程级全局态"的用例。**教训**:z42 的 L1(fresh VmContext)只隔离
   **VM/托管态**;碰 **OS 全局态**(env / cwd / 信号 / 绑定固定端口 / IPC)的用例必须 L2 真进程。
   → **别把 L1 吹成万能**;明确 L1 管 VM 态、L2 管 OS 态。这正印证你说的"少数才需隔离进程"。
2. **用例自己声明,harness 不猜**。`[Collection]`/`DisableParallelization` 让用例声明能否并行。
   → z42 用例 manifest 带 **`isolation = context|process`** + **`parallel = true|false`**;默认 `context`+`true`。
3. **runner 内嵌进 app,harness 是外层 driver**。→ 印证 G6 架构(agent 内嵌,xtask/CI 当 XHarness)。
4. **wasm 单线程 → 并行靠独立实例**(worker/浏览器)+ Helix 分片。→ z42 wasm 并行 = **Web Workers**。

---

## B. 经 workload 构建(= dotnet workloads + publish)

- **.NET 模型**:`dotnet workload install wasm-tools|android|ios` 装平台工具链 → `dotnet publish -r <RID>`
  产出 self-contained app(含 runtime + 程序集 + 内嵌 runner)。测试 app 与用户 app **同一 publish 路径**。
- **z42 目标**:test-host = **workload 构建的一个 app**。现在 `_build{Wasm,Ios,Android}Testhost` 在 xtask 里
  ad-hoc 拼 cargo/wasm-pack/xcframework;迁到 **`z42b publish --rid <rid>` + workload manifest**:
  - `[platform.<rid>] link = static|dynamic`、assets(§1 语料 bundle)、entry(test-agent)。
  - test-host 当 **WorkloadBase 5 相位**(构建/资产/打包/部署/运行)的第一个真实消费者,逼着补齐。
- **产品化**:补齐后 `z42 publish --rid ios-arm64`(等)即**用户**构建跨平台 app 的入口 —— 测试地基顺带
  产出产品能力(与 .NET"测试 app 和用户 app 同一条 publish"对齐)。

---

## C. 并行架构(三层 + 自声明,参照 xunit/RemoteExecutor/Helix)

```
用例 manifest 自声明:  isolation = context(默认) | process     parallel = true(默认) | false
                              │                          │
      ┌───────────────────────┼──────────────────────────┼───────────────────────┐
      ▼                       ▼                          ▼                        ▼
  L1 进程内并行           L1 但串行(parallel=false)   L2 多实例(isolation=process)   L3 分片(吞吐)
  fresh VmContext         同池不并发                   独立进程/embed 实例             多进程 + shardK/N
  Rust 线程池 jobs        (≈ xunit collection 串行)    (≈ RemoteExecutor)             (≈ Helix)
  绝大多数用例                                          少数 IPC/OS 全局态             叠加在 L1 之上
```

- **L1 默认**:`__run_goldens_isolated(paths, entries, jobs)` builtin —— 每例 fresh `VmContext`(隔离 VM 态)+
  线程本地 stdout 捕获;`jobs>1` 走 Rust 线程池。**铁律**:heap 互斥,只跨 String。
- **L1 串行子集**:`parallel=false` 的用例在池里串行(≈ xunit collection 内串行 / 同 `[Collection]`)。
- **L2 多实例**:`isolation=process` 的用例(IPC:俩程序 socket/pipe 通信;或改 env/cwd/信号/绑固定端口)→
  起独立 embed 实例(host 各跑一份),它们之间真跨进程(≈ RemoteExecutor)。**少数**。
- **L3 分片**:desktop/CI 再多起进程 + `shardK/shardN`(现有),每进程内部再 L1 并行 → 二维并行(≈ Helix)。

> 隔离粒度**能省则省**:L1(context)覆盖多数;L2(进程)只给真碰 OS 全局态/IPC 的少数;L3 叠吞吐。
> 这就是 .NET 的最终形态(删 AppDomain、进程内线程并行为主 + RemoteExecutor 兜硬用例 + Helix 撒规模)。

---

## D. 移动 / web 并行(平台自适应,参照 XHarness)

| 平台 | L1 线程 | L2/L3 进程 | 策略(= .NET 对应) |
|------|:---:|:---:|------|
| **desktop** | ✅ 多核 | ✅ spawn | L1+L2+L3 全开(= CoreCLR 桌面 + Helix) |
| **iOS/Android** | ✅ 多核 | ⚠️ iOS 禁任意 spawn;Android 受限 | **主力 L1 线程**,`jobs`=核数;L2/L3 基本不用(= XHarness 单设备内嵌跑,full-AOT@iOS) |
| **web/wasm** | ❌ 单线程 | —— | 并行 = **Web Workers**:N worker × 独立 wasm 实例 × 一个 shard;worker 内顺序(= .NET wasm 多浏览器/worker 分片);builtin 线程池在 wasm 退化 `jobs=1` |

---

## E. 结果协议 + CI(参照 xunit xml / Helix → Azure）

- agent 出 **JSON report**(有)→ 折叠 job 已转 **JUnit** → GitHub Checks(= xunit xml → CI)。
- 全平台**同一 JSON schema**(desktop/mobile/wasm 一致),harness 只负责回收通道(stdout 文件 / VFS /
  asset)差异 —— 与 .NET"同一 xunit 结果、XHarness 适配回收"一致。

---

## F. 全面分阶段计划(每步 gate 不中途红)

| 阶段 | 内容 | 参照 | 验证门 |
|------|------|------|--------|
| **P0 隔离审计** | 扫现有 goldens/tests,标出碰 OS 全局态/IPC 的少数(→`isolation=process`),其余 `context` | RemoteExecutor 标注面 | 产出用例 isolation 清单 |
| **P1 golden 进 embed(L1,jobs=1)** | builtin `__run_goldens_isolated` + agent 编排 + 语义对齐(stdout/stderr/throw/interp_only)+ 接 `_enumerateCasesF` | app 内嵌 runner | embed 全 golden **与 host 并跑对账**,逐例 pass/fail 相等 |
| **P2 L1 并行 + 自声明** | Rust 线程池 `jobs`;`parallel=false` 串行;heap 互斥不变量 | xunit collections | 并行==顺序结果;TSan/压测无竞争 |
| **P3 L2 多实例** | `isolation=process` 用例走独立 embed 实例;IPC 样例 | RemoteExecutor | IPC/全局态样例验证;不污染 L1 池 |
| **P4 经 workload 构建 test-host** | host 构建迁 `z42b publish --rid` + workload;补 WorkloadBase 相位 | dotnet publish -r | 四平台经 workload 出的 test-host 跑通(= 现 CI 折叠 job) |
| **P5 平台并行** | wasm Web Workers 分片;mobile 线程池;`jobs`=核数 | XHarness/wasm workers | 各平台并行 RUN 绿(CI) |
| **P6 L3 分片接入** | 多进程 + shardK/shardN + CI 矩阵,每进程内 L1 | Helix queues | CI 矩阵绿 + 用时下降 |
| **P7 替换 gate golden stage** | 对账等价后 gate 用 embed 跑 golden;host runner 退役/降为对照 | —— | 全绿 + 用时不劣化 |
| **P8 产品化(可选)** | `z42 publish --rid` 面向用户;文档 + 样例 | dotnet workloads | 用户能发布跨平台 app |

> **纪律**(和 .NET 一样保守):P1–P6 embed 与 host runner **并存对账**,逐例等价后(P7)才替换,绝不先删。
> **性能**:每例重载 stdlib(= .NET 每 assembly 独立进程的成本),先求正确;"跨 context 只读共享 stdlib
> 代码、静态字段各自新建"是 P2 后优化(省大头,heap 隔离规则下谨慎设计)。

---

## G. 与 .NET 的差异(z42 特有,不照搬)

1. **z42 无 AppDomain 包袱**:一开始就是"fresh VmContext = 隔离",比 .NET 从 AppDomain 迁到进程更干净。
2. **单一 z42 字节码 agent 全平台复用**:.NET 各 flavor 仍是同 IL,但 z42 的 agent 是**一份字节码**,
   连 harness 编排也同一份 —— 比 .NET 的"每平台 XHarness 适配 + 各 runtime 打包"更统一。
3. **z42b/workload 一条链**:.NET 的 workload/publish 与测试 harness(XHarness)是两套工具;z42 让
   test-host 直接经 `z42b publish` 出,构建与产品发布**同一条链**(§B),更省。

---

## H. 待 User 裁决

1. **架构**:按 §A–§D(app 内嵌 agent + XHarness 式 driver + 三层自声明并行 + workload publish)推进?
2. **自声明**:用例 manifest 加 `isolation`/`parallel` trait(照 xunit/RemoteExecutor),harness 不猜 —— 认可?
3. **workload 深度**:P4 迁构建 + P8 产品化一起排,还是 P8 先不做?
4. **语义边界**:抛异常 golden / stderr —— embed 完全对齐 host(捕获 stderr + 异常入比对),还是留 host?
