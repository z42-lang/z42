# Design: REPL 后台预热依赖世界

## Architecture

```
interactive_main.Main()  [主线程]                 worker  [Std.Threading.Thread]
  s = Script.Create()          ── spawn ──▶   Script.Prewarm(s):
  s.PrewarmThread = spawn(...)                    scan = DepScan.ScanDirsLazy(...)   ~1.4s
  Repl.SetCompleter(...)                          for u in Usings:                    ~0.4s
  loop:                                              DepScan.EnsurePackageLoaded(scan,u)
    Completer.SetActive(s)                        state.CachedScan = scan   ◀── 原子发布
    line = Repl.ReadBlock(">>> ")  ← 主线程阻塞在原生 readline（打字间隙）
        └─[VM] enter_native_parked → readline → exit_native_parked
              └─(Tab)→ complete_via_callback: exit_park → replComplete → enter_park
    r = Script.Eval(s, line)
        └─ _ensureWarm(s): s.PrewarmThread.Join()  ← 消费前汇合（已完成则瞬回）
```

**一句话**：预热与「用户打字」并行；主线程阻塞在 readline 期间对 GC 表现为「已 park、根冻结」，
worker 独占 z42 执行跑编译器管线，末尾把成品 `DepScanResult` 一次性发布给会话；首次 `Eval` 消费前
`Join` 汇合。

## Decisions

### Decision 1: 后台线程 vs 启动同步 vs 跨会话缓存

**问题**：3.4s 的 input-independent 预热放哪。
**选项**：
- A 后台线程预热：启动仍 118ms，预热盖在打字间隙 → 首次 eval 近瞬时。**需 VM 侧 GC-safe park**。
- B 启动同步预热：实现最简、无并发，但提示符要等 3.4s 才出——只是把等待从「回车后」挪到
  「出提示符前」，没用到打字间隙，UX 更差。
- C 跨会话磁盘缓存：warm 启动近瞬时，但首次-ever 仍付 3.4s，需处理 stdlib 变更失效——正交，另做。
**决定**：**A**。唯一能真正利用打字间隙的方案，User 已选定。C 作为后续正交叠加。

### Decision 2: 阻塞原生调用期间的 GC-safe park（本 change 的核心风险点）

**问题**：GC 停顿协议（[safepoint.rs](../../../../src/runtime/src/gc/safepoint.rs)）要求收集器等到
`parked_count >= vm_contexts.len() - 1`，即所有其它线程主动停到 safepoint。而线程只在**执行 z42
字节码**命中 `check_safepoint` 时才 park。主线程阻塞在原生 rustyline `readline` 里**永不命中**——
若 worker 触发 GC，收集器死等主线程 park（要到用户回车 readline 返回才可能），worker 在首次 GC 卡死。

**选项**：
- A 加「进入/离开阻塞原生调用」的 park 登记：进入 readline 前 `parked_count += 1` + notify；离开后
  按 `park_until_idle` 尾部等到安全相位（非 Requested/Marking）再 `-= 1`。等价 JVM 的
  `_thread_in_native` / Go 的 `entersyscall`——线程在原生调用期间根冻结、可被收集器安全扫描。
- B 让 GC 用不需要全员 park 的算法：现有 STW 与 concurrent 两条路都需要 mutator 在某点 park
  （concurrent 的 handshake 亦然），无法回避。
- C 预热不分配到触发 GC：编译器管线分配量大、不可控。

**决定**：**A**。这是本 change 不可约简的核心。实现为一对函数（或 RAII guard）：

```
// 进入阻塞原生调用：本 ctx 计入 parked，唤醒可能在等的收集器。此后 z42 根须冻结。
pub fn enter_native_parked(ctx) {
    ctx.core.parked_count.fetch_add(1, AcqRel);
    let phase = ctx.core.gc_phase.lock();
    ctx.core.gc_phase_cv.notify_all();
    drop(phase);
}
// 离开阻塞原生调用：若正 GC，先等到安全相位（避免解 park 后立即改根与收集器竞争），再解除计数。
pub fn exit_native_parked(ctx) {
    let mut phase = ctx.core.gc_phase.lock();
    while matches!(*phase, Requested | Marking) { ctx.core.gc_phase_cv.wait(&mut phase); }
    ctx.core.parked_count.fetch_sub(1, AcqRel);
    drop(phase);
}
```

> 与既有 `park_until_idle` 的关系：那是「命中 safepoint → 增计数 → 等 Idle → 减计数」的一体调用；
> native-park 把「增计数」（进入原生前）与「等安全相位+减计数」（离开原生后）**拆成两半**，中间是
> 阻塞的原生调用。语义一致、复用同一 `parked_count` + `gc_phase_cv`，无新同步原语。

### Decision 3: 补全器回调的 park 反转

**问题**：主线程 `enter_native_parked` 后阻塞在 readline；用户按 Tab，rustyline 在 readline 内**同步
回调**补全器（`complete_via_callback` → z42 `replComplete`）。此时主线程要**重入 VM 执行 z42**——但
它正处「native-parked、根冻结」态，与 worker 的 GC 假设冲突。

**决定**：补全器回调核心（`complete_via_callback`）在重入 VM 前 `exit_native_parked`（恢复为正常
running mutator，参与 safepoint、必要时随 worker 的 GC 一起 park），返回 rustyline 前 `enter_native_parked`。
于是「阻塞在 readline」= parked，「跑补全 z42」= running，两态清晰、收集器永不误判。

### Decision 4: handoff —— 本地构建 + 末尾原子发布，无锁

