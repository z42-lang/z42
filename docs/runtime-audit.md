# z42 运行时审计与改进计划（临时迭代底稿）

> **临时底稿**：运行时改进迭代的工作文档，不属于三本书。阶段 0 已全部完成（修复的知识已上浮到 internals / reference，本文不再保留）；本文现在是**阶段 1** 的工作文档。迭代全部完成后删除本文件（同时移除 `docs/README.md` 里的入口链接）；届时仍未完成的项转入 `docs/internals/` 对应机制页的「待办 / Deferred」（类别登记见 [doc-system.md](agent/rules/doc-system.md)）。
> 范围：`src/runtime`。方法：按子系统（值模型/IR、加载链路/启动/配置、解释器、JIT、GC、corelib/native/host/VmContext、构建打包）逐个只读审计 + 实测（release z42vm，macOS arm64：`sample` 采样、GC trace、计时）。
> 证据级别：**【实测】** profile / trace / 计时（方法见附录）；**【确认】** 读代码确认；**【推断】** 有代码依据但需测量。
> 计时环境负载较高，绝对值可能偏高 20–40%，相对比例可信。文中行号对应审计时的版本，随修改漂移。

---

## 0. 结论速览

1. **头号系统性问题：运行时的"身份"全程按名字。** 函数/类型/字段/方法从解码起就被还原成 `String`，运行期再靠一堆哈希表和旁路缓存"重新压缩"。wire 格式本来是紧凑的下标与 token，加载器先解压成字符串、执行时再哈希回去。JIT 字段 helper 按名解析在 z42c 构建里占 **约 10%**。惰性包函数没有整数身份，导致 PIC 永远缓存不了跨包目标，而 z42c 的代码全在惰性包里（§4.1）。
2. **调用协议是解释器与 JIT 共同的天花板。** 每次调用约 11 次原子 RMW、3 次加锁、4 次 TLS、2 个 `Arc<str>` 引用计数，JIT 还要经 4 次 helper。单态虚调用 JIT ≈52 ns、解释器 ≈94 ns；fib 递归每次调用 ≈140 ns【实测】。帧管理在 z42c 里占 **约 10%**（§4.2）。
3. **对象模型与 GC 的内存开销是 .NET 的 2.5–3 倍，RSS 是 GC 记账堆的 4.5–6 倍。** 72 B 头（含每对象一把 Mutex）+ 单独 malloc 的负载；TLAB 不复用死槽。默认的分代 + 增量模式在大堆场景下**比纯 STW 更慢也更大**【实测】（§4.3、§4.4）。
4. **JIT 在结构上只能单线程。** worker 线程永远走解释器（`jit_ctx` 恒为 0），z42c `--jobs N` 的并行阶段全部是解释执行【确认】（§4.5）。
5. **组件化的前提尚不成立。** interp→jit 有 12 处依赖；metadata ↔ gc ↔ vm_context 成环；GC trait 有 58 个方法却只有 1 个实现，还大量泄漏实现细节；VmCore 是上帝对象，挂着 socket/文件/进程表；`#[cfg(target_os)]` 在 PAL 外有 123 处、PAL 内只有 13 处（§5）。
6. **还有 6 个已确认、未修的缺陷**（§3），都不在热路径上，但有挂起、fd 泄漏、数据竞争的风险。

---

## 1. 实测数据

> §1.1–§1.4 测于阶段 0 落地之前。阶段 0 修掉了其中的 cctor 屏障按名检查、`F_FULLFSYNC`、惰性加载击穿等项；P1-0 在当前代码上重测，结果写进 §1.5，作为阶段 1 的基线。

### 1.1 微基准（release z42vm，5 次中位数）

| 场景 | JIT | interp | 折算 |
|---|---|---|---|
| 10_mono_vcall（2000 万次单态虚调用 + 字段读） | 1045 ms | 1902 ms | JIT ≈52 ns/次，interp ≈94 ns/次 |
| 05_polymorphic_dispatch（1000 万次，4 路 PIC） | 570 ms | 1130 ms | 56 / 112 ns/次 |
| 01_fibonacci fib(25)（24.3 万次调用） | 45.7 ms | 52.7 ms | 扣除约 11 ms 启动后：JIT ≈140 ns/调用，interp ≈170 ns/调用 |
| 04_arith_loop | 52 ms | 175 ms | 纯数值循环，JIT 有效 |
| 09_alloc_ctorless（150 万对象全存活） | 473 ms | 500 ms | ≈310 ns/迭代；RSS 243 MB ≈ 155 B/对象（.NET 约 56 B） |
| 11_type_test_chain | 574 ms | 1031 ms | |
| 12_gc_churn | 533 ms | 726 ms | |
| 13_gc_large_heap（活集理想约 50 MB） | 3.0 s | 3.0 s | RSS 991 / 1182 MB；最大停顿 66 / 111 ms |
| hello（Console.WriteLine） | 10.2–11.6 ms | — | 进程 + dyld 基线约 3 ms |
| hello + 1 次自由函数调用 | 24.4 ms | — | RSS 23 MB（不调用时 11 MB）：cctor 屏障把命名空间送进惰性加载器，装入全部 16 个 stdlib 包（阶段 0 已修） |

**JIT 模式下 `b.Get()` 的剖面**（单态、IC 恒命中）：helper 合计约 90%，生成码只占约 10%。按 self 采样数：`invoke_entry` 469、`JitFrame::recycle` 306、`jit_obj_field_slot` 267、`pop_frame` 265、`jit_vcall` 256、`str::from_utf8` 188、`push_frame` 176、`take_pooled_regs` 168、`memcmp` 95、`_tlv_get_addr` 44。

### 1.2 真实负载：z42c 构建编译器工作区（9 个包，约 9.2 万行 z42，jobs=1，JIT）

墙钟 25.9 s，RSS 峰值 1.0 GB，GC 记账峰值 170 MB。`sample` 采样 18 s 共 15023 个样本，按包含时间统计：

| 开销项 | 占比 | 根因 |
|---|---|---|
| cctor 屏障（`ensure_callee_owner_init` 9.2%、`ensure_static_owner_init` 3.7%、type/module init 1.2%） | **≈14%** | 每次调用、每次静态读都要按 `.` 切分限定名，再去 `HashMap<String, Arc<TypeDesc>>` 查，可能还要加锁（§4.1） |
| JIT 字段 helper（obj_field_slot 3.3、obj_ref_field_slot 2.9、field_get 3.0、field_set 0.9） | **≈10%** | 每次执行都做 `from_utf8` + NameIndex memcmp + 对象锁 + Arc clone（§4.3） |
| 帧管理（push 1.7、pop 2.5、take_pooled_regs 2.6、recycle 3.1） | **≈10%** | §4.2 |
| GC | 6.7% | |
| `write_atomic` → `F_FULLFSYNC` | **6.3%** | 每个缓存文件都整盘刷写（§4.8） |
| Cranelift 编译（regalloc2 3.2%） | 5.1% | 每函数一次 finalize（§4.5） |
| `jit_is_instance` | 1.8% | 每次 `from_utf8` |
| mimalloc malloc/free | <1% | 历史上的"受 malloc 限制"已不成立，现在受查找限制 |

GC 模式对比：STW 21 次回收、25.1 s、844 MB；分代 193 次、26.2 s、968 MB。**分代模式对真实负载零收益。**

> 注：以上剖面用的是 10-02 13:40 的 release 二进制，早于 #1048（per-ctx 查找缓存）。§1.4 用当前 origin/main 重测校准，结论不变。

### 1.3 GC 与内存

- 13_gc_large_heap 各模式对比：STW 2.20 s / 803 MB / 9 次 / 最大停顿 79 ms；concurrent 2.51 s / 842 MB / 99 ms；**generational 4.05 s / 1097 MB / 214 次 / 最大停顿 193 ms**【实测】。
- `Z42_GC_PHASES` 共 167 次 trip，其中退避倍数 x4 出现 45 次、x16 出现 3 次、x64 出现 23 次；有 `major cycle … reclaimed 0`；`trip minor gate 5.8M x16 grown 92.5M (last freed 0B)`【实测】。
- 每对象实际字节（GC 审计用布局镜像算出）：

| 对象 | z42 | .NET |
|---|---|---|
| 2 字段类 | 88 B | 32 B |
| `Node{long, long[], Node}` | 96 B | 40 B |
| `long[8]` | 216 B | 88 B |
| 09 场景的 6 字段 Node | 128–136 B | 56 B |

### 1.4 当前代码重测（用 origin/main 80cd8f3b5 源码单独编译的 release z42vm）

同一 z42c 构建：23.0 s，RSS 1.05 GB；14994 个样本，按包含时间统计：

