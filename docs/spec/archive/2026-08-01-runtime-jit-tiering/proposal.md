# Proposal: 运行时 JIT/interp 分层执行 + IR 回收（准则 2）

状态：🟡 规划（2026-07-30，全量规划 Phase 1+2；待 User 确认后按阶段实施）
类型：**vm**（执行分发 / JIT 缓存 / 内存生命周期）→ 完整流程
子系统：`runtime`

## Why

book [optimization-pipeline](../../../book/src/runtime/optimization-pipeline.md) **准则 2**:运行时优化必须控制内存与
时间开销;分层升级后旧层内存要能回收;**回收要池化,不得频繁向 OS 申请/归还**。当前 VM 违反三点:

1. **模式全程序定死**(`--mode interp`/`jit`),无热度分层。jit 模式**每个被调函数首 call 就编** —— 冷函数
   (只调 1 次)也付编译代价(编译时间 > 解释 1 次)。**违反"只有热函数值得升级"**。
2. **不可 JIT 的函数每次调用重扫** `jit_unsupported_reason`(整函数指令走一遍)—— 空 `OnceLock` 槽无法区分
   "没编"与"编不了"。**纯浪费的每-call 时间开销**。
3. **IR + 原生码并存,永不回收**(`Function` 恒驻留 `Arc<Module>`)。**违反"旧层内存要能回收"**。

## What Changes（分三阶段,依赖递进）

### Phase 1 —— 阈值分层 + 三态负缓存（时间开销;本次主体）
- **每函数调用计数**(side table `Vec<AtomicU32>` 按 func id,预分配,零 per-call 堆分配)。
- jit 模式**不再首 call 就编**:计数 < 阈值 → 走 interp 执行(复用 `cross_zpkg_via_interp`);到阈值 → 编译缓存;
  之后原生。**冷函数永不编**(省编译时间 + code 页)。阈值可配(`Z42_JIT_THRESHOLD`,默认待基准定)。
- **三态槽**:`FnEntry.ptr == null` = Rejected(编不了,记负缓存,走 interp 不再重扫);`ptr != null` = Compiled;
  `OnceLock` 空 = Unknown(未到阈值)。保持 lock-free 稳态读。

### Phase 1.5 —— 混合模式（interp 感知 JIT;解锁 Phase 2 + 消局限）
- 让 **interp 的 `Call`/`VCall` 分发能路由到已编译原生码**(检查 id 的 `FnEntry`:Compiled → 调原生;否则 interp)。
- 效果:①消除 Phase 1"interp 子树粘滞"(热函数从 interp 帧也能进 JIT);②**保证已编译函数永不被 interp 执行**
  —— 这是 Phase 2 回收 IR 的安全前提。

### Phase 2 —— IR 回收 + 池化（内存开销;依赖 Phase 1.5）
- 函数编译完 + Phase 1.5 保证其永不被 interp 执行 → 回收 `Function.blocks`(指令体,内存大头;保留
  `exception_table`/`line_table`/`reg_types` —— 栈迹/catch/frame 仍需)。
- **所有权粒度**:`blocks` 改为可单独释放(如 `Mutex<Option<Vec<BasicBlock>>>` 或等价),脱离"整个
  `Vec<Function>` 一个 `Arc`"的粗粒度。
- **池化(准则 2 第 4 条)**:回收的 `blocks`/`BasicBlock`/`Vec` 进**线程/模块级 free-list 复用**(下次加载或
  升级重用容量),**不回收即还 OS**。镜像现有 `REGS_POOL`/`FRAME_POOL`。JIT code 页批量申请。

## Scope（允许改动的文件）

Phase 1（本次先实施）:
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/jit/frame.rs` | MODIFY | 调用计数 side table + 三态槽(null-ptr=Rejected) + resolve 路径改阈值判定 |
| `src/runtime/src/jit/helpers/call.rs` | MODIFY | jit_call:计数 → 阈值前 interp / 阈值后编译 / Rejected 走 interp |
| `src/runtime/src/jit/lazy.rs` | MODIFY | compile 触发挂到阈值;负缓存写入 |
| `src/runtime/src/jit/translate.rs` | 只读 | `jit_unsupported_reason`(阈值时判 Rejected) |
| `src/runtime/src/host/config.rs` | MODIFY | `Z42_JIT_THRESHOLD` 配置 |
| `src/runtime/src/jit/*_tests.rs` | NEW | 冷→interp / 到阈值→编译 / Rejected 负缓存 / 结果一致 |
| `docs/book/src/runtime/jit-lazy-compile.md` | MODIFY | 分层机制补节 |

Phase 1.5 + 2（本次仅规划,实施单列）:`src/runtime/src/interp/exec_call.rs`/`exec_vcall.rs`（interp→JIT 路由）、
`src/runtime/src/metadata/bytecode.rs`（`Function.blocks` 所有权粒度 + 回收）、新 free-list 池模块。

## Out of Scope
- deopt（JIT→interp 回退重执行）—— 本设计只 interp→JIT 单向,不做逆向 deopt。
- AOT。
- 改编译期 IR 优化管线（正交,已独立落地）。

## Open Questions（需 User 裁决）
- [ ] **阈值默认值**:N=?(N=1=现状无分层;N=2/10/100 滤冷函数)。建议可配 + 基准定默认。
- [ ] **Phase 1.5/2 是否本轮实施**:还是 Phase 1 落地稳定后单开?(Phase 2 依赖 Phase 1.5,均较重)
- [ ] cold-tier 复用 `cross_zpkg_via_interp` 的通用性:需验证它能正确跑任意(非 cross-zpkg)函数(args/返回/异常/receiver)。
