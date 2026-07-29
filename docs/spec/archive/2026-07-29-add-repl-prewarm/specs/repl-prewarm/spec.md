# Spec: REPL 后台预热

## ADDED Requirements

### Requirement: 启动即后台预热依赖世界

REPL 交互模式启动时，在显示提示符前 spawn 一个后台 worker 线程构建依赖世界（`DepScan.ScanDirsLazy`
+ 加载默认 using 包），主线程立即进入行读取等待用户输入。

#### Scenario: 提示符即时出现，预热并行
- **WHEN** 用户 `z42 repl` 启动交互模式
- **THEN** 提示符 `>>> ` 在 ~启动耗时（≈120ms）内出现，不等待依赖世界构建
- **AND** 一个后台 worker 线程已开始跑 `Script.Prewarm`

#### Scenario: 打字间隙盖住预热，首次 eval 近瞬时
- **WHEN** 用户在提示符出现后花 ≥ 预热时长（真实 PTY 下打字数秒）敲第一行并回车
- **THEN** 首次 `Eval` 消费依赖世界前 `Join` worker 瞬回，求值近瞬时（不再干等 ~3.4s）

#### Scenario: 打字极快时退化为同步、不更差
- **WHEN** 首行在 worker 完成前就回车（如 piped 输入无间隙）
- **THEN** `Eval` 的 `Join` 阻塞到 worker 完成，结果正确；总耗时不劣于旧同步路径

### Requirement: 预热结果与旧同步路径等价

后台预热构建的 `CachedScan` 与旧「首轮 `Eval` 内同步构建」在语义上等价——同一会话首行及后续行的求值
结果、错误信息、补全候选均不因预热而改变。

#### Scenario: 首行表达式结果正确
- **WHEN** 预热已完成，用户输入 `1+2`
- **THEN** 输出 `3`，与非预热路径 byte-for-byte 一致

#### Scenario: `.reset` / `-c` 走 inline 兜底
- **WHEN** 会话 `.reset`（重建 `ScriptState`，无 worker 句柄）或 `-c "expr"` 单次求值
- **THEN** `Eval` 走 inline `Prewarm` 兜底，行为与今天一致

### Requirement: 阻塞原生调用期间 GC-safe，后台预热不死锁

主线程阻塞在原生行读取期间，对 GC 收集器表现为「已 park、z42 根冻结」，使后台 worker 触发的 GC 能
推进而不死等主线程。

#### Scenario: worker 预热触发 GC 不卡死
- **WHEN** worker 跑 `Prewarm`（编译器管线大量分配）触发一次 GC，而主线程正阻塞在 `ReadBlock`
- **THEN** GC 正常完成，worker 继续预热直至发布 `CachedScan`；主线程 readline 不受影响
- **AND** 用户回车后 `Eval` 正常求值

#### Scenario: 预热窗内 Tab 补全不死锁
- **WHEN** worker 预热进行中，用户在提示符按 Tab 触发补全器重入 VM
- **THEN** 补全器作正常 mutator 参与 safepoint（`exit → 跑 → enter`），返回候选（此刻仅会话变量/
  声明名，`CachedScan` 尚 null → 无导入名），不与 worker 死锁

### Requirement: handoff 无锁、并发读安全

worker 全程操作本地 `DepScanResult`，仅最后一步原子发布到 `state.CachedScan`；并发读该字段的补全器
只见 null 或完全成品，不见半构造态。

#### Scenario: 补全器并发读只见 null-or-complete
- **WHEN** 预热进行中，补全器读 `state.CachedScan`
- **THEN** 读到 `null`（`_addImportedNames` 既有 null 分支返回空）或完全构建好的 scan，绝不读到
  正被 `EnsurePackageLoaded` 变异的中间态

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更。复用既有 `__thread_spawn` / `__thread_join` builtin 与
`DepScan` / `PackageCompile` 管线。

## Pipeline Steps

- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [x] VM interp / runtime — `safepoint.rs` native-park 原语 + `repl.rs` 接线
- [x] toolchain（z42.scripting / interactive）— `Prewarm` + spawn + Eval 汇合