| 开销项 | 10-02 二进制 | 当前代码 | 说明 |
|---|---|---|---|
| cctor 屏障 | ≈14% | **≈10.7%** | #1048 的 per-ctx 缓存削掉一部分；剩下的一半是对函数名做 `rfind('.')`（3.4%），另有 `try_lookup_type` 1.5%、HashMap 1.0% |
| JIT 字段 helper 按名 | ≈10% | **≈10.5%** | 无变化 |
| 帧管理 | ≈10% | **≈10.4%** | 无变化 |
| `write_atomic`（`F_FULLFSYNC`） | 6.3% | **12.0%** | 随磁盘争用波动 |
| `resolve_lazy_slot` | — | 6.5% | 主要是持锁做惰性编译（compile_fn 4.9%），另有每次调用的 mutex 约 1% |
| GC / Cranelift | 6.7% / 5.1% | 6.7% / 5.2% | |

可直接消除的开销合计约 45%：按名查找约 23%、帧约 10%、fsync 约 12%。

微基准在新旧两个 VM 上背靠背计时，没有差异：10_mono_vcall JIT 1139 → 1077 ms，fib 44.3 → 44.3 ms，startup 13.4 → 12.6 ms，属于噪声范围。OSR ref 回写缺陷和自由函数加载 16 个包的缺陷在当前代码上**均可复现**。

### 1.5 阶段 0 之后重测（P1-0）

当前代码 = origin/main `b8470b191` + #1128（静态字段屏障缓存），release z42vm，macOS arm64。测量时机器 load average 10–14，绝对值偏高，相对比例可信。

**z42c 构建编译器工作区**：工作区现为 11 个包（比 §1.2 多 2 个），墙钟不能与 §1.2 直接比。jobs=1 墙钟 16.8–17.3 s，RSS 0.88–0.92 GB；默认并行 12.4 s（user 26 s），RSS 0.96 GB。

`sample` 14 s 共 11655 个样本，按包含时间统计：

| 开销项 | 当前 | §1.4 | 说明 |
|---|---|---|---|
| 帧管理（`push_frame` / `pop_frame` / `take_pooled_regs` / `JitFrame::recycle`） | **13.5%** | ≈10.4% | 几乎全是 self 时间；其他项降下来后成了最大单项（§4.2） |
| GC（其中 minor 4.0%） | **9.9%** | 6.7% | |
| 解释器 `exec_function_body`（self） | **9.5%** | — | 惰性包函数大多没被路由到 native（§4.5 第 2 条） |
| JIT 字段 helper（`jit_obj_field_slot` / `jit_obj_ref_field_slot` / `jit_field_get` / `jit_field_set`） | **8.7%** | ≈10.5% | 按名解析（§4.3） |
| `resolve_lazy_slot`（其中持锁 `compile_fn` 6.5%） | **8.0%** | 6.5% | |
| Cranelift 编译 | 6.3% | 5.2% | |
| `jit_call`（self，按名调用，含 `from_utf8`） | 5.8% | — | §4.1 |
| `jit_array_*` | 3.9% | — | |
| `write_atomic`（`F_BARRIERFSYNC`） | 3.2% | 12.0% | |
| `memcmp`（self，名字比较） | 2.8% | — | |
| cctor 屏障合计（调用 0.5%、静态字段 0.5%、模块初始化 0.9%） | 2.5% | ≈10.7% | |
| is / as（`isa_td`、`jit_is_instance`） | 2.0% | — | |

**微基准**（5 次中位数；12_gc_churn 为 9 次）：

| 场景 | 基线 JIT / interp | 当前 JIT / interp |
|---|---|---|
| 01_fibonacci | 45.7 / 52.7 ms | 40.0 / 45.1 ms |
| 04_arith_loop | 52 / 175 ms | 52.7 / 158.9 ms |
| 05_polymorphic_dispatch | 570 / 1130 ms | 577 / 1077 ms |
| 09_alloc_ctorless | 473 / 500 ms | 456 / 503 ms |
| 10_mono_vcall | 1045 / 1902 ms | 990 / 1818 ms |
| 11_type_test_chain | 574 / 1031 ms | 415 / 1008 ms |
| 12_gc_churn | 533 / 726 ms | 515 / 796 ms |
| 13_gc_large_heap | 3.0 / 3.0 s | 2.97 / 3.30 s |
| hello | 10.2–11.6 ms | 12.0 ms，RSS 12 MB |
| hello + 1 次自由函数调用 | 24.4 ms，RSS 23 MB，惰性加载 16 个包 | 11.9 ms，RSS 12 MB，不再惰性加载 |

**13_gc_large_heap 各 GC 模式**（3 次取中位）：

| 模式 | 基线 | 当前 |
|---|---|---|
| STW | 2.20 s / 803 MB / 9 次 / 最大停顿 79 ms | 2.16 s / 799 MB / 9 次 / 80 ms |
| concurrent | 2.51 s / 842 MB / 最大停顿 99 ms | 2.39 s / 837 MB |
| generational | 4.05 s / 1097 MB / 214 次 / 最大停顿 193 ms | 3.11 s / 995 MB / 75 次 / 最大停顿 95 ms，p99 74 ms |

**结论**：阶段 0 拿掉了 cctor 屏障（≈10.7% → 2.5%）和整盘刷写（12% → 3.2%）。当前的头部是帧管理、GC、惰性包函数留在解释器、按名字的字段访问和调用、持锁的惰性编译，分别对应 P1-5、P1-7、P1-2、P1-4。分代模式在大堆场景下仍比 STW 慢约 45%、RSS 多约 25%。

---

### 1.6 跨语言内存 / GC 对照（2026-10-07）

origin/main `bcf9fa25`，release z42vm（JIT，mimalloc）。对照：Python 3.9、Node 26、Ruby 2.6、.NET 10（workstation GC）、Java 21（G1 / Serial）。每组 3 次中位，峰值 RSS 用 `/usr/bin/time -l`（**不带 `--stats`**：它会把全部活对象收进一个 Vec，每对象多 16–32 B）。密度 = (RSS(N) − RSS(0)) / N。测量时机器负载偏高，墙钟只看量级，内存数字基本不受影响。脚本：scratchpad `memcmp/run_all.sh`（会话内）。

| 负载 | z42 分代 | z42 STW | 最优 | Python | Node | z42 分代 / 最优 |
|---|---|---|---|---|---|---|
| 小对象（int + 引用），B/个 | 107.9 | 99.8 | 24.5（Java） | 81.0（slots）/ 194.9 | 70.0 | 4.4× |
| `long[8]`，B/个 | 267.7 | 247.7 | 83.8（.NET） | 203.9 | 197.9 | 3.2× |
| 短字符串，B/个 | 134.8 | 102.6 | 56.6（Java Serial） | 73.2 | 63.5 | 2.4× |
| `List<int>`，B/元素 | 77.6 | 117.7 | 8.9（Ruby）/ 10.4（.NET） | 46.1 | 31.5 | 8.7× |
| `Dictionary<string,int>`，B/条 | 291.7 | 368.3 | 99.5（Node） | 181.2 | 99.5 | 2.9× |
| binary-trees 18：墙钟 / 峰值 RSS | 14.0 s / 760 MB | 14.0 s / 699 MB | 0.30 s（Java）/ 59 MB（Python） | 28.0 s / 59 MB | 0.50 s / 185 MB | — |
| 大活堆流失（13 的移植）：墙钟 / RSS | 3.60 s / 1029 MB | 2.10 s / 837 MB | 0.25 s（Java G1）/ 113 MB（Python） | 2.35 s / 113 MB | 0.63 s / 461 MB | — |

**z42 的字节去向**（已与实测对账）：

- **小对象 ≈ 100 B**：
  - `RegionEntry<ScriptObject>` 72 B，其中每对象一把 Mutex 8 B、GC 元数据 32 B（`gc/region/entry.rs`）。
  - chunk 18432 B 被 mimalloc 取整到 20480 B 档，每对象多 8 B。
  - 字段 payload 单独 `alloc_zeroed` 16 B（`metadata/types/obj_storage.rs`）。
  - 分代模式下 `young_list` 再加 8 B。
- **`long[8]` ≈ 250 B**：
  - `RegionEntry<ArrayObj>` 104 B，chunk 取整后 112 B。
  - 每个数组都 `Arc::from(element_type)` 一次，24 B。
  - 元素块是 16 B 头加 64 B。
  - 侧表约 9 B。
- **`List<int>` 77.6 B**：
  - 泛型 `T[]` 一律装箱，每元素 16 B。
  - 每次新建数组都先建 `vec![Value; n]` 临时 Vec 再拷进 GC 块。单独 `new int[16M]` 就是 RSS 415 MB，而 used 只有 67 MB。
- **流失场景 RSS / used 达 8×**：
  - TLAB 不复用空洞。
  - 死对象的 payload 要等槽被覆写才释放。
  - 没有任何 decommit。
  - 退避让年轻集合膨胀。
  - 因此 `Z42_GC_MAX_BYTES` 约束不住 RSS。
