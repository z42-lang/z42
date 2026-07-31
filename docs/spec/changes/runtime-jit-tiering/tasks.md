# Tasks: 运行时 JIT/interp 分层执行 + IR 回收

> 状态：🟡 规划完成,待 User 确认范围/阈值 | 创建：2026-07-30 | 类型：vm

## 进度概览
- [ ] Phase 1: 阈值分层 + 三态负缓存（时间开销）
- [ ] Phase 1.5: 混合模式（interp 感知 JIT）
- [ ] Phase 2: IR 回收 + 池化（内存开销）

## Phase 1a: 阈值分层（jit_call）+ 三态负缓存 —— 本次
- [x] 1.1 `JitModuleCtx.call_counts: Vec<AtomicU32>`（setup 预分配 merged_len,零 per-call 分配）
- [x] 1.2 三态槽：`FnEntry::rejected()`（null ptr）+ resolve 稳态读判定（两路径通用负缓存）
- [x] 1.3 `resolve_fn_by_id_tiered`（阈值）仅 jit_call 用;`resolve_fn_by_id`（非 tiered）其余调用点不变
- [x] 1.4 `Z42_JIT_THRESHOLD` 配置（env,默认 1000,clamp≥1;N=1=现状）—— 默认取高：只有真正热函数才编译，冷长尾留 interp
- [x] 1.5 收窄实证：全冷→None 使 vcall/indirect/objnew 兜底不健壮(86 fail)→ 只 jit_call tiered
- [ ] 1.6 GREEN：e2e(interp+jit) + cross-zpkg + stdlib + 自举 + vscode-syntax
- [ ] 1.7 Rust 单测：三态(Compiled/Rejected/Unknown) + 阈值(冷 interp / 热编译) + 结果一致
- [ ] 1.8 基准：多冷静态函数程序编译时间↓ + 热循环不回归
- [ ] 1.9 文档：jit-lazy-compile.md 补分层节

## Phase 1b: 扩展分层到 vcall/closure/ctor —— 已完成
- [x] `jit_vcall`：PIC(68) + vtable(262) 两 resolve site 切 tiered；vtable None-臂已有健壮 interp 兜底
      （receiver + args）→ 冷方法留 interp。profile 实证：冷方法 Bar 留 interp、热方法 Baz 编译。
- [x] `jit_call_indirect`（闭包）：None-臂原本只报错(无兜底) → 补 interp 兜底（env 前置 + args），切 tiered。
- [x] `jit_obj_new`（构造）：None-臂原本**静默跳过 ctor**（字段未初始化,is_pattern_binding 5→0）→ 补 interp
      兜底（interp 跑 ctor 原地改 `this`=obj_val），切 tiered。
- [x] 新增 `resolve_fn_by_name_tiered`；三 helper 各自 interp 兜底对任意冷 callee 健壮。
- [x] GREEN：e2e 424/0（含 delegate/class/closure）+ 全门禁。
- 备注：入口/热方法从 **interp 帧**调用仍不回跳 JIT（interp 的 Call 不感知 JIT）——真正混合模式是 Phase 1.5，
  Phase 1b 只让「从 JIT'd 码发起的」方法/闭包/构造调用分层。

## Phase 1.5: 混合模式（依赖 Phase 1）
- [ ] 1.5.1 interp Call/VCall 分发查 FnEntry：Compiled → 原生
- [ ] 1.5.2 保证已编译函数永不被 interp 执行（Phase 2 前提）
- [ ] 1.5.3 测试：interp 帧调已编译函数走原生 + GREEN

## Phase 2: IR 回收 + 池化（依赖 Phase 1.5）
- [ ] 2.1 `Function.blocks` 所有权粒度（可单独释放容器）
- [ ] 2.2 回收触发：Compiled + 不被 interp 执行 → drop blocks（留 metadata）
- [ ] 2.3 free-list 池（回收容器复用,不还 OS）；镜像 REGS_POOL/FRAME_POOL
- [ ] 2.4 测试：回收后内存↓ + 池命中(不抖 OS) + GC-stress + GREEN

## 备注
- 单向分层,无 deopt（Decision 7）。
- 准则 2 第 4 条（池化不抖 OS）贯穿 Phase 2。
- Phase 1 不改 interp 模式行为;只改 jit 模式 callee 分发。
