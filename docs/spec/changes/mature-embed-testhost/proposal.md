# 提案:嵌入式 test-host 成熟化 —— goldens 进 embed · workload 构建 · 分层并行 · 全平台

> 状态:**DRAFT**(需 User 确认方向后进 IMPL)。对齐 2026-08-05。
> 前置:G6 嵌入式 test-host 已交付(desktop/wasm/iOS/Android,`--rid`,CI 折叠,stdlib 单元
> 283 全覆盖;PR #96)。本提案把它从"跑 stdlib 单元"成熟为"跑全部可运行语料 + 经 workload 构建 +
> 分层并行 + 全平台"的完整测试地基,并顺势把 workload 补成**用户可用**的跨平台发布系统。

---

## 0. 四条主线(User 命题)

1. **golden test 能否进 embed** —— 需要 per-case 隔离,之前判为阻塞;可行性已重新验证(§1)。
2. **借此完善 workload** —— embed host 都经 workload 构建,进而**提供给用户**发布自己的跨平台 app(§2)。
3. **并行怎么解** —— 多数用例不需进程隔离,只有少数 IPC 类需要;分层:进程内并行 + 少数多实例 +
   现有多进程分片(§3)。
4. **移动端 / web 怎么并行** —— 平台自适应:mobile 线程、web Web Workers(§4)。

---

## 1. Golden 进 embed —— 可行,靠"每例全新 VmContext"隔离

### 1.1 可行性(已验证,附依据)

| 关注点 | 结论 | 依据(file) |
|--------|------|------------|
| 每 golden 隔离 | ✅ `app::run` 每次调用建**全新 `VmCore`/heap/静态字段**,零进程级泄漏 | `app.rs` `VmContext::with_module`;静态字段在 `VmCore`(`vm_context.rs`) |
| VM 内再起隔离 VM | ✅ 无"当前 VM"全局/TLS;builtin 显式收 `&VmContext`;先例 `__load_bytecode_in_memory` | `reflection.rs` |
| 多线程并行 | ✅ heap per-`VmCore`,**无全局 GC 锁**;`VmContext`/`Value` 是 `Send+Sync` | `arc_heap.rs`;`send_sync.rs` |
| 并行 stdout 捕获不串 | ✅ 捕获栈**线程本地**(`STDOUT_SINKS`) | `io.rs` |

> **铁律**:各 golden 的 heap **互不相通**——绝不跨 context 传 `Value`/`GcRef`(非原子 `Rc`)。
> 只把捕获到的 **stdout 字符串**跨出来。

### 1.2 机制

新增 Rust builtin(agent 当薄编排,全平台同一份 z42 字节码):

```
__run_goldens_isolated(paths[], entries[], jobs) -> captured[]
  内部:每个 golden → push 线程本地 stdout 捕获 → 全新 VmContext 跑 entry → take 捕获 → String
       (jobs>1 时分配到 Rust 线程池;各 context heap 互斥、捕获线程本地 → 真并行真隔离)
agent: outs = Runtime.runGoldensIsolated(paths, entries, jobs)
       逐个 outs[i].stripNl == expected[i].stripNl ? passed : failed
```

语料来源复用 `_enumerateCasesF`(现有 golden 枚举,**零漂移**)→ 喂 builtin。

### 1.3 能进 vs 不能进(语义边界,必须对齐 host)

| 语料 | 进 embed? | 说明 |
|------|:---:|------|
| 可运行单 module goldens(`src/tests/<cat>/<name>[/source].z42` + 19 个 stdlib dir-golden) | ✅ | 全新 context 跑 Main、比 stdout |
| 编译失败 golden | ❌ | 期望编不过;embed 只跑编得过的 → 留 host |
| byte-format(zbc/zpkg) | ❌ | 比产物字节,不"跑" |
| exit-code fixture(`harness=false`) | ❌ | 断言退出码,`[Test]` 表达不了 |
| cross-zpkg / multi-exe | ❌ | 多包/多产物,非单 module |

**语义对齐点**(逐一处理):① host 比 **stdout+stderr** 合并、只 strip 尾换行、不查 exit;`captureStdout`
只抓 stdout → 需决定是否也捕获 stderr。② 抛异常的 golden:host 当文本 diff,embed 当 fail → 需捕获
异常信息并入比对,或标注这类 golden。③ `interp_only` 标记:builtin 跑 interp 即安全(需能选 mode)。

---

## 2. 经 workload 构建 test-host —— 顺势把 workload 做成用户可用

### 2.1 现状 vs 目标

- **现状**:`_buildWasmTesthost`/`_buildIosTesthost`/`_buildAndroidTesthost` 在 xtask 里**ad-hoc**
  拼(直接调 cargo / wasm-pack / xcframework + 手工装配资产)。与"用户发布跨平台 app"是两套。
- **目标**:test-host = **workload 构建出的一个 app**(test-agent app.zpkg + 平台 shell)。经
  `z42b publish --rid <rid>` + workload manifest 产出,和用户发布自己 app **同一条路径**。

### 2.2 收益

1. **一条构建路径**:test-host 与用户 app 共用 workload,消除 xtask 里的 ad-hoc 重复。
2. **完善 WorkloadBase 5 相位**(构建 / 资产 / 打包 / 部署 / 运行):test-host 是**第一个真实消费者**,
   逼着把 5 相位补齐、验证到位。
3. **用户可用**:补齐后,`z42 publish --rid ios-arm64`(等)即用户构建跨平台 app 的入口——test-host
   只是其中一个 app。测试地基顺带产出产品能力。

### 2.3 落地要点(需 workload 现状审计)

- 把 embed host 的"平台 shell + 资产装配 + 链接嵌入 VM"表达为 **workload manifest**
  (`[platform.<rid>] link=static|dynamic`、assets、entry),交 z42b/workload 执行。
- test-agent bundle(§1 语料 + stdlib)作 workload 的 **asset 阶段**产物。
- 现有 `appbuilder/`(iOSWorkload.z42 / WasmWorkload.z42 / …)+ z42b build-hook 是起点;需审计
  WorkloadBase 5 相位缺口(独立子任务)。

---

## 3. 并行模型 —— 三层,按需选隔离粒度

> User 洞察:**多数用例不需要进程隔离**;只有少数**进程间通信 / 全局 OS 资源**才需要独立进程。

| 层 | 隔离粒度 | 适用 | 机制 |
|----|---------|------|------|
| **L1 进程内并行**(默认) | 全新 `VmContext`(heap/静态字段各自) | **绝大多数用例** | Rust 线程池,`jobs` 上限;§1 的 builtin 就是它 |
| **L2 多实例** | 独立进程 | **少数 IPC**(俩程序 socket/pipe/信号 通信)、依赖进程级全局的 | 起 2+ embed 实例(host 各跑一份),它们之间真跨进程 |
| **L3 多进程分片**(现有) | 独立进程 | 加机器/加核吞吐、CI 跨机 | spawn 几个进程 + `shardK/shardN`;**与 L1 组合**(每进程内部再 L1 并行) |

- **默认走 L1**:VmContext 隔离已等价 host 每例子进程的语义(静态/heap 干净),够绝大多数 golden。
- **L2 按需**:用例声明"需要独立进程 / IPC"(manifest 标记)→ 该用例走多实例,不进 L1 池。
- **L3 叠加**:desktop/CI 仍可多起进程(现有 `jobs`/分片),每进程内 L1 并行 → 二维并行。

隔离粒度 = **能省则省**:L1(context)覆盖多数;L2(进程)只留给真需要跨进程的少数;L3 是吞吐叠加。

---

## 4. 移动端 / web 并行 —— 平台自适应

| 平台 | L1 进程内线程 | L2/L3 多进程 | 并行策略 |
|------|:---:|:---:|---------|
| **desktop** | ✅ 多核线程 | ✅ spawn 进程 | L1(线程)+ L3(多进程)+ L2(IPC 实例)全可用 |
| **iOS/Android** | ✅ 多核线程 | ⚠️ 受限(iOS 禁任意 spawn;Android 受限) | **主力 L1 线程**;L2/L3 基本不用;`jobs`=设备核数 |
| **web/wasm** | ❌ 单线程(wasm-threads+SharedArrayBuffer 复杂且受限,暂不依赖) | —— | 并行 = **Web Workers**:每 worker = 独立 wasm isolate(≈L2/L3),worker 内顺序;shard 到 N workers |

- **agent/host 按平台选策略**:desktop 用 Rust 线程池;wasm 的"并行"在 **JS 侧起 N 个 Web Worker**,
  每 worker 一个 wasm 实例跑一个 shard(harness 层,不改 agent)。mobile 用线程池、`jobs`=核数。
- **wasm 特例**:§1 builtin 的"Rust 线程池"在 wasm32 上退化为 `jobs=1`(单线程),真并行交给
  Web Workers 分片(harness `run.js` 起多个 worker,各 fetch 一个 manifest 分片)。

---

## 5. 分阶段落地(gate 不中途红)

| 阶段 | 内容 | 验证门 |
|------|------|--------|
| **P1** golden 进 embed(隔离,jobs=1) | builtin `__run_goldens_isolated` + agent 编排 + 语义对齐 + 接 `_enumerateCasesF` | embed 全 golden **与 host golden 并跑对账**,pass/fail 集逐例相等 |
| **P2** L1 并行 | Rust 线程池 `jobs`;heap 互斥不变量守住 | 并行 vs 顺序结果一致 + 无数据竞争(TSan/压测) |
| **P3** 经 workload 构建 test-host | 把 host 构建迁到 z42b publish + workload;补 WorkloadBase 相位缺口 | 四平台经 workload 产出的 test-host 跑通(= 现有 CI 折叠 job) |
| **P4** 平台并行 + L2 | wasm Web Workers 分片;mobile 线程池;IPC 用例走 L2 多实例 | 各平台并行 RUN 绿(CI);IPC 样例验证 |
| **P5** 替换 gate golden stage | embed golden 与 host 对账等价后,gate 用 embed 跑 golden(host runner 退役或降为对照) | 全绿 + 用时不劣化 |

> **纪律**:P1–P2 期间 embed golden **与 host runner 并存对账**,证明逐例等价后(P5)才替换,
> 绝不先删 host。性能:每 golden 重载 stdlib(与 host 子进程同成本);"跨 context 只读共享 stdlib
> 代码、静态字段各自新建"是 P2 后的优化项(省大头,但 heap 隔离规则下需谨慎设计)。

---

## 6. 待 User 裁决

1. **方向**:按本提案(golden 进 embed via 隔离 builtin + workload 统一构建 + 三层并行 + 平台自适应)推进?
2. **并行粒度**:L1 默认(Rust 线程池)+ L2 按需 + L3 叠加 —— 认可这个分层?
3. **workload 深度**:P3 要顺带把 WorkloadBase 5 相位补到"用户可 `z42 publish` 发布 app",还是先只把
   test-host 迁过去、用户可用留后续?
4. **语义边界**:抛异常 golden / stderr 捕获 —— 要 embed 完全对齐 host(捕获 stderr + 异常入比对),
   还是这类少数留 host golden runner?