- **吞吐**：binary-trees 比 Java 慢 46×，GC 只占约 13%，主要在分配和调用路径。

## 2. z42vm 二进制构成

z42vm `__text` 构成（macOS arm64，共 4.97 MB），是拆 crate（阶段 2）与可选依赖的依据：

| 组成 | 体积 | 占比 |
|---|---|---|
| z42 VM 本体 | 1.46 MB | 29% |
| Cranelift + regalloc2 | 1.35 MB | 27%（Linux x64 上 3.82 MB / 47%，其中 `cranelift_assembler_x64` 1.88 MB） |
| rustls + ring + webpki | 0.57 MB + 约 0.2 MB 表 | 无条件依赖，移动端 staticlib 也带着 |
| regex-syntax / automata | 0.29 MB | 只因 tracing-subscriber 开了 `env-filter` |
| clap | 0.20 MB | |
| toml | 0.11 MB | |
| std | 0.53 MB | |

---

## 3. 已确认、未修的缺陷

| ID | 缺陷 | 证据 | 位置 | 修法 |
|---|---|---|---|---|
| D1 | cctor `claim` 等待时既不 park 也不 poll | 【确认】yield 若干轮后 200 µs sleep 轮询，30 s 超时 | `vm_context/cctor.rs` 的 claim 等待循环 | 另一线程等 cctor 时若有 GC 请求，GC 要等满 30 s，然后抛出伪造的"循环初始化"异常；等待改为 Condvar 加 `NativeParkGuard` |
| D2 | socket 读期间把 stream 从表里摘走 | 【确认】`corelib/network/tcp.rs` 读路径 `map.remove(&slot_id)` 后在锁外读，读完放回；UDP/TLS 同构 | | 全双工写会拿到 handle_invalid；读期间 Close 无效，读完 socket 又被放回（fd 泄漏）；改成 `Arc<TcpStream>` + closed 标志 |
| D3 | `is_exception_subclass` 只查入口模块 | 【确认】`exception/mod.rs:234-245` 只走 `module.type_registry` | | 惰性包里的异常层级会丢 StackTrace/Message【推断】；改调 `isa_td` |
| D4 | 每对象 Mutex 被 JIT 绕过 | 【确认】JIT 在 hoist 时取出 bytes 指针即释放锁，之后裸写 | | 按 Rust 语义是数据竞争（UB）；`__array_copy` 先锁 src 再锁 dst，存在 ABBA 死锁面【推断】。根治要过内存模型决策（⏸） |
| D5 | `isa_cache` / `subclass_memo` 以地址为 key | 【确认】`vm_context/isa_cache.rs` 以 `*const TypeDesc` 与类名字符串地址为 key，依据是"元数据在 VM 生命周期内不朽" | | 可回收 load context 被回收后不失效，地址复用可能误判【推断】；随 TypeId（P1-2）改为按 id 做 key |
| D6 | OSR 后栈闭包 env 丢失（已随栈闭包删除而消除） | 【确认】z42c 从不给 `MkClos` 置栈分配标志，栈闭包路径不可达 | — | 已决（User，2026-10-07）：删除运行时栈闭包支持。`Value` 不再有栈闭包变体，interp `Frame` / `JitFrame` / `VmFrame` 不再有 `env_arena`，闭包恒堆分配；OSR 交接只有寄存器，无可丢的 env。zbc `MkClos` 的尾字节保留、VM 读后丢弃，下次 zbc bump 删除 |

---

## 4. 性能：根因与改法（按收益排序）

### 4.1 根因一：身份按名字（z42c 中约 25% 以上可直接消除）

**现象**【确认 + 实测】：
- **cctor 屏障**：属主判定已缓存在 `Function.owner_init`（调用屏障）和按字段槽号的 `static_owner_cache`（静态字段屏障），自由函数与无 cctor 的类型永久免检。剩下的是 JIT 仍经 helper 判定，没有在生成码里内联代际检查。
- **惰性包函数没有整数身份**：VCallIC 载荷是入口模块 `functions` 下标，所以 Lazy 目标**永不进 PIC**。每次跨包虚调用都要重走：TypeDesc Arc clone、vtable 线性字符串比较、func_index miss、`try_lookup_function`（锁 + 哈希）；JIT 还要再查一次 `LazyTable`（全局 Mutex + SipHash）。z42c 的分派站点（Call 10252 / VCall 1245 / ObjNew 2116 / FieldGet 19539 / StaticGet 2070 / ConstStr 6231）**全部在惰性包里**。
- **ObjNew** 每次都哈希类型名和 ctor 名（resolver 算好的 `type_tokens` 没用于派发，`exec_object.rs:13-18` 自己承认）。
- **CallIndirect / FuncRef / 闭包**：每次 `name.to_string()` 分配 + 哈希；`LoadFn` 每次新分配一个 GC 字符串。
- **ConstStr**：每次锁 + 哈希，惰性包里要查两遍。
- **JIT helper** 按名字传参（指针 + 长度），每次在 helper 里重建 `&str` 再去哈希。
- **三套函数注册表**（`Module.func_index` 键 u32 / `LazyLoader.function_table` 键 String / JIT `LazyTable` 键 String 外加 Mutex），**两套类型注册表**，**每个调用点三套并行缓存**（`method_tokens` / `cross_module_targets` / `call_jit_ic`）。

**改法（root fix：链接一次，之后全程用整数）**：
1. VmCore 上建进程级 **append-only `FuncTable` / `TypeTable`**：分块存储，`AtomicU32 len` 用 Release 发布，读取无锁。入口模块函数占 0..n，惰性加载的函数逐个追加 FnId。TypeId 本来就是全局唯一的。
2. `Function` 上的 MethodDesc 式运行期槽：`owner_init` 已有，再加 `jit: {state, code ptr, calls}`。JIT 编译时就已知 owner：没有 cctor 的不生成屏障，有的内联成 `load gen; cmp; brif cold`。
3. 编译器侧引入 **beforefieldinit** 语义：只含字段初始化器的合成 cctor 打上类标志，这类类型的静态方法调用不设屏障。
4. PIC 载荷、`method_tokens`、ObjNew 站点、FuncRef、Closure 全部存 FnId / TypeId；三套缓存合一；IC 能缓存 Lazy 目标。
5. 解码时把 token 解析成 FnId / TypeId / StringId，名字只留在侧表供反射和诊断用（`ir.md` 里 deferred 的 slim-instruction-stringid）。

**收益**【推断】：跨包 VCall 100–300 ns → 约 5 ns。这也是 load context 卸载、多 VM 共享镜像、AOT（不再烘焙绝对地址）的前置条件。

### 4.2 根因二：调用协议与帧模型（两后端共同的天花板）

**现状**【确认】：每次 interp→interp 调用包括：
- `call_stack: Arc<Mutex<Vec<VmFrame>>>` 加锁 3 次：更新行号、push、pop；
- `VmFrame` 96 B，带两个 `Arc<str>` 名字（clone/drop 共 4 次 RMW）；
- `next_frame_id` 一次 `fetch_add`；
- 4 次 TLS：寄存器池取/还、`VmGuard`、`HeapGuard`；
- `resize(max_reg, Null)` 写满寄存器；

JIT 的每次调用 = 4 次 native→Rust helper（vcall/call、`jit_regs_ptr`、`jit_set_ret`、入口 hoist 的 field slot）+ 同一套帧协议。**没有 JIT→JIT 直接调用。** 这套"建帧 / push / 调用 / pop / marshal"序列在代码里有 **10 份拷贝**。

**改法**：
1. **v1（低风险）**：
   - `VmFrame{*const Function, regs, pc: Cell<u32>, base}`，名字、文件、行号在生成栈回溯时由 func+pc 懒算；
   - `call_stack` 改为 owner-only 的 `UnsafeCell`：跨线程只在 owner 已 park 时读，safepoint 握手已经提供了 happens-before；
   - 寄存器池挪进 VmContext；guard 只在引擎入口安装；
   - frame_id 加内联快门；
   - JIT 侧：`regs_ptr` 作为帧首字段直接按偏移 load，`Ret` 直接写 `frame.ret`。
2. **v2**：每线程一块**连续 Value 栈**，供 interp 与 JIT 共用。Lua 式寄存器窗口传参（零拷贝），帧头只有 2 个字，GC 扫 `[0, top)`。三个 arena 合并为栈上 bump；`RefKind::Stack` 改用绝对下标，顺带消掉 `store_thru_ref` 把 `*const` 强转 `*mut` 的别名 UB。
3. **v3**：JIT 调用点经 FnId 的 code 槽直接 call（stub 负责惰性编译），helper 只留给慢路径。

**收益**【推断】：10_mono_vcall interp 97 → 约 50 ns；JIT 54 → 约 5–10 ns；z42c 帧相关的约 10% 基本拿回。

