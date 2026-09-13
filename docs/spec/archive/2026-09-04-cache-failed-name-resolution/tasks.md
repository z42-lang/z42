# Tasks: 缓存「查不到」的名字解析（cache-failed-name-resolution）

> 状态：🟢 实施+验证完成，待合并 | 创建：2026-09-04 | 类型：perf（不改可观察语义 → 最小化模式）
> 来源：三面评审二轮 P8「主循环 profile」。**profile 先行**——找到的是这条，不是原先排在
> 队首的「对象分配单块化」（`object_regions` 只占 36/2500 采样）。

## 背景（实测 profile，不是纸面推演）

分配密集基准（`Node`：4 个基元字段 + 2 个引用字段，1200 万次 `new`）做 `sample` 采样，
2996 个采样里 **1593 个（53%）落在 `jit_obj_new`**，而且**不在分配上**：

```
2996 exec_function_body
  1593 jit_obj_new                        ← 53%
    770 JitModuleCtx::resolve_id_by_name   →  try_lookup_function
    754 （else 分支）                       →  try_lookup_function   ← 同一个名字查第二遍
        └ candidates_for_namespace → format!("{ns}.") + 全表扫描 + Vec<String> + sort
        └ remaining_declared       → 又一次全表扫描 + Vec<String> + sort
        └ run_pending_static_inits → 再抢 2~3 次锁 + 扫 static_init_state
```

**根因**：`Node` 是一个**没有显式构造函数**的类。`new Node()` 去查
`Bench.AllocObj.Node..ctor$0`——这个函数**根本不存在**。于是每一次分配都：

1. `module.func_index` 未命中 → `lazy_table.by_name` 未命中 → `try_lookup_function` 走
   **完整解析**：命名空间路由 → Fallback-B 到不动点 → `None`；
2. **失败结果不被记住**，下一次分配从头再来；
3. `jit_obj_new` 的 `else` 分支**又调一次**（tiered 解析返回 `None` 分不清「不可 JIT」
   与「压根没有」），于是同一次分配付**两遍**。

interp 侧同病（`exec_object::obj_new` 的 `ctx.try_lookup_function(ctor_name)`）。
「没有显式构造函数的类」在真实代码里极其常见，不是基准特有的病。

## 实施

- [x] 1.1 `LazyLoader` 加 `negative: Option<Box<NegativeResolveCache>>`
      （两个 `FxHashSet<String>` + 指纹）。**懒分配**：从不失败的程序一分钱不付。
- [x] 1.2 有效性守卫 = **`registry_fingerprint()`：四个集合的 len 元组**
      （`loaded_zpkgs` / `declared_zpkgs` / `function_table` / `type_registry`）。
      这四个集合**只增不减**（全仓 grep 无 `remove` / `clear` / `retain` / `drain`），
      所以「长度不变」⇒「没有任何原先查不到的名字变得查得到」。长度一变整体丢弃重建。
      比在 5 个 insert 站点手工 invalidate 更难漏（#411 正是「三处各改一遍」出的错）。
- [x] 1.3 `resolve_function` / `resolve_type`：入口查负缓存，失败出口写负缓存。
- [x] 2.1 `try_lookup_{function,type}`：能**证明**排空是空操作时跳过
      `run_pending_static_inits()`（省两次加锁 + `static_init_state` 扫描）。
      判据 `static_init_drain_is_noop` 四条全真才跳：本次没加载新包、loader 的
      `pending_static_inits` 空（在调用方已持有的那把锁里顺手读）、
      `pending_type_init_count == 0`、`running_static_inits == 0`。
      **不是语义变更**：任一条不成立就照常排空；`static_init_concurrent` 依赖的
      「他线程正在跑初始化器」屏障就等于 `running_static_inits != 0`。
- [x] 2.2 两个无锁镜像计数器（`VmCore`）：`pending_type_init_count` 镜像
      `pending_type_inits.len()`；`running_static_inits` 在写 `InitState::Running`
      处 +1、写 `Done` 处 −1，**都在 loader 锁内**，所以 `== 0` ⇒ 不存在 `Running` 项。
      只用于 `== 0`（「可证明没事做」）这一个方向。
