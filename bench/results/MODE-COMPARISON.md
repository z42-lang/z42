# interp vs jit 基线对比（perf-vm-iteration Phase 0）

> 生成：`bench/scripts/compare-modes.sh`（hyperfine, 8 runs, warmup 2）
> 机器：darwin-arm64（M 系列）；z42vm release build。
> 数据：`bench/results/mode-comparison.json`（本地 regen 产物，`bench/results/*.json` 被 gitignore，
> 与 `e2e.json` 同约定；下方表格已内嵌全部数字）。这是**首次**存在的 interp/jit 端到端对比
> —— 此前 `xtask bench` 从不 sweep 模式,1.5× 之类数字仅以源码注释手记于 `scenarios/04`。

## 原始耗时（含启动）

| 场景 | interp(ms) | jit(ms) | jit× |
|------|-----------:|--------:|-----:|
| 01_fibonacci（Fib 25，call-heavy） | 163.5 | 79.3 | 2.06× |
| 02_math_loop（100k 整数循环） | 58.0 | 60.0 | 0.97× |
| 03_startup（VM+stdlib 加载基线） | 46.2 | 54.3 | 0.85× |
| 04_c2_p1_arith_loop（10M i64 紧循环） | 688.6 | 104.7 | 6.58× |
| 05_polymorphic_dispatch（10M 四路虚派发） | 4716.6 | 1078.7 | 4.37× |

## 扣除启动后的**计算耗时**与**单操作成本**（关键）

启动基线：interp 46.2ms / jit 54.3ms。减去后：

| 场景 | 操作数 | interp 计算 | interp/op | jit 计算 | jit/op | 计算 jit× |
|------|-------:|-----------:|----------:|---------:|-------:|---------:|
| Fib 25 | ~242,785 calls | 117.3ms | **~483ns/call** | 25.0ms | **~103ns/call** | 4.7× |
| math_loop | 100k iters | 11.8ms | ~118ns/iter | 5.7ms | ~57ns/iter | 2.1× |
| arith_loop | 10M iters | 642.4ms | **~64ns/iter** | 50.4ms | **~5ns/iter** | 12.7× |
| poly_dispatch | 10M vcalls | 4670.4ms | **~467ns/vcall** | 1024.4ms | **~102ns/vcall** | 4.6× |

## 结论:数据坐实三个根因,并给出优先级

1. **调用/虚派发是最贵的操作,两个引擎都慢**
   - interp ~467–483ns/(v)call,jit ~102ns/(v)call。这就是根因 B(每调用 3 把共享调用栈锁 +
     regs/args 两次 Vec 分配 + 两次 Arc 帧名分配)+ 根因 A(poly 对象字段访问的 per-object mutex)。
   - **JIT 也逃不掉**:`jit_call` 每次仍 `push_frame/pop_frame` + 分配全新 `JitFrame`。
   - → **Phase 1(调用路径去锁去分配)+ Phase 2(去 per-object 锁)是"一份工作、两个引擎同时提速"
     的最高杠杆**,应排在 JIT 专项之前。`05_polymorphic_dispatch` 是它们的天然回归靶。

2. **纯算术:JIT 已很好(5ns/iter),解释器差(64ns/iter)**
   - arith_loop jit 12.7×,证明 Cranelift 算术特化有效。解释器 64ns/iter 的开销来自逐指令
     `site_idx` 解析 + safepoint + `frame.get` 的 Result/clone(根因 C 解释器侧 / Phase 3)。
   - 算术密集代码在真实负载中较少,故 Phase 3 优先级低于 1/2。

3. **短程序 JIT 反而更慢(math_loop 0.97×、startup 0.85×)**
   - JIT 冷编译成本 ~8ms 未被小循环收益抵消,且无分层(single unsupported op → 整函数退回 interp)。
   - → 印证 Phase 4「`opt_level` 一行 + safepoint 内联 + 分层/OSR」的必要性;`opt_level=speed`
     与惰性编译收益需要一并测。

## 复现

```bash
cargo build --manifest-path src/runtime/Cargo.toml --release
bench/scripts/compare-modes.sh          # 用 release z42vm + .z42/libs
```