### 4.3 根因三：对象模型与字段访问

**现状**【确认】：
- 字段读要走：句柄 → RegionEntry（2 次 Acquire + 对象 Mutex）→ ScriptObject → `Arc<TypeDesc>` → cold Box → `ObjectLayout` → field_access → ObjStorage（另一个 malloc 块）。一次字段访问约 10 次依赖 load。
- FieldIC 只缓存 slot，不缓存 offset。
- 对象头 72 B，含每对象一把 Mutex；负载在 GC 区之外单独 malloc；string 字段放在 refs 侧表（16 B），字节区为它留的 8 B 永不使用，所以一个 string 字段占 24 B；每个泛型对象还要冗余地逐实例拷贝 type_args。
- 数组：104 B 头 + 每个数组一个 `Arc<str>` 元素类型名；`new int[n]` 先建 16n B 的临时 Vec，再转 4n B，再拷进 GC 块；泛型 `T[]` 一律 Boxed，所以 `List<int>` 每元素 16 B（压缩存储只需 4 B）。
- JIT 的入口 hoist：只要函数里出现"从不被写的对象寄存器 + 字段"，每次激活都在入口调一次 slot helper，与循环无关。对 getter 是负优化。

**改法**：
1. FieldIC 载荷改成 `TypeId | offset:16 | kind:8`；`field_access` 挪到 TypeDesc 热区（≤64 B）。
2. JIT：原生单态站点 IC，生成序列为 tag 检查 → load `type_desc` → 比较 → load 字段；hoist 只用于循环内的不变接收者。
3. **内存模型决策**（已定，见 §7「对象模型（M10 + M11）已定方案」）：去掉每对象 Mutex，字段改为同宽原子读写（原始类型 relaxed，引用 release 写 / acquire 读），lock 语义交给 Monitor 侧表。
4. 实现 `object-abi.md` §3 已设计但未实施的 16 B 统一头；string 字段内联为 8 B；负载并入 GC 槽，一次分配；引用数组元素 8 B；TypeDesc 由 arena 持有，对象存裸指针。

**收益**【推断】：字段访问 15–25 ns → 1–3 ns；小对象 104 B → 32–40 B。

### 4.4 根因四：GC 策略与内存复用

**策略环（解释了"分代比 STW 慢且大"、0B 周期、长停顿、linux-arm64 回收次数塌缩）**【确认代码链路 + 实测佐证】：
1. 增量周期打开时 `begin_alloc_black`，周期内所有新生对象都打上本周期 epoch；
2. minor 用 `keep_major` 把它们一律视为活（不再给它们升龄，但仍留在年轻代里占位）；
3. minor 回收不到东西，被判为徒劳，退避 ×4，没有上限，最高 ×64（加上限、周期内不判徒劳是吞吐换停顿的取舍，⏸ 待 User 决策）；
4. 结果是二选一：巨型 minor（长停顿），或者长时间不回收（次数塌缩、堆超调）。

**内存放大**：
- TLAB **从不复用半活 chunk 里的死槽**（`region.rs:273-275` 写着 "Slot-level reuse … stays Deferred"）；
- 死对象的 payload 要等槽位被复用才释放；
- 每次暂停都丢弃 TLAB 尾巴；
- 池化 chunk 从不还给 OS；
- `used` 只计实际占用的 50–75%，且不计侧表和碎片。

结果是 RSS 达到 used 的 4.5–6 倍。

**其他**：
- 写屏障遇到老指新时取**全局 region Mutex**；大数组一个条目只有一张卡，每次 minor 整个扫一遍。
- `MagrGC` 是一个 58 方法、单实现的 dyn trait，每次分配和屏障都走虚调用。
- 每次 `new` 约 7–9 次原子 RMW + 1 对锁 + 1 次 malloc。
- `stats()` 和 `PauseStatsRaw` 每次调用都遍历全堆。
- 报告的停顿不含 TTSP。
- ConcurrentMarkSweep 全面劣于 STW，又与增量 major 功能重叠，已删除（User 2026-10-07 决定）：`gc-mode` 只剩 `stw` / `generational`。

**改法**：
- **根治**：年轻代归 minor 管，晋升时 promote-black，年轻对象不再 allocate-black；先在 `tests/gc_incremental_model.rs` 模型 D 里穷举验证。
- TLAB 洞复用（Immix 式行/槽复用）；sweep 时就地释放 payload；续用尾巴；池超阈值后 `madvise`。
- 策略按真实 footprint 记账（`committed_bytes` 已能报告，闸门仍按 `used`）；pacer 目标改为一个 nursery 内跑完一个周期。
- STW 握手：分配点加停车点；JIT 编译、zpkg 加载、cctor 等待期间都 park；TTSP 单独计量。
- 无锁原子卡表；大数组按元素区间设卡；热路径用编译期选定的具体 Heap 类型，冷 API 留在 `HeapAdmin` trait。

**收益**【推断】：13 场景分代模式回到不高于 STW；RSS ÷2–4；arm64 回收次数不再塌缩。

**待做的验证**：linux-arm64 `--jobs 4` 下跑 `Z42_GC_PHASES=1 Z42_GC_TRACE=1`，看 trip 行是否出现 ` x64`、`open cycle` 是否接连出现、trip 行数与 AfterCollect 行数是否一致。

### 4.5 JIT 专项

1. **worker 线程不跑 JIT**【确认】：`construct.rs:112` 在 `new_with_core` 里把 `jit_ctx` 置 0；JIT ABI 经共享的 `JitModuleCtx.vm_ctx` 访问 VmContext（safepoint 计数、call_stack 都是主线程的），直接放开会产生数据竞争。改法：每线程上下文放进 JitFrame 或作为第 3 个参数传入，JitModuleCtx 挂到 VmCore 上。
2. **惰性函数缺统一身份的 5 个后果**：
   - interp→native 既不路由也不计数（只编了 58 个方法）；
   - PIC 不缓存 lazy 目标；
   - `resolve_lazy_slot` 每次调用都加锁；
   - 不可翻译的 lazy 目标没有负缓存，每次调用都整函数重扫；
   - OSR 只支持 merged 函数。

   全部由 §4.1 的 FnId + code 槽解决。
3. **每编一个函数就 `finalize_definitions` 一次**：每个函数独占一块 mmap，外加 mprotect 和 icache 刷新；实测记录 1325 个函数约 85 MB RSS。代码内存永不释放。改法：自定义 `JITMemoryProvider` 用致密 arena（双映射或 MAP_JIT），批量或后台编译，复用 Context，helper 惰性导入，共享 landing pad。
4. **类型化寄存器全有或全无**：寄存器只要有一处出现在内存路径（如作数组下标）就整个失去常驻资格；`int`（I32）的常量和 Copy 走 helper；`Value: Copy` 之后，"引用 Copy 需要 helper 才能 drop"的理由已经失效；比较和分支没有融合；前向 BrCond 也 poll。
5. **不可翻译的函数**按整函数粒度降级：调用方带 `ref` 实参、任何泛型调用、`CallNative` 都会让整个函数走解释器，并把它的 lazy 子树也拖进解释器。
6. Cranelift 用默认 `opt_level` none；生成码到处烘焙进程内绝对地址，阻碍 AOT 复用（`aot.md` 设想的"泛化为 `M: Module` 即可"不成立）。
7. JIT 忽略逃逸分析的 `stack_alloc`，比 interp 多做 GC 分配。
8. helper 里任何 panic 都会 abort 进程（`extern "C"` 不展开）。
9. 新增一个 helper 要改 6 处；Cranelift 签名与 Rust 签名分开手写，没有编译期校验。

### 4.6 解释器专项

- `Instruction` 32 B，热的三寄存器指令只用 13 B（59% 是 padding）；寄存器在内存里是 u32，wire 上是 u16；FieldGet/Call/VCall/ObjNew 这些最热的指令反而被装箱。
- 每条指令都做一次 `func.resolved.get()`（OnceLock Acquire）；带 token 的指令要经 `site_index[b][i]` → 分类 Vec，三级依赖加载。
- 没有 quickening；超级指令只有 CmpBr 一条（文档自测收益 5–11%）。
- `Convert`：int→long 在运行期是恒等变换，仍要走一遍分发。
- builtin：`[Native]` 桩 = 一次完整 z42 调用 + 实参收集。
- 静态字段：VmCore **全局** `Mutex<Vec<Value>>`；读到 Null 时每次都重跑 `verify_static_field`，还会分配一个 String。
- 异常：每个 try 条目 2 次 SipHash（label 是 String）；throw 时当场格式化整个栈。
- **改法**：加载期把 IR lower 成 interp 专用的预链接紧凑码，也就是 `tiered-execution.md` §3 已设计的 tier1 指令流，直接拿来当 tier0：
  - u16 寄存器、16 B op、跳转用偏移；
  - 操作数里直接带 IC 槽号；
  - 按 `reg_types` 静态 quicken（AddI64、CmpI64）；
  - 消除恒等 Convert；
  - 原生桩直接派发 builtin；
  - 有了定长编码之后再上 handler 表 + 尾调用分发。

  这是 iOS/wasm（只有解释器）性能的主要杠杆。