- [x] 2.3 `await_init_quiescence` 加同一个无锁前置检查。
- [x] 2.4 `jit_obj_new`：类名/构造函数名从 IR 串存储**借用**（`&str`），不再每次分配
      `to_string()` 两份；只有合成空 TypeDesc 的兜底分支才 `to_string()`。

## 验证

- [x] 3.1 `cargo test --lib` 1053 + 21 passed（含 4 条新的负缓存单测）
- [x] 3.2 wasm32 检查 0 error
- [x] 3.3 `xtask test` ✅ GREEN
- [x] 3.4 A/B 实测（同机 hyperfine；base = **同一棵树 HEAD 用同一条 cargo 命令**编出的 VM）

## 实测

| 场景 | base | 本变更 | |
|---|---|---|---|
| 分配密集（1200 万 `new`，无构造函数）| 4.933 s ± 0.051 | **2.555 s ± 0.012** | **1.93×** |
| **z42c 编译 hello**（`--no-incremental`，真实工作负载）| 483.7 ms ± 7.7 | **461.1 ms ± 6.7** | **1.05×** |
| 分配密集的 peak RSS | 2691 MB | 2691 MB | 持平（本变更是 CPU 不是内存）|
| hello 启动 | 见下「度量陷阱」 | | 无可归因变化 |
| `bench/scenarios` 01/02/04/05/07/08/10/11 | | | 全在噪声内（≤1.02×，无回归）|

只做负缓存、不做 2.1 的排空跳过时，分配密集基准是 **1.66×**；2.1 把它抬到 **1.93×**。
z42c 那 5% 是**非微基准**的证据：编译器里大量类没有显式构造函数，每个 `new` 都在付这笔钱。

未测：`z42i -c` REPL（本树没建 toolchain；它的耗时主要在编译管线，与 z42c 那条同源）。

## ⚠️ 度量陷阱：hello 启动的「布局彩票」（本次踩到，值得记住）

wall-clock 上本变更一度稳定显示 hello 启动 **+0.4 ms（+6%）**，可复现、跨轮次一致。
用 `instructions retired`（对代码布局免疫的计数）追下去，最小复现是：

> **在 `LazyLoader` 上加一个从不读的 `usize` 字段**（其余全部保持 HEAD，`git diff` 只有 3 行），
> hello 启动就从 **69.6 M 指令涨到 73.5 M（+5.7%）**——与本变更观察到的 +4 M 分毫不差。

也就是说这 0.4 ms **与本变更的逻辑无关**：`--print-stats-on-exit` 的计数器
（builtin_calls / jit_methods_compiled / allocations / …）base 与 PR **完全一致**，
把负缓存的调用点全部 `if false` 掉、把缓存装箱成 8 字节、给辅助函数加 `#[inline(never)]`、
把整个 `LazyLoader` 装进 `Box`（让 `VmCore` 大小不再随它变），+4 M 都原样存在；
而一个纯死字段就能复现全部差值。

**教训**：hello 启动只有 ~6.5 ms、以冷代码为主，`LazyLoader` 布局的任何扰动都会带来
±6% 的摆动。判断一个改动是否真的拖慢启动，必须

1. 看 **instructions retired**（`/usr/bin/time -l`）而不是只看墙钟；
2. 做**扰动对照组**（HEAD + 一个死字段）——对照组同样摆动 ⇒ 差值不可归因于本变更。

（本次差点因此砍掉一条 1.94× 的优化。）**已写进
`docs/book/src/dev/benchmarking.md`「启动类微小回归：先排除布局彩票」**，供后续启动优化引用。

## Out of scope

- **对象分配单块化**（`bytes` + `refs` 两次额外 malloc）：二轮 P3 原本的头号候选，
  本次 profile 显示它排在名字解析后面。仍在队列里，需要单独 proposal。
- `jit_obj_new` 剩下的**两次** `try_lookup_function`：负缓存之后两次都只是「一把锁 +
  两个哈希探测」；再合并需要给 `resolve_id_by_name` 换一个能区分「不可 JIT」与
  「不存在」的返回类型，收益/风险比不如本次这几条。
- 把「本类无构造函数」缓存到 IR 调用点（类似 `type_token`）：能连那把锁也省掉，
  但要动 IR 指令与 JIT helper 签名，单独变更。