**问题**：worker 构建 `CachedScan` 期间，主线程的补全器可能并发读 `state.CachedScan`
（[Completer.z42 `_addImportedNames`](../../../../src/toolchain/scripting/src/Completer.z42) 遍历
`sc.Exported`）。若边建边发布，会读到半构造/正被 `EnsurePackageLoaded` 变异的 `Exported`。

**决定**：`Prewarm` **全程操作本地 `DepScanResult scan`**（`ScanDirsLazy` + 循环
`EnsurePackageLoaded(scan, u)`），**最后一步**才 `state.CachedScan = scan`（64 位指针写原子）。
补全器读 `state.CachedScan` 只见 **null（预热未完）或完全成品**——`_addImportedNames` 的
`if (sc == null) return;` 既有分支天然覆盖 null 情形（预热窗内 Tab 只补会话变量/声明名，此刻本就为
空，无损）。**无需锁、无需 gate flag**。首次 `Eval` 前 `Join` worker（内存屏障）→ 主线程必见成品。

其它被补全器读的会话字段（`VarNames`/`DeclNames`）在预热窗内**不被变异**（`Eval` 尚未跑，且首个
`Eval` 会先 `Join`）→ 无并发写。

### Decision 5: Eval 的汇合点与兜底

`Eval` 首轮 `if (state.CachedScan == null)` 改为 `_ensureWarm(state)`：

```
private static void _ensureWarm(ScriptState state) {
    if (state.CachedScan != null) { return; }
    if (state.PrewarmThread != null) { state.PrewarmThread.Join(); state.PrewarmThread = null; }
    if (state.CachedScan == null) { Script.Prewarm(state); }  // 无 worker（如 .reset/-c）或 worker 异常
}
```

- 正常：worker 已发布 → `Join` 瞬回，直接用。
- 打字极快（预热未完）：`Join` 阻塞到 worker 完成——退化为「与今天相同的一次同步预热」，不更差。
- `.reset` 重建 `s`（`PrewarmThread==null`）/ `-c` 单次求值：走 inline `Prewarm` 兜底，行为不变。

### Decision 6: `-c` 单次求值不预热

`Main()` 的 `-c "expr"` 分支求值后立即退出，无「用户打字」窗口 → 不 spawn，`Eval` 内 inline
`Prewarm` 兜底。避免为一次性求值多起一个线程。

## Implementation Notes

- **ScriptState → z42.threading 依赖**：`PrewarmThread` 字段类型 `Thread` 使 z42.scripting `using
  Std.Threading` 并加 toml 依赖。z42.threading 已在 SDK/种子中、且 REPL zpkg 闭包本就随 SDK 发布
  → 无 bootstrap 种子问题（z42.scripting 是 toolchain 层、非 z42c 自身，不踩 bootstrap-seed 轴 ④）。
- **闭包跨线程**：`Thread.Start(() => Script.Prewarm(s))` 捕获 `s` 须为堆 Closure；编译器对 thread
  spawn 已做堆提升（`threading.rs` 对栈闭包有显式报错），此处沿用。
- **native-park 包裹层级**：包在 `builtin_repl_readline` / `builtin_repl_readblock` **最外层**（整个
  多行块读取一次 park；块间的括号平衡判断是纯 Rust、不碰 z42 堆，parked 期间安全）。补全器回调是唯一
  嵌套重入点，单独反转。
- **wasm/plain-stdin 回退**：`plain_readline` 路径同样要 park（否则 piped 输入下 worker 也会卡）。
  在 builtin 入口统一 park，与具体行编辑器实现无关。

## Testing Strategy

- **回归（正确性）**：`tests/repl_prewarm/` 管道驱动一个含首行表达式的会话，断言结果正确（预热路径
  与旧同步路径 byte-for-byte 同结果），且进程正常退出（无死锁）。
- **并发压力（手动/CI）**：piped 会话首行前不给输入延迟，强制 `Eval` 的 `Join` 命中 worker 未完成
  分支；再跑一个「首行前 Tab」的场景（若 PTY 测试设施允许）验证补全器 park 反转不死锁。
- **无回归**：`xtask test`（e2e + cross-zpkg + stdlib + compiler + vscode-syntax）全绿；REPL 既有
  `repl_*` 测试（completion / decls-multiline / default-usings）不变。
- **GC 交互**：跑一次「首行前故意长等 + 大量默认 using」使 worker 触发至少一次 GC，确认不卡死
  （手动，或 CI 设小 GC 阈值的用例）。
- **实测**：`bench/repl/run.sh` 复测 0/1/5-eval；预期 1-eval 在有打字间隙的真实 PTY 下近 startup，
  piped（无间隙）下不劣于今天 3.5s。

## Deferred / Future Work

### repl-prewarm-future-cross-session-cache

- **来源**：本 change proposal Out of Scope；BASELINE future-work ④。
- **触发原因**：进程内预热解决「首次 eval 卡」，但每次**冷启动**仍付 3.4s 构建。
- **前置依赖**：`DepScanResult` 可序列化 + stdlib 世界指纹（失效判定）。
- **触发条件**：REPL 冷启动频繁成为体感瓶颈时。
- **当前 workaround**：无（本 change 已把体感转移到「后台并行」，冷启动仍需一次构建）。

### repl-prewarm-future-generic-native-park

- **来源**：本 change proposal Out of Scope。
- **触发原因**：native-park 目前只给 REPL readline 接线；文件/网络等阻塞 builtin 若与后台线程共存
  也需同款处理。
- **触发条件**：出现「主线程阻塞在某原生 I/O + 后台 z42 线程分配触发 GC」的新场景。