### 4.7 加载链路专项

- 每个函数在加载期大约被处理 15 遍：
  - SIGS 解码后再**克隆**进 Function（原件随即丢弃，本可直接 move）；
  - block label 用 `format!("block_N")` 生成后再哈希回下标；
  - STRS 池解码 3 遍；
  - packed zpkg 先拆成 116 个模块、各自重建局部池，再 merge 拼回去并二次 remap；
  - 二次 merge 后注册表和索引**全量重建**；
  - `resolve_module` 对入口模块**全部函数**预填 ResolvedTokens，副作用是提前加载包，而解释器和 JIT 本来已有首执解析；
  - `build_declared_candidates` 为拿到 NSPC 把每个依赖整文件读进来并解码整个池，不走 memo。
- z42.core：252 KB 文件 → 常驻约 3.5–4 MB（约 10 倍膨胀）、约 8 万次小分配【推断】；`z42c --help` RSS 43–48 MB【实测】。
- 惰性加载在 **lazy_loader 写锁内**做文件 I/O 和全量解码，期间所有线程的 lookup miss 都被阻塞。
- 每个 VmCore（包括每个 golden、每个 host module）都从头解码 z42.core。
- **改法**：
  - `PackageImage`：不可变，Arc 共享，进程级缓存；`ZpkgReader` 只解析一遍；函数体按需解码；
  - per-VM 的 `LinkState`（FuncTable / TypeTable / IC 侧表 / init 状态）；
  - 块和异常表改成 u32 下标，删掉 label 与 `block_index`；
  - `FunctionRt` 首执时再构建；`ResolvedTokens` 装箱；
  - 加载移出写锁：读锁下规划 → 锁外解码 → 写锁下发布。

### 4.8 builtin / IO / 反射专项

- `write_atomic` 不 park；编译缓存本来按 build_id/哈希校验，可进一步改成 PAL 的 `replace_atomic(path, bytes, Durability::{None, Ordered, Full})`，缓存用 None。
- fs 全部不 park；stdout 写不 park，管道写满时会挂住 GC；`builtin_file_read/write` 在**全局** file_handles 锁下做系统调用。
- socket/进程的每次调用都在 GC 堆上分配一个判别元组；文件读和 socket 读逐字节 `set_boxed`；进程输出同时生成字符串和逐字节装箱的数组，约 16 倍膨胀。
- 反射没有任何缓存：每次 `typeof` / `GetType()` 都新建一个 `Std.Type`；`GetMethods` 每次重建整张图；dotless 查找在**写锁**下克隆全部类型名；struct 反射用 387 行 Rust 复刻编译器的 StructLayout 算法。
- builtin 错误模型无类型（`anyhow::Error` → 一律 `Std.Exception`）：`Int32.Parse` 抛不出 FormatException；`TryParse` 用异常实现，失败路径极慢。

---

---

## 5. 框架设计（组件化 / 目录 / 扩展 / 维护）

### 5.1 分层现状与依赖环【确认，grep 计数】

- **metadata 是跨四层的 god-module**，里面同时装着：格式读取（formats/zbc_reader/TIDX）、IR（bytecode，以及属于 interp 优化的 superinstr）、对象模型（types/*，依赖 gc 46 处）、链接（loader/merge/lazy_loader/context）、派发缓存（resolver/ic）。还有向上依赖：vm_context 9–10 处、corelib 6 处、config 1 处。resolver 不是纯函数，它会加载包、排空初始化队列。
- **interp ↔ jit 成环**：interp→jit 12 处（OSR、divert，直接读 `JitModuleCtx` 内部字段）；jit→interp 41 处。`componentized-runtime.md` §4.2 计划的 crate 拆分会因这个环直接编译失败。
- **gc ↔ metadata ↔ vm_context 成环**：gc/ 之外引用 `crate::gc::*` 的有 corelib 68、metadata 48、vm_context 25；GcRef 的标记位格式、Value 判别值、"GC 不移动"的假设都烘焙进了 JIT 机器码。
- **VmCore 是上帝对象**：约 40 个字段，包括 7 个 OS 资源表（processes/threads/files/tcp/tls/udp…，按 target cfg 门控）。corelib/native 有 72 处直接访问 `ctx.core.<field>`。
- **进程全局可变状态**（与"VmContext 是唯一规范来源"相悖）：`HOST`、IO sink、`fs_backend::ACTIVE` 与 VFS、`LOADED_COMPRESSION`、repl 注册表、`VM_CORES`、`BUILTIN_INDEX`、`str_meta` TLS。同一进程里的两个 VM 会互相串扰，例如 `__vfs_enable` 会切换所有 VM 的 fs backend。
- **PAL 名不副实**：平台 cfg 在 PAL 外 123 处、PAL 内 13 处（corelib 85、vm_context 16、gc 13）。PAL README 写的"其余模块零 cfg"不成立。

### 5.2 interp / JIT 语义双写（已产生漂移）

| 语义 | 实现份数 | 已知分歧 |
|---|---|---|
| 字段 Get/Set | interp `exec_object.rs:264-474` ↔ JIT `object_field.rs:105-309`；JIT 内部 field_set 又写了 3 遍 | JIT 缺 PinnedView 分支；错误通道一边 `bail!` 一边抛裸 Str；访问 Str 上不存在的字段，两边报错文本不同 |
| ObjNew 整条 symres 判定 | 两份，各约 240 行 | |
| Array / Static / is-as / CallIndirect / ToString | 两份 | `jit_mk_clos` 在 OOM 时 `unreachable!` 直接 abort，interp 抛可 catch 的 OOM；Str+Str 拼接 JIT 用 `format!`，interp 用融合分配 |
| 原生调用序列（建帧 → push → 调用 → pop） | **10 份** | |
| 寄存器池 | 2 套 | 清 Null 的不变量相反 |

helpers 里有 59 处"镜像/对称"注释，多处记录的是已修过的分歧 bug。

**已经共享得好的部分**（可以作为模板）：`vcall_resolve`、`isa_td`、`semantics.rs` 标量运算、struct `*_val` 核心、IC 存储。

**改法**：抽出引擎无关的 `objops` 层：`field_get/set`、`array_*`、`obj_new(site_cache)`、`static_*`、`call_indirect_target`、`make_closure`。参数为 `&Value`，返回 `Result<Value, OpError{Internal | Throw(Kind, msg)}>`。两个引擎只负责寄存器适配和异常通道映射。

### 5.3 扩展性

- **加一个 builtin**：至少 3 处，通常 4–8 处：Rust 实现（手写实参解包）、表文件、z42 侧 `[Native]` 声明，视情况还有编译器硬编码名、allowlist、park、wasm cfg 桩、ext 胶水、文档。一致性检查只核对名字和 void，不核对 arity 和类型。
  - 建议：`#[builtin(name, blocking, pure)]` proc-macro 生成解包包装器、`BuiltinDesc{arity, params, ret, flags}`、按模块分组的 const 表和 `builtins.manifest`。编译器用 manifest 在编译期校验 `[Native]` extern；`blocking` 自动包 park，闭包拿不到 ctx，park 期间在结构上无法分配；`PURE` 标量 builtin 交给 JIT 做 intrinsic。这套描述符同时可以由 cdylib 通过 `z42_plugin_v1()` 导出，统一 ext 插件 ABI。
- **加一个 JIT helper**：6 处，签名不校验。建议用 `helpers!{}` 宏统一生成。
- **配置**：3 套装配实现（main 严格、`from_env` 宽松、z42-host 丢掉 `[properties]`）；同一默认值写在 3–4 处；C 入口 `z42_host_run_app` 和 wasm 都绕过侧车推导，与 `runtime-settings.md` 的声明矛盾。建议唯一入口 `config::assemble()` + 声明式宏；VmCore 持有 `Arc<RuntimeConfig>`，为多 VM 留出路线。
- **native**：Tier 1（libffi，约 1000 行 + 3 个 crate）**没有生产消费者**，却在默认 feature 里，并且每个 interp 帧都要付一次 `VmGuard` 的 TLS 写；ext 路径每个库手写胶水（compression 约 400 行），硬编码 `KNOWN_EXT_LIBS`，启动时急切扫描目录。建议把 feature 拆成 `native-ext`（只要 libloading）和 `native-ffi`（C5 落地前默认关闭）；改用函数表 ABI v2（`const Z42Api*`），与 repl 的 `ReplCallbacks` 同构，完全不依赖链接方式。

### 5.4 死代码与遗留（【确认】0 个生产引用）

- `Function.exec_mode`（解码了但从不读取）。
- ConcurrentMarkSweep 模式（已删除，见下「需 User 决策」）、finalizer 机制（零注册）、`collect()` 默认 no-op、死旋钮 `Z42_GC_THROTTLE_RATIO`。
- `thread/mod.rs` 空桩；z42-macros 的 `compile_error!` 桩仍从 z42-rs prelude 导出。
- `versions.rs` 大部分是 changelog 注释，而且已经漂移。
- AOT：`app.rs` 先做急切 BFS，`vm.rs` 再报错退出，两处报错文本还互相矛盾（LLVM vs cranelift）。
- 注释体量：GC 非测试代码 42% 是注释，大量是历史和实测叙述，文件常因触到行数上限而被机械拆分，而不是按内聚性拆。

### 5.5 建议的目标分层（自下而上、无环）

> ⚠️ User（2026-10-06）认为下面的目录结构不好。阶段 2 拆 crate 之前，先与 User 讨论分层与目录，不按此图直接实施。

```
format/      纯 reader → PackageImage（不可变、进程级共享、按需解码）
ir/          不可变 IR（无 serde、无运行期缓存）
object/      Value / 句柄 / 对象头 / 布局 / codec（只暴露 for_each_ref_slot）
link/        FuncTable / TypeTable / 惰性策略 / load context（副作用经 trait LinkEnv 反转）
dispatch/    IC / tokens（interp 与 JIT 共享）
objops/      引擎无关的对象操作语义（field/array/static/obj_new/call_indirect/vcall/isa）
heap/        GC（编译期具体类型 + 冷 HeapAdmin trait；safepoint 移到 thread/）
engines/     interp（预链接紧凑码）· jit（经 TierApi 注册槽接入，不被 core 静态依赖）
host/        Rust 原生 host::core API + 薄 C 壳
stdlib-*/    独立 crate：os（fs/process/platform）、net（rustls 可选）、repl-host；
             通过 BuiltinDesc 描述符表静态注册，也可编成 cdylib 插件复用同一份描述符
