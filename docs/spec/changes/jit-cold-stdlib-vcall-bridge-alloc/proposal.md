# Proposal: 修 JIT 调冷 stdlib 方法的 bridge 分配开销（dict/generic JIT 6× 慢）

> 状态：🟡 已诊断、未实现（存档待接手，2026-08-04）| 类型：vm（JIT 执行性能）

## Why
实测：**JIT 模式跑 dict/字符串密集代码比 interp 慢 ~6×**（3M dict 查找：interp ~6s / jit ~36s；
CI `08_dict_heavy` speedup 0.47× 是同一现象的轻量版）。这类"JIT'd 热函数频繁调用冷 stdlib 方法"
的模式在真实代码里很常见（任何用 Dictionary / List / string 操作的热循环），是明确的大杠杆。

## 根因（已定位，勿再走弯路）

**不是** `jit_vcall` 的 name-resolution 慢。（本轮试了 2 次：IC 命中但 `resolve_fn_by_id_tiered`→None 时，
按缓存的 `fn_idx` 直接 interp（含 lazy-id 经 slot 名解析），跳过 4×`format!` 候选名 + 12 次 map 探测。
加诊断确认**我的新路径确实被执行**（ContainsKey 命中），**但性能纹丝不动** → name-resolution 不是瓶颈。）

**真因 = JIT→interp bridge 本身的每次调用分配**。JIT'd caller 每调一个冷 stdlib 方法（dict 的 Set/Get/
ContainsKey、string 的 ToString/`+` 都是 vcall），`jit_vcall` 的 interp 分支要 per-call：
- 分配 `call_args: Vec<Value>`（this + args）；
- 建 `JitFrame`（regs 虽池化，仍有 setup）；
- `push_frame` 一个 `VmFrame`（含 Arc clone）+ pop。

profile：`z42::jit::run` 伞下 **~900 malloc/free 样本主导** + fmt/clone/hash。interp 侧 `exec_function`
用池化帧 + 更直接的路径，per-call 分配少得多 → 6× 差距。**这是固有的 bridge 开销**：JIT 加速不了
builtin/stdlib-dominated 的工作，反而给每次跨界调用加了 wrapper 分配。

## What Changes（候选方向，未定）

- **方向 A：减 bridge 分配**。让 `jit_vcall`（+ `jit_call`）的 interp 分支复用池化的 arg 缓冲 / 免 `Vec`
  collect / 精简 VmFrame push。需设计（借用/生命周期 + 与现有 IC/tiering 交互），改动面中等、风险中。
- **方向 B：tiering 启发式**。识别"调冷方法密集"的函数（大量 vcall 到未 JIT 的 stdlib），**不 JIT 编译**
  它们（保持 interp），避免 bridge 开销。需在 tiering 判据里加"冷调用密度"信号。
- 二者可组合。先 A 量测能收回多少，不够再上 B。

## 如何接手（工具链已就绪）

1. **隔离工具链**（本 worktree 已建，别用 z42-test 共享 FLAT——会被别的任务的格式 bump 打架）：
   `Z42_HOME=<z42-test>/.z42 <z42-test>/xtask build compiler && ... build stdlib` →
   `artifacts/build/libraries/dist/release`（24 zpkg）+ `.../z42c.driver/release/dist/z42c.driver.zpkg`。
2. **复现 bench**：`Dictionary<string,int>` 的 3M 次 `ContainsKey`+`Get`（见 memory [[perf-alloc-and-array-levers]]
   附的 dh2.z42 形态），`--mode interp` vs `--mode jit` 对比。
3. **profile**：`sample <pid> <秒> -file`（backtick 用 python 解析）；关注 `jit_vcall` interp 分支的分配
   + `z42::jit::run` 伞下的 malloc。
4. 相关代码：`src/runtime/src/jit/helpers/vcall.rs`（`jit_vcall`）、`src/runtime/src/jit/frame.rs`
   （`JitFrame`/池）、`interp/exec_vcall.rs`（对照 interp 的省分配路径）。

## Out of Scope
- name-resolution 优化（已证伪非瓶颈）。

## 关联
- 本轮 perf 串（已合并）：packed 数组、int/char JIT 内联、interp 数组去 clone、对象分配去类名 clone、
  **帧名预计算 2.5×**（PR #107/#109/#110/#111/#113/#114）。见 memory [[perf-alloc-and-array-levers]]。
