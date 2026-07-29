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

## 探针结果:`opt_level=speed` 是负收益（2026-07-29，Phase 4）

初始分析假设 Cranelift 默认档偏低,设 `opt_level=speed` 是"零成本 JIT 加速"。**实测证伪**:

| | jit 默认档 | jit speed 档 |
|--|--------:|----------:|
| startup（冷编译） | 54.3ms | 58.9ms(+4.6) |
| arith 计算 | 50.4ms | 52.9ms |
| poly 计算 | 1024ms | 1028ms |

speed 档零计算提升,只多 ~4-5ms 冷编译。根因:JIT 的成本在 **opaque helper call +
每 op 的 24B `Value` load/store**,这两者 Cranelift 都无法跨 op 去箱消除。→ **真正的 JIT
杠杆是结构性去箱 + 内联 helper（Phase 4 剩余项）,不是调 opt 档。** 已保留默认档
（`src/runtime/src/jit/lazy.rs`）。这条负结果正是"先度量再改"的价值。

## Phase 1 结果:regs Vec 池化（2026-07-29，Decision 3，已落地）

`Frame::new` 每 call `vec![Null; n]` 一次 malloc+free。改为 per-thread free-list 复用
（`Drop for Frame` 归还已 clear 的 Vec；drop 序保证 VmFrame root 先被 pop,无 GC 耦合）。

| 场景（interp 计算，扣启动） | 池化前 | 池化后 | 提升 |
|--|------:|------:|-----:|
| Fib 25 | 117ms | 80ms | **−32%** |
| poly_dispatch 10M | 4670ms | 3804ms | **−19%** |
| arith_loop（单帧,无 call） | 646ms | ~680ms | 噪声（无 call → 池化不参与） |

JIT 列不变（用 JitFrame 非 Frame,池化 interp-only）。每 call 省一次 malloc/free 的收益
远超预期——poly 做 10M call,分配器往返是真实热点。GREEN 全绿 + 自举不动点 5/5。
> 注:这是 interp-only 改动;默认 JIT 模式的等价收益要等 Decision 1（call_stack 去锁）
> + jit_call 的 JitFrame 池化,均依赖 GC 并发裁决。

## Phase 1 结果:interp 直接填 callee frame（2026-07-29，Decision 3，已落地）

直接调用路径（`exec_call::call`）原本 `collect_args` 分配 args Vec + 参数双 clone
（caller reg→args Vec→callee reg）。新增 `exec_function_from_regs` / `Frame::new_from_regs`
直接从 caller regs+indices 填 callee 帧——零 args Vec、单 clone（镜像 JIT `new_args_from`）。

| Fib 25（interp 计算，扣启动） | baseline | +regs 池化 | +direct-fill |
|--|------:|------:|------:|
| | 117ms | 80ms | **68ms** |
| 累计 | — | −32% | **−42%** |

poly 不变（vcall 路径 prepend `this`,非纯 index-fill,未在本次 scope;留后续）。JIT 列不变。

## Phase 1 结果:vcall 接收者直接填帧（2026-07-29，Decision 3，已落地）

vcall 对象/基元 IC 命中的稳态热路径原本 `vec![obj_val]` + `collect_args` 两次 Vec 分配
+ 参数双 clone。`collect_args` 从无条件位置下移到冷路径（boxing / 基元名 / vtable 各自
局部 materialize）,IC 命中路径改 `exec_function_from_receiver_regs` / `new_from_receiver_regs`
直接填 regs[0]=receiver、regs[1+i]=caller args——零 Vec、单 clone。

| poly_dispatch 10M（interp 计算，扣启动） | baseline | +regs 池化 | +vcall 直接填 |
|--|------:|------:|------:|
| | 4670ms | 3804ms | **3247ms** |
| 累计 | — | −19% | **−30%** |

JIT 列不变（interp-only）。GREEN 全绿 + 自举 5/5。

## Phase 3 结果:块标签哈希消除（2026-07-29，剖析驱动，已落地）

macOS `sample` 剖析数组重循环发现 **~25% interp 时间在 SipHash**：每条 `Br`/`BrCond`
用块标签字符串查 `block_index: HashMap<String,usize>`（std 默认 SipHash），循环里每
迭代一次。修复：`loader::build_block_indices` 把分支标签**一次性解析成块索引**
（新 `Function.branch_targets: Vec<BranchTargets>`），interp 分支变直接整数跳转、零哈希
（未预解析时回退标签表，保留 undefined-block 错误路径）。

| interp 场景 | 修复前 | 修复后 | 提升 |
|------|------:|------:|-----:|
| 数组重循环 50M 读 | 4.49s | **2.21s** | **−51%** |
| arith 紧循环 10M | 688ms | **466ms** | **−32%** |
| poly_dispatch | 3247ms | 2902ms | −11% |

比剖析预测的 25% 更大（标签字符串存取一并消除）。**JIT 不变**（Cranelift 原生分支）。
锁是红鲱鱼、剖析一次即定位此 25% 热点——先剖析再动手的价值。GREEN 全绿 + 自举不动点。

## Phase 4 结果:JIT 单态 i64 数组读内联去箱（2026-07-29，系统级，`jit-inline-fastpaths` 分支）

针对「JIT 本质是穿线解释器、每操作跨 extern-C helper」的系统级瓶颈。上限 spike:同 50M 循环
`total += a[j]`（helper）834ms vs `total += j`（原生）252ms = 3.31×（无 load 下界）。

| JIT 数组重循环 50M 读 | 耗时 | 说明 |
|--|-----:|------|
| baseline（每 get 走 `jit_array_get`） | 834ms | boxed Value 往返 + C 边界 |
| 方案 A（per-get `jit_array_data` + 原生 load） | 655ms（1.27×） | 每 get 仍一次取指针 helper |
| **方案 B（loop-invariant 提指针到入口）** | **388ms（2.15×）** | **零 per-iteration 调用;= 含 load 的原生真实上限** |

方案 B:never-reassigned 数组寄存器的 buffer ptr+len 在入口块一次取（非抛出 `jit_array_data_opt`），
循环体内纯原生 bounds+load+去箱。null/无效数组 → len=0 → 无符号 bounds 恒 OOB → 回退 helper
在真实访问点抛正确异常（0 迭代不误抛）。正确性 jit==interp 覆盖 in-bounds/OOB/重赋值/null-0迭代/
null访问/**GC-stress(64M 分配跨 GC 提指针存活)**。GC 安全:非移动 GC + 定长数组 → ptr 稳定。

## 复现

```bash
cargo build --manifest-path src/runtime/Cargo.toml --release
bench/scripts/compare-modes.sh          # 用 release z42vm + .z42/libs
```