z42vm-cli/   clap / tracing-subscriber（不再是 lib 依赖）
```

---

---

## 6. 打包与嵌入的遗留项

z42vm 继续静态链接 VM，不改为动态链接 `native/libz42`（结论与重新考虑的触发条件见 [packaging.md](internals/src/devinfra/packaging.md)）。SDK 瘦身、dylib 元数据（`@rpath` / SONAME / `z42.dll.lib`）、glibc 下限已修。剩下的：

1. **嵌入路径比 z42vm 慢 1.26–1.55 倍**（分配器能解释大约 2/3）：给 staticlib/cdylib 打包构建加 lib 内 mimalloc feature；剩余差距需要 profile。影响面：self-contained、iOS、Android、wasm。
2. libz42.a 导出 69 个 `jit_*` 和 37 个 `ffi_*` 全局符号，嵌入者容易撞名。
3. clap / tracing-subscriber 拆进 `crates/z42vm`，`env-filter` 换成 `Targets`（省 0.3–0.45 MB）；rustls 加 feature 开关。
4. z42-compression 没做 LTO；macOS 部署目标不一致。
5. Android `opt-level="z"`；打包吞掉错误（nightly Android 包里 compression 确实缺失）。
6. apphost 用 spawn + wait 而不是 exec，子进程被信号杀死时退出码变成 1。
7. PGO 机会：70 KB 的 `exec_function_body`。
8. **Tier-1 native 扩展改用函数表 ABI**：扩展要调全局符号 `z42_register_type`，而 z42vm 不导出它；函数表 ABI 让这个问题消失（与 §5.3 的 native 建议同一件事）。

---

## 7. 迭代计划与进度

> 状态：⬜ 待做 · 🟡 进行中 · ✅ 已合并（附 PR 号）· ⏸ 需 User 决策。每项一个 PR（大项拆成多个），各自过完整 GREEN（`./xtask test` + `./xtask test runtime`；纯文档 PR 走 docs 快速通道）。
> 推进方式（User，2026-10-06）：阶段 0 完成后开始阶段 1。fix / perf / refactor 项照阶段 0 的方式推进（PR → GREEN → auto-merge → 下一项）；**带设计取舍的项先开 draft PR 写方案**（Why / Spec / Design / Scope），User 确认后再实施。命中 workflow.md「批量授权模式」的中断条件即停下询问。
> 本表在阶段收尾时用一个文档 PR 统一更新，单个 PR 不改本表（避免相邻行冲突）。

### 阶段 1：身份与调用

| ID | 类型 | 内容 | 验证要点 | 状态 |
|---|---|---|---|---|
| P1-0 | docs | 当前代码重测基线：z42c 构建剖面、§1.1 微基准、13_gc_large_heap 两种模式、hello 启动；写进 §1.5，据此复核下面的排序 | — | ⬜ |
| P1-1 | fix | 缺陷批（§3 D1、D2、D3、D6，各一个 PR）：<br>• D1 cctor 等待改 Condvar + park<br>• D2 socket 改 `Arc<TcpStream>` + closed 标志（UDP / TLS 同构）<br>• D3 `is_exception_subclass` 改调 `isa_td`<br>• D6 先写出复现，再修 | 每项先写失败测试 | ⬜ |
| P1-2 | vm | **进程级函数 / 类型身份**（§4.1，先出方案）：<br>• VmCore 上 append-only `FuncTable` / `TypeTable`，读无锁；惰性加载的函数逐个追加 FnId<br>• PIC、`method_tokens`、ObjNew / CallIndirect / FuncRef / ConstStr 站点缓存改存 FnId / TypeId，IC 能缓存 Lazy 目标<br>• 三套函数注册表、两套类型注册表合一<br>• D5：`isa_cache` 按 TypeId 做 key | 跨包 VCall 微基准；z42c 剖面里 `try_lookup_function` / `LazyTable` 消失；lazy 与 merged 函数行为一致 | ⬜ |
| P1-3 | refactor | **引擎无关的 `objops` 层**（§5.2）：field get/set、array、obj_new 先行；两引擎只做寄存器适配与异常通道映射；统一错误通道，空引用抛 `NullReferenceException` | interp 与 JIT 对同一组用例逐条一致（含错误文本）；删掉对应的双写实现 | ⬜ |
| P1-4 | perf | **字段访问**（§4.3，依赖 P1-2、P1-3）：<br>• FieldIC 载荷 `TypeId \| offset \| kind`，`field_access` 挪进 TypeDesc 热区<br>• 静态字段改为稳定地址的 cell + seqlock，去掉 VmCore 全局 `Mutex<Vec<Value>>`<br>• JIT 原生单态字段 IC；hoist 只用于循环内的不变接收者 | 字段微基准；z42c 剖面里 JIT 字段 helper 下降 | ⬜ |
| P1-5 | vm | **调用协议 v1**（§4.2，先出方案）：瘦 `VmFrame{*const Function, regs, pc, base}`，名字 / 行号在生成栈回溯时懒算；`call_stack` owner-only；寄存器池进 VmContext；guard 只在引擎入口安装；JIT `regs_ptr` 按偏移 load、`Ret` 直写 `frame.ret` | 栈回溯与异常栈文本不变；10_mono_vcall / fib 微基准；跨线程 GC 扫栈测试 | ⬜ |
| P1-6 | vm | **JIT 每线程上下文，worker 线程跑 JIT**（§4.5 第 1 条，先出方案）：每线程状态放进 JitFrame 或作为参数传入，`JitModuleCtx` 挂到 VmCore | z42c `--jobs 4` 墙钟；cross-thread 测试在 JIT 下全绿 | ⬜ |
| P1-7 | fix | **GC 内存复用与年轻代根治**（§4.4，先过模型 D）：TLAB 复用半活 chunk 的死槽、sweep 时就地释放 payload；年轻代归 minor 管、晋升时 promote-black | `tests/gc_incremental_model.rs` 模型 D 穷举；13_gc_large_heap 分代不劣于 STW；RSS 下降 | ⬜ |
| P1-8 | vm | **Builtin ABI v2**（§5.3，先出方案）：`#[builtin]` 宏 + manifest、类型化 `BuiltinError`（`Int32.Parse` 抛 FormatException）、`blocking` 标志自动 park、实参不经 Vec、`[Native]` 桩折叠 | 编译器用 manifest 校验 `[Native]` 声明；错误类型测试 | ⬜ |

**依赖与顺序**：P1-0、P1-1 先做；P1-2 是 P1-4、P1-6 的前置；P1-3 在 P1-4 之前（字段语义先收成一份再改载荷）；P1-5 不依赖 FnId（帧里存 `*const Function` 即可），帧管理是当前最大单项（§1.5），可与 P1-2 并行；P1-7、P1-8 与其余项独立。

### 阶段 1 已定方案（User，2026-10-06）

调研与论证细节见已关闭的方案 PR（P1-5 #1137、P1-2 #1139、P1-7 #1140）；这里只记决定和实施序列。

**P1-5 调用协议 v1**（先于 P1-2 做）
- 决定：
  - 崩溃信号转储只完整打印崩溃线程的栈，其他线程只打帧数。
  - 三个 arena 改为只由所属线程访问，另立一项，不放进本项。
- 序列：
  - PR-0：retention 诊断在停世界窗口里查询。
  - PR-1：原生调用序列收成 `jit::invoke::call_native`，解释器帧登记收成 `enter_frame`。
  - PR-2：瘦 `VmFrame{func, regs, pc, …}`，名字和行号在生成回溯时由 func + pc 现算；删掉解释器调用时的行号二分；JIT helper 少传两个参数。
  - PR-3：`call_stack` 只由所属线程访问，跨线程只经 `scan_parked`。
  - PR-4：合并两套寄存器池、guard 移到入口、frame_id 惰性分配。
  - PR-5：JIT `regs_ptr` 按偏移 load，`Ret` 直写。

**P1-2 进程级函数 / 类型身份**
- 决定：
  - FuncRef / Closure 在创建时就绑定 FnId，接受目标包加载和名字解析错误提前到 LoadFn / MkClos 时发生。
  - 惰性函数与 merged 函数对称：按阈值计数后升层编译。
  - TypeId 保持进程全局，用稀疏分段表承载。
- 序列：
  - 1：`SegVec` + `FuncTable` + FnId 登记。入口函数的 FnId 等于 `module.functions` 下标；惰性函数在包登记时分配，first-wins。
  - 2：解释器 Call 的 `method_tokens` 改存 FnId。
  - 3：JIT 槽位按 FnId 建、带负缓存；惰性目标也路由到 native；OSR 支持惰性函数。
  - 4：VCall PIC 改存 FnId。
  - 5：ObjNew 站点缓存。🟡 已实现、待合并（分支 `vm/objnew-site-cache`）：站点存类描述符 `Arc<TypeDesc>` + ctor 的 FnId，interp 与 JIT 共用 `interp/obj_new_resolve.rs`；命中不哈希、不拿锁。仍按名：回落描述符的本地类（每次现建描述符）。
  - 6：TypeTable 与 `isa_cache` 改 key（顺带修 D5）。🟡 已实现、待合并（分支 `vm/type-table`）：VmCore 上 `TypeTable`（进程级 TypeId → 最新版本描述符，稀疏分段、读无锁；入口模块构造时登记，惰性包在加载 / fixup 后发布）；`isa_cache` / `subclass_memo` 改按 `(接收者 TypeId, 目标键)` 做键，目标键缓存在指令 / 异常表行的 `TypeKeyCell`（已登记类型取其 TypeId，否则为名字保留一个 id）。未做：两张类型名字表（`Module.type_registry` / 加载器 `type_registry`）并入 TypeTable；回落描述符与 corelib 原生句柄单例仍无 id、不缓存。
  - 7：ConstStr 改为每 ctx 一张无锁表。
  - 8：FuncRef / Closure 改存 FnId。
  - 9：清理。
- 不改 `.zbc` / `.zpkg` 格式。

**P1-7 GC**
- 决定：
  - 选 B1：删掉 `keep_major`，minor 只认自己能到达的；epoch 戳保留，作「周期内出生」的标签。
  - 不做「sweep 时释放 payload」的过渡方案。payload 的根治随对象模型改造（阶段 2：payload 并入 GC 槽）。
  - 池超阈值时 decommit。
  - 软上限和 `Z42_GC_MAX_BYTES` 改按真实 footprint 计。
- 前置验证：
  - 模型 D 判定 B1 安全，现行策略在活性断言上给出反例（#1146）。
  - 当前 main 关掉 `keep_major`、用 `Z42_GC_SLICE_MS=0.05` 跑 z42.net 全套 5 轮，每轮 50/50。历史记录里的挂死已不再出现。
- 序列：
  - B1：删 `keep_major`，并加断言「Sweeping 期间晋升的条目必须带 epoch」。
  - A2：定长区续用 TLAB 尾巴，复用半活 chunk 的空洞（按槽的 claimed 位图）。
  - A3：变长区复用空洞。
  - A4：decommit（定长区按页对齐，记录 generation 下限）。
  - A5：committed 计数、`stats()` 降为 O(1)、软上限按 footprint。

**对象模型（M10 + M11）已定方案**（User，2026-10-07）
- 目标：堆上每个可变单元 ≤ 8 B、用同宽原子读写，去掉每对象 Mutex 后不会读到「一半标签 + 一半指针」；16 B `Value` 只留在寄存器。
  - 原始类型字段保持原宽度；引用字段改为 8 B 自描述指针（低 3 位记种类，0 = null），顺带去掉字符串 / 闭包字段的 16 B 侧表；泛型 `T` 字段仍是 16 B，但标签在分配时按实例化类型写死，之后只原子地改 8 B 负载。
  - 对象搬进变长区（16 B 紧凑头 + 按大小分档的空闲表，顺带解决空洞复用）；`lock` 语义移到 Monitor 侧表（首次加锁才膨胀，GC 停世界时清理）；identity hash 仍按地址。
  - 不改字节码格式（编译器的对象布局已给每个引用字段留了 8 B）。
- 决定：
  - 内存序：引用字段 release 写 / acquire 读，原始类型字段 relaxed。用户可见的变化写进文档：没有同步的跨线程读写不再按对象串行化。
  - ConcurrentMarkSweep 在 R4 之前删除。
  - `Volatile`、`Interlocked`、`lock(obj)` 以后再对语言暴露（Monitor 侧表做好之后）。
  - 编译期不知道 `T` 的擦除泛型数组（MIXED 模式）先用全局分片锁兜底，M13 之后去掉。
  - M13（泛型 `T[]` 按原始类型打包）排在 M10 / M11 之后。
- 序列（每步单独过 GREEN；小对象现约 104 B）：
  - R0：P1-3 先行，把两个引擎的字段 / 数组 / 静态字段读写收进 `objops`（`src/runtime/src/objops/`）。
  - R1–R3：字段与数组元素改用同宽原子；引用改为 8 B 自描述指针；数组分 REF / PRIM / MIXED 三种模式（每个字符串字段 −16 B，`object[]` 每元素 −8 B）。
    - R1 🟡：对象字段单元。基元同宽 relaxed；直接引用字段（含 `string` / `object` / 接口 / 委托）一律 8 B 自描述字（低 3 位种类，0 = null，release 写 / acquire 读，标记期写改 swap 供 SATB）；擦除泛型把基元写进 `object` 字段时装进单元素盒子（种类 7）；GC 只经 `visit_refs`；JIT 引用读改为 acquire load + 种类查表，`string` 字段也走内联。型参字段、内联 struct 引用叶子、合成布局仍在 16 B 侧表。实测（100 万个活对象，含 `object[]` 容器）：1 个 string 字段 GC 用量 88 → 72 B/个、RSS 160 → 122 B/个；2 个引用字段 112 → 80 B、168 → 153 B；z42c 工作区构建峰值 RSS 829 → 805 MB，墙钟持平。
    - R2：型参字段的 16 B 单元在分配时按实例化写死标签（基元实参 → 基元标签；引用 / 未知实参 → 引用字），之后只原子改 8 B 负载；类型不符的写入（如 `T = int` 收到 null）的处置要先定。
    - R3：数组元素模式 REF（8 B 引用字）/ PRIM（打包基元）/ MIXED（擦除 `T[]`，16 B + 分片锁）；struct blob 的引用叶子同步改 8 B，对象内联 struct 的叶子随之出侧表。
  - R4：删掉每对象 Mutex（88 B），顺带修掉 D4（JIT 绕过锁）与 `Array.Copy` 的 ABBA 死锁面。
  - R5：Monitor 侧表。
  - R6：对象搬进变长区（56 B；最大、风险最高的一步）。
  - R7：侧表换成位图（即 M8，40 B）。
  - R8：对象头 24 → 16 B（32 B，与 .NET 持平）。
  - R9：数组头与元素合成一块（`long[8]` 250 → 约 96 B）。

**需 User 决策**（⏸）：
- ~~删除 ConcurrentMarkSweep~~ —— 已定（User 2026-10-07：删）并已删除。`gc-mode` 只剩 `stw` / `generational`（及 `-mark-sweep` 别名）；传 `concurrent` 按非法枚举值处理：`--set` 致命（退出码 2），环境变量 / 配置文件警告并回落默认 `generational`，错误信息列出合法取值。
- 可 catch 的栈溢出（当前是致命错误；前置条件见 vm-architecture.md「原生栈预算」）。
- `int` 等窄整数的算术溢出不回绕到本宽度：`int.MaxValue + 1`、`int.MinValue / -1` 两路都得 `2147483648`，值仍按 i64 存、运算后不截断。要不要按声明宽度回绕、在哪一层截断（编译器插 Convert，还是 VM 按类型运算），属于语言语义决策。
- 栈闭包（D6）——**已决（User，2026-10-07）：删除运行时支持**。z42c 的逃逸分析从不给 `MkClos` 标栈分配，栈闭包路径整条不可达；`Value` 的栈闭包变体、帧 `env_arena`、GC 对它的扫描、`CallIndirect` 分支都已删除，闭包恒堆分配。以后要消掉不逃逸闭包的分配，走 JIT 标量替换，不再加值变体。
- GC 退避上限与「周期内的 minor 不判徒劳」（B1 落地后先重测，可能不再需要）：吞吐换停顿。13_gc_large_heap 分代模式下最大停顿 73.5→50 ms、p99 58.5→24 ms、不再出现 x64，代价是墙钟慢约 30%（`--large` 2.4 倍）。

**阶段 1 预期**【推断】：z42c 构建再快 1.3–1.5 倍（字段约 10%、帧约 10%、跨包 VCall，加上并行阶段吃到 JIT）。

### 内存 / GC 持续优化（User，2026-10-07：内存占用和 GC 离其他脚本语言差距还很大，持续发掘）

依据 §1.6，按收益排序。M 前缀与 P1-7 的 GC 方案互补。

| ID | 杠杆 | 预期 | 依赖 | 状态 |
|---|---|---|---|---|
| M1 | 数组直接在 GC 块里零初始化，去掉临时 `vec![Value; n]` | 每元素峰值 −16 B；`List<int>` 77.6 → 约 50 B | — | 🟡 |
| M2 | 数组元素类型名不再每个数组一份 `Arc<str>`；压缩 ArrayBacking | 每数组 −24~−40 B | — | 🟡 |
| M3 | `"k" + i` 直接格式化，不产生 ToStr 临时串 | 每次拼接少 40 B 垃圾 | — | 🟡 |
| M4 | `stats()` 不再收集全部活对象 | 观测不再额外占用 16–32 B/对象 | — | 🟡 |
| M5 | chunk 大小贴合分配器档位 | 每对象、每数组 −8 B | — | 🟡 |
| M6 | 按真实占用记账，软上限 / `Z42_GC_MAX_BYTES` 按真实占用判定（即 P1-7 A5） | 上限真正约束 RSS | — | 🟡 |
| M7 | 池中空 chunk 超阈值 decommit（即 P1-7 A4） | 稳态 RSS 下降 | — | 🟡 |
| M8 | 用位图 / 区间替代 young_list、all_blocks、var young 等侧表 | 分代模式每对象 −8 B、每串 −16 B；实测（分代）每对象 98 → 81 B、每串 115 → 89 B，槽头 72 → 64 B，z42c 构建 RSS 788 → 754 MB、墙钟持平；09 STW full mark +10%（64 B 步长，待查） | — | 🟡 |
| M9 | 年轻集合增长加上限（futility 时提前晋升或转 major，而非放大 nursery） | 流失场景 RSS −13%，最大停顿 94 → 18 ms | 09_alloc_ctorless 吞吐退化（⏸） | ⏸ |
| M10 | 字段 payload 内联进 GC 槽 | 每对象 −16~−24 B；死 payload 随槽一起释放 | — | ⬜ |
| M11 | 对象头 72 → 约 24 B：去掉每对象 Mutex，标记 / 存活 / 年龄 / 代号合成一个字，稀有字段移到侧表 | 每对象 −40~−48 B | 内存模型已定（见「对象模型（M10 + M11）已定方案」），按 R0–R9 推进 | ⏸ |
| M12 | 空洞复用（TLAB 认领半活 chunk） | 流失场景 RSS 降 2–4× | M10 | ⬜ |
| M13 | 泛型 `T[]` 的实参为原始类型时按类型打包 | `List<int>` / Dictionary 值每元素 16 → 4 B | 已定排在 M10 / M11 之后；编译器配合 | ⏸ |

预期：M1–M8 落地后，对象约 85 B、数组约 190 B、`List<int>` 约 50 B；M10 + M11 之后，对象约 35–40 B（约 .NET 的 1.3×）；再加 M12，流失场景的 RSS / 活集从约 8× 降到 2–3×。

### 对标差距（User，2026-10-07：全面对标，争取各方面都有不错的表现）

跨语言对标基准今后放在独立仓库 z42-lang/Benchmarks，届时系统补充；下表是本地一次测量的结果（24 条负载，M2 Ultra，load 4–9，z42 jit + 分代 GC，对照 Python 3.9 / Node 26 / Ruby 2.6 / .NET 10 / Java 21）。先处理连 CPython 都不如的几项：

| ID | 负载 | z42 / Python | z42 / Node | 初步判断 | 状态 |
|---|---|---|---|---|---|
| T1 | json | 28× | 40× | 待剖析 | 🟡 |
| T2 | str_builder / str_split_join | 7.9× / 13× | 11× / 10× | StringBuilder 是纯脚本实现，每次 Append 生成一个 GC 字符串 | 🟡 |
| T3 | dict_ops / sort | 4.8× / 3.7× | 7.3× / 7.4× | 待剖析（泛型装箱、比较器调用、哈希路径）。**Dictionary / HashSet 按插入顺序遍历（User 2026-10-07）**：改为 CPython 式插入有序紧凑哈希表（稠密 entries + `int[]` 索引表，首槽取 hash 低位、冲突后 perturb 探测）；dict_ops n=2M 2.02 → 1.75 s、RSS 286 → 165 MB，n=5M 超过 240 s → 4.7 s；sort 未动 | 🟡 |
| T4 | large_heap | 1.6× | 5.9× | GC 策略与内存复用（P1-7 / M 系列） | 🟡 |
| T5 | closures | 1.3× | 8.3× | 闭包调用按名（P1-2 PR 8） | ⬜ |

其余负载：
- 启动是所有运行时里最快的（11 ms、12 MB），4 线程能真正并行。
- 数值计算比 Python 快，但比 Node 慢 2–25×。
- 虚调用比 Node 慢 5–6×。

写对标程序时撞到的语言 / 库缺口：
- 局部变量 `C[][] x = …`（元素为用户类）解析失败；
- `new T[n][]` 不支持；
- double 没有定点格式化，插值串没有格式说明符。

### 阶段 2：紧凑执行与对象模型（1–2 个月，需另行确认）

第 5 项拆 crate 之前先与 User 讨论目录分层（§5.5）。


1. 解释器预链接紧凑码（u16 寄存器 / 16 B op / 偏移跳转 / 内嵌 IC 槽 / quickening / 尾调用分发）。
2. 每线程连续 VM 栈，interp 与 JIT 共用；JIT→JIT 直接调用；JIT 代码 arena；批量或后台编译；类型化 SSA 寄存器。
3. 对象模型：去掉每对象 Mutex（内存模型已定，按 §7 的 R0–R9 推进）、16 B 统一头、string 内联、单次分配、8 B 引用数组元素。
4. `PackageImage` + `LinkState`；metadata 分层；TierApi 注册槽解开 interp↔jit 环；GC 改为编译期具体类型。
5. 拆 crate：core / stdlib-os / stdlib-net（rustls 可选）/ z42vm-cli。

### 阶段 3：长期

精确 stack map + 移动式 nursery；用已 park 的 mutator 线程做并行标记；AOT 复用 JIT 翻译核（前提：改用符号化常量）；按 packaging.md 记录的触发条件再评估动态链接 / 可选 JIT。

---

---

## 附录：测量方法

- **微基准**：先 `z42c --emit-zbc src/bench/scenarios/<n>.z42 <n>.zbc --opt-all`，再 `z42vm --mode jit|interp <n>.zbc <Namespace>.Main`，取 5 次中位数。
- **真实负载剖面**：
  1. 在 `src/compiler` 下执行 `z42c build --workspace --release --output-dir <tmp> -q`，事先把 stdlib 的 zpkg 拷进 `<tmp>`；
  2. 运行期间用 macOS 的 `sample <z42vm pid> 18 -file out.sample` 采样；
  3. 按函数名统计包含时间（对每条调用链只计最外层匹配）。
  - `Z42_PORTABLE_VM=<z42vm>` 可以让同一个 z42c 跑在指定的 VM 上，便于新旧对比。
- **GC**：`Z42_GC_TRACE=1`（每次回收一行）、`Z42_GC_PHASES=1`（trip 行带退避倍数 `xN`），`--set gc-mode=stw|generational`。
- **惰性加载**：`Z42_LOG=z42::metadata::lazy_loader=debug`，统计 `lazy-loaded zpkg` 行数。
- **启动**：`/usr/bin/time -l`（RSS），用 Python `subprocess` 循环计时取中位数。
