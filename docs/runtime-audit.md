# z42 运行时审计与改进计划（临时迭代底稿）

> **临时底稿**：运行时改进迭代的工作文档，不属于三本书。阶段 0 已全部完成（修复的知识已上浮到 internals / reference，本文不再保留）；本文现在是**阶段 1** 的工作文档。迭代全部完成后删除本文件（同时移除 `docs/README.md` 里的入口链接）；届时仍未完成的项转入 `docs/internals/` 对应机制页的「待办 / Deferred」（类别登记见 协作规范的文档总纲）。
> 范围：`src/runtime`。方法：按子系统（值模型/IR、加载链路/启动/配置、解释器、JIT、GC、corelib/native/host/VmContext、构建打包）逐个只读审计 + 实测（release z42vm，macOS arm64：`sample` 采样、GC trace、计时）。
> 证据级别：**【实测】** profile / trace / 计时（方法见附录）；**【确认】** 读代码确认；**【推断】** 有代码依据但需测量。
> 计时环境负载较高，绝对值可能偏高 20–40%，相对比例可信。文中行号对应审计时的版本，随修改漂移。

---

## 0. 结论速览

1. **头号系统性问题：运行时的"身份"全程按名字。** 函数/类型/字段/方法从解码起就被还原成 `String`，运行期再靠一堆哈希表和旁路缓存"重新压缩"。wire 格式本来是紧凑的下标与 token，加载器先解压成字符串、执行时再哈希回去。JIT 字段 helper 按名解析在 z42c 构建里占 **约 10%**。惰性包函数没有整数身份，导致 PIC 永远缓存不了跨包目标，而 z42c 的代码全在惰性包里（§4.1）。
2. **调用协议是解释器与 JIT 共同的天花板。** 每次调用约 11 次原子 RMW、3 次加锁、4 次 TLS、2 个 `Arc<str>` 引用计数，JIT 还要经 4 次 helper。单态虚调用 JIT ≈52 ns、解释器 ≈94 ns；fib 递归每次调用 ≈140 ns【实测】。帧管理在 z42c 里占 **约 10%**（§4.2）。
3. **对象模型与 GC 的内存开销是 .NET 的 2.5–3 倍，RSS 是 GC 记账堆的 4.5–6 倍。** 72 B 头（含每对象一把 Mutex）+ 单独 malloc 的负载；TLAB 不复用死槽。默认的分代 + 增量模式在大堆场景下**比纯 STW 更慢也更大**【实测】（§4.3、§4.4）。
4. ~~**JIT 在结构上只能单线程。**~~ ✅ P1-6 已解决：worker 线程照常分层、编译、跑原生码，z42c `--jobs 4` 构建 −19%（§4.5、§7）。
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

### 1.7 阶段 1 收官重测（2026-10-18）

main `6723c6a50`（阶段 1 全部条目已合并：P1-2 函数/类型身份、P1-4 字段访问 #1260、P1-5 调用协议 v1、P1-6 JIT 每线程 #1238、P1-7/M 系列 GC 与内存、P1-8 前三 PR、对象模型 R0–R9、M13 泛型数组打包）。release z42vm，macOS arm64，微基准 7 次中位、hello 15 次、z42c 构建 5 次；测量时 load 4–6（微基准）与 15–16（构建，墙钟偏高 20–40%、相对比例可信；P1-0 时 load 10–14，同为高负载窗口，两者的构建数字可对照）。

**微基准**（对照 §1.1 阶段 0 前与 §1.5 的 P1-0；墙钟 ms，jit / interp）：

| 场景 | 阶段 0 前 | P1-0 | 收官 | 累计 |
|---|---|---|---|---|
| 01_fibonacci | 45.7 / 52.7 | 40.0 / 45.1 | **23.1 / 28.7** | 2.0× / 1.8× |
| 04_arith_loop | 52 / 175 | 52.7 / 158.9 | 54.7 / 153.8 | 持平（纯数值循环，JIT 早已有数） |
| 05_polymorphic_dispatch | 570 / 1130 | 577 / 1077 | **281 / 868** | 2.0× / 1.3× |
| 09_alloc_ctorless | 473 / 500 | 456 / 503 | **131 / 248** | 3.6× / 2.0× |
| 10_mono_vcall | 1045 / 1902 | 990 / 1818 | **434 / 1107** | 2.4× / 1.7× |
| 11_type_test_chain | 574 / 1031 | 415 / 1008 | **332 / 861** | 1.7× / 1.2× |
| 12_gc_churn | 533 / 726 | 515 / 796 | **192 / 452** | 2.8× / 1.6× |
| 13_gc_large_heap 分代 jit | 3.0 s（RSS 991 MB，停顿 66 ms） | 2.97 s | **943 ms，停顿 7.5 ms** | 3.2×，停顿 ÷8.8 |
| 13_gc_large_heap STW jit | 3.0 s（RSS 1182 MB，停顿 111 ms） | 2.16 s / 799 MB | **763 ms，停顿 27 ms** | 3.9×，停顿 ÷4 |
| hello | 10.2–11.6 ms | 12.0 ms / 12 MB | **10.0 ms / 13 MB** | 启动仍是同类最快之一 |

分代与 STW 的关系已反转：13 场景分代比 STW **快**（943 vs 763 ms 墙钟同量级、停顿 7.5 vs 27 ms）且 RSS 更低——§1.5 时「分代比 STW 慢 45%、RSS 多 25%」的问题（B1 + M8/M9 + R6–R9 + 回收闸门按条目）已解决。

**z42c 构建编译器工作区**（对照 §1.5 的 P1-0；`--no-incremental`，产物一致）：

| 口径 | P1-0 | 收官 | 变化 |
|---|---|---|---|
| `--jobs 1` | 16.8–17.3 s，RSS 0.88–0.92 GB | **12.95 s，287 MB** | 墙钟 −25%，RSS ÷3.1 |
| 默认并行 | 12.4 s，RSS 0.96 GB | **7.49 s，341 MB** | 墙钟 −40%，RSS ÷2.8 |

（同轮交错对照的更长序列：P1-4 时 11.51 s → R8 期 6.38 s → M13 前 7.60 s → 收官 7.49 s，负载各不相同只看量级。）「阶段 1 预期：z42c 构建再快 1.3–1.5 倍」**达成并超出**（jobs=1 约 1.3–2.2×、并行约 1.7×，视负载窗口），RSS 的改善（÷3）远超预期。

**内存**（各条目实测的汇总，密度 = (RSS(N)−RSS(0))/N，100 万个装 `object[]`）：小对象 STW 空类 65.6 → **49.4**、`{int; string}` 81.6 → **65.6**（R8）；数组 `long[8]` 181.3 → **138.3**、`object[4]` 171.4 → **80.7**（R9）；`List<int>` 每元素 16 → **4 B**、`Dictionary<string,int>` 总量 −20%（M13）。§1.6 里「小对象 ≈ 100 B、流失场景 RSS/used 8×」的画面已分别压到 49–66 B 与 2–3×（M12 空洞复用 + R8 后续条目闸门）。

**收官后剖面头部**（12 s sample，质量有限——窗口混入驱动等待，只列可辨项）：safepoint 慢查 18%、`jit_obj_new` 18%、vcall 派发 17%。按名字的调用 / 字段路径在 z42c 剖面里已不显著（P1-2/P1-4 的收益）；「身份按名字」剩下的主要残余是三套函数注册表合一的清理与 PIC 未覆盖的反射侧。

---


---

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
| D4 | 每对象 Mutex 被 JIT 绕过（已随对象模型 R4 消除） | 【确认】JIT 在 hoist 时取出 bytes 指针即释放锁，之后裸写 | — | R4 删掉了每对象 Mutex：所有引擎对字段 / 元素的读写都是同宽原子单元访问，JIT 内联码与解释器、helper 走同一套内存序，没有「绕过锁」可言；`Array.CopyRange` 不再取锁，ABBA 死锁面随之消失 |
| D5 | `isa_cache` / `subclass_memo` 以地址为 key | 【确认】`vm_context/isa_cache.rs` 以 `*const TypeDesc` 与类名字符串地址为 key，依据是"元数据在 VM 生命周期内不朽" | | 可回收 load context 被回收后不失效，地址复用可能误判【推断】；随 TypeId（P1-2）改为按 id 做 key |
| D6 | OSR 后栈闭包 env 丢失（已随栈闭包删除而消除） | 【确认】z42c 从不给 `MkClos` 置栈分配标志，栈闭包路径不可达 | — | 已决（User，2026-10-07）：删除运行时栈闭包支持。`Value` 不再有栈闭包变体，interp `Frame` / `JitFrame` / `VmFrame` 不再有 `env_arena`，闭包恒堆分配；OSR 交接只有寄存器，无可丢的 env。zbc `MkClos` 的尾字节保留、VM 读后丢弃，下次 zbc bump 删除 |

---

## 4. 性能：根因与改法（按收益排序）

### 4.1 根因一：身份按名字（z42c 中约 25% 以上可直接消除）

**现象**【确认 + 实测】：
- **cctor 屏障**：属主判定已缓存在 `Function.owner_init`（调用屏障）和静态字段单元旁的属主缓存（静态字段屏障），自由函数与无 cctor 的类型永久免检。剩下的是 JIT 仍经 helper 判定，没有在生成码里内联代际检查。
- **惰性包函数没有整数身份**：VCallIC 载荷是入口模块 `functions` 下标，所以 Lazy 目标**永不进 PIC**。每次跨包虚调用都要重走：TypeDesc Arc clone、vtable 线性字符串比较、func_index miss、`try_lookup_function`（锁 + 哈希）；JIT 还要再查一次 `LazyTable`（全局 Mutex + SipHash）。z42c 的分派站点（Call 10252 / VCall 1245 / ObjNew 2116 / FieldGet 19539 / StaticGet 2070 / ConstStr 6231）**全部在惰性包里**。
- **ObjNew** 每次都哈希类型名和 ctor 名（resolver 算好的 `type_tokens` 没用于派发，`exec_object.rs:13-18` 自己承认）。
- **CallIndirect / FuncRef / 闭包**：每次 `name.to_string()` 分配 + 哈希；`LoadFn` 每次新分配一个 GC 字符串。（P1-2 PR 8 已改：创建时绑定 FnId，调用按 id。）
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

1. ~~**worker 线程不跑 JIT**~~ ✅ 已解决（P1-6，方案见 §7「P1-6」）：编译码与 helper 的上下文参数改为运行线程自己的 `VmContext`，共享 JIT 状态挂到 `VmCore.jit`，worker 线程照常分层、编译、跑原生码。
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
- 带 token 的指令要经 `site_index[b][i]` → 分类 Vec，两级依赖加载（`func.resolved` 由执行循环每次激活读一次，不再每条指令一次 acquire 读，见 R6 后续 ①）。
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
| P1-0 | docs | 当前代码重测基线：z42c 构建剖面、§1.1 微基准、13_gc_large_heap 两种模式、hello 启动；写进 §1.5，据此复核下面的排序 | — | ✅ #1133 |
| P1-1 | fix | 缺陷批（§3 D1、D2、D3、D6，各一个 PR）：<br>• D1 cctor 等待改 Condvar + park<br>• D2 socket 改 `Arc<TcpStream>` + closed 标志（UDP / TLS 同构）<br>• D3 `is_exception_subclass` 改调 `isa_td`<br>• D6 先写出复现，再修 | 每项先写失败测试 | ✅ D1 #1138、D2 #1142、D3 #1136、D6 #1188（删栈闭包） |
| P1-2 | vm | **进程级函数 / 类型身份**（§4.1，先出方案）：<br>• VmCore 上 append-only `FuncTable` / `TypeTable`，读无锁；惰性加载的函数逐个追加 FnId<br>• PIC、`method_tokens`、ObjNew / CallIndirect / FuncRef / ConstStr 站点缓存改存 FnId / TypeId，IC 能缓存 Lazy 目标<br>• 三套函数注册表、两套类型注册表合一<br>• D5：`isa_cache` 按 TypeId 做 key | 跨包 VCall 微基准；z42c 剖面里 `try_lookup_function` / `LazyTable` 消失；lazy 与 merged 函数行为一致 | ✅ PR 1–8：#1158 #1175 #1181 #1187 #1194 #1200 #1207 #1211 |
| P1-3 | refactor | **引擎无关的 `objops` 层**（§5.2）：field get/set、array、obj_new 先行；两引擎只做寄存器适配与异常通道映射；统一错误通道，空引用抛 `NullReferenceException` | interp 与 JIT 对同一组用例逐条一致（含错误文本）；删掉对应的双写实现 | ✅ #1184（即对象模型 R0） |
| P1-4 | perf | **字段访问**（§4.3，依赖 P1-2、P1-3）：<br>• FieldIC 载荷 `TypeId \| offset \| kind`，`field_access` 挪进 TypeDesc 热区<br>• 静态字段改为稳定地址的 cell + seqlock，去掉 VmCore 全局 `Mutex<Vec<Value>>`<br>• JIT 原生单态字段 IC；hoist 只用于循环内的不变接收者 | 字段微基准；z42c 剖面里 JIT 字段 helper 下降 | ✅ #1260（`field_access` 不挪热区、不用 seqlock 的取舍见已定方案段）＋ 后续 #1279（StaticGet/Set 内联、屏障放行进单元）、#1281（值类型静态字段零值） |
| P1-5 | vm | **调用协议 v1**（§4.2，先出方案）：瘦 `VmFrame{*const Function, regs, pc, base}`，名字 / 行号在生成栈回溯时懒算；`call_stack` owner-only；寄存器池进 VmContext；guard 只在引擎入口安装；JIT `regs_ptr` 按偏移 load、`Ret` 直写 `frame.ret` | 栈回溯与异常栈文本不变；10_mono_vcall / fib 微基准；跨线程 GC 扫栈测试 | ✅ PR-0–5：#1143 #1145 #1152 #1156 #1161 #1171 |
| P1-6 | vm | **JIT 每线程上下文，worker 线程跑 JIT**（§4.5 第 1 条，先出方案）：每线程状态放进 JitFrame 或作为参数传入，`JitModuleCtx` 挂到 VmCore | z42c `--jobs 4` 墙钟；cross-thread 测试在 JIT 下全绿 | ✅ #1238（方案见下方已定方案段，`--jobs 4` −19%） |
| P1-7 | fix | **GC 内存复用与年轻代根治**（§4.4，先过模型 D）：TLAB 复用半活 chunk 的死槽、sweep 时就地释放 payload；年轻代归 minor 管、晋升时 promote-black | `tests/gc_incremental_model.rs` 模型 D 穷举；13_gc_large_heap 分代不劣于 STW；RSS 下降 | ✅ B1 #1146 #1154；A4 / A5 #1177；A3 随 M12 #1258（变长区空洞复用）；A2 不再需要（R9 删掉定长区） |
| P1-8 | vm | **Builtin ABI v2**（§5.3，先出方案）：`#[builtin]` 宏 + manifest、类型化 `BuiltinError`（`Int32.Parse` 抛 FormatException）、`blocking` 标志自动 park、实参不经 Vec、`[Native]` 桩折叠 | 编译器用 manifest 校验 `[Native]` 声明；错误类型测试 | 🟡 PR1–3 已合（#1255 #1262 #1271：基础设施 + convert/io/thread + fs/io/process + net/tls/monitor，方案见下方已定方案段——manifest 方案已被 2026-10-12 决定取代）；**批次 4 已迁反射纯查询**（type_query 22 + enums 6：不执行 z42 代码的 28 个迁 `#[builtin]`，不压轻帧也不标 `reenters`；`Enum.Parse`/`GetGenericTypeDefinition`/`GetEnumUnderlyingType` 的错误从 Rust 抛 `ArgumentException` / `InvalidOperationException`——与 C# 一致，此前是 `Std.Exception`）；**批次 5 已迁反射合成类 #1322**（17 个：成员列举 / attribute 特性（标 `reenters`——factory 是用户 z42 构造）/ 泛型合成（`MakeGenericType` 违约 → `ArgumentException` 同 C#）/ `class_type_arg` / `class_default` / `methodof` / `GetType`）；**批次 6 已迁反射可执行类**（11 个，全标 `reenters`：method/invoke_static/activator/ctor/property·field get·set/module_load 2/`run_goldens_isolated`——被反射调用的 z42 代码抛出的异常经 `BuiltinError::Thrown` 原样传播，实测异常类型与消息不变）；**反射域旧表清零**；**批次 7 已迁 lctx / asm 域**（14 个：装载上下文与程序集的全部查询 / 合成 + `__lctx_load`（标 `reenters`）+ `__type_is_collectible` / `__type_assembly`——其余纯查询不压轻帧；`Unload` 的根拒绝 / 未知上下文 → `InvalidOperationException`）；**批次 8 已迁 repl 域**（5 个：`complete_probe` / `member_names` 标 `reenters`（执行 z42 回调 / `static_get` 可触发 cctor），回调 throw 经 `Thrown` 原样传播；`set_completer` / `set_key_editor` / `set_keywords` 纯配置不压轻帧）；剩余：ext 压缩库、string / math / object / gc 剩余旧表项、io 剩余（TestIO sink / `__concat` / `__len` / `__contains`），旧表迁空后删 `Native::Val`/`Void` 与 `PART1`/`PART2`（当前旧表 152 项） |

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
  - 5：ObjNew 站点缓存。✅（随 `vm/objnew-site-cache`）：站点存类描述符 `Arc<TypeDesc>` + ctor 的 FnId，interp 与 JIT 共用 `interp/obj_new_resolve.rs`；命中不哈希、不拿锁。仍按名：回落描述符的本地类（每次现建描述符）。
  - 6：TypeTable 与 `isa_cache` 改 key（顺带修 D5）。✅（随 `vm/type-table`）：VmCore 上 `TypeTable`（进程级 TypeId → 最新版本描述符，稀疏分段、读无锁；入口模块构造时登记，惰性包在加载 / fixup 后发布）；`isa_cache` / `subclass_memo` 改按 `(接收者 TypeId, 目标键)` 做键，目标键缓存在指令 / 异常表行的 `TypeKeyCell`（已登记类型取其 TypeId，否则为名字保留一个 id）。未做：两张类型名字表（`Module.type_registry` / 加载器 `type_registry`）并入 TypeTable；回落描述符与 corelib 原生句柄单例仍无 id、不缓存。
  - 7：ConstStr 改为每 ctx 一张无锁表。✅（随 `vm/conststr-per-ctx`）：落在每 VM（VmCore）一张 `StrTable`，不是每线程——GC 字符串属于 VM 的堆，字符串身份对程序不可见，读侧无锁后每线程一份已无收益。`ConstStr.idx` 即 VM 内稠密的字符串 id（入口池 `0..n`，惰性包的池注册时整块追加，id 永不复用；加载器的 `string_pool` 并入此表），槽位存文本 + `OnceLock<Str>`；interp 与 JIT 共用 `VmContext::const_str`，首执后命中不拿锁、不哈希、惰性包也只查一次。驻留串由表的 `roots` 列表作 GC 根。
  - 8：FuncRef / Closure 改存 FnId。✅（随 `vm/funcref-closure-fnid`）：`Value::FuncRef(FnId)`（4 B 载荷，`Value` 仍 16 B）、`ClosureData { env, fn_id }`；`LoadFn` / `MkClos` 在创建时经 `exec_call::fn_ref_id` / `bind_fn_ref` 把名字绑成 FnId（本模块 `func_index` → 惰性加载器，含加载目标包），结果存进站点 token `ResolvedTokens::fn_ref_tokens`（resolver 加载期按 `id_of` 预填），之后同一站点只读一次 token。`CallIndirect` 按 id 派发：interp `fn_by_id` + 经 `resolve_fn_by_id_tiered` 计数并转原生码，JIT `jit_call_indirect` 直接查槽位表；JIT 的 `LoadFn` 已绑定时内联成一次寄存器写。目标名解析不到时在 `LoadFn` / `MkClos` 处抛 `MissingSymbolException`（此前在调用时报内部错误）。删除 PR 7 的 `fn_name_cache` / `intern_fn_name`；`FuncRef` 不再是堆引用。`__delegate_eq` 按 id 比较，`__delegate_fn_name` / 显示从 id 取名字，`__make_closure` 走同一条绑定。
  - 9：清理。
- 不改 `.zbc` / `.zpkg` 格式。

**P1-6 JIT 每线程上下文**（User，2026-10-12：选最根本、以后不用再迭代的方案，不计改动量）
- 决定：
  - 编译码与所有 JIT helper 的上下文参数改为**运行线程自己的 `VmContext`**（HotSpot / .NET 的「线程寄存器」）。生成码按偏移读的字段收进 `VmContext` 上 `#[repr(C)]` 的 `JitHead`：本线程栈下限（栈底帧 push 时按驱动线程写入）、cctor 门指针，外加缓存的共享运行时指针；safepoint 计数仍在 `VmContext::safepoint_skip`，内联快路直接按偏移读写，少一次间接。
  - 共享的 JIT 状态挂到 `VmCore.jit: OnceLock<JitRuntime>`（每 VM 一个 JIT：槽表、`Mutex<LazyCompiler>` 与代码页、OSR 缓存、阈值、模块指针），随 VmCore 释放；删除 `JitModuleCtx` / `JitModule` 与 `VmContext::jit_ctx`。
  - 编译在**调用线程**上、编译锁内进行（谁先到阈值谁编，别的线程等锁）；批量 / 后台编译仍归 §4.5 第 3 条。等锁不 park（调用方可能手握只在 Rust 局部里的新对象），锁内不到 safepoint：可能加载包的 token 解析与 `chained_forward` 查找都挪到拿锁之前。OSR 缓存在锁内复查、写入。
  - 不改字节码 / 格式；行为上唯一的变化是 worker 线程跑原生码（不可翻译的入口在解释器上跑时，其调用的已编译函数也回跳原生码）。
- 安全性逐项核对：待抛异常、safepoint 计数与慢路径、帧栈登记（`call_native` 压进本线程帧栈）、寄存器池、三个 arena、计数器分片、`isa_cache` / 查找缓存都是每线程的；静态字段（VmCore 锁）、cctor 门（Condvar + park）、调用点缓存（`Function.resolved` 里的原子单元，本就由多线程解释器共用）是共享且已同步的；槽位经 `OnceLock` release / acquire 发布，发布前 cranelift 已 mprotect + 刷 icache + `pipeline_flush_mt`；worker 的 JIT 帧经 `scan_frames_parked` 与主线程一样被扫描。
- 实测（release z42vm，与合并基 fd500f4ac 交错，负载 9–28）：z42c 工作区构建 `--jobs 4` 10.72 → 8.63 s（−19%，7 轮中位），`--jobs 1` 13.29 → 12.99 s（持平），产物逐字节一致；`--jobs 4` 下 VM 实际编译 1144 → 2050 个函数（worker 也升层）。`06_thread_scaling`（每线程 5000 万次迭代）941 → 272 ms；01（fib 32）/ 05 / 10 单线程 0.97 / 0.96 / 0.99。

**P1-4 字段访问**（✅ #1260）
- 决定：
  - FieldIC 载荷 = `TypeId:32 | FieldLoc`，`FieldLoc` = `offset:16 | width:8 | kind:8`（kind：基元精确 `TAG_*` / 引用字 / 型参单元 / struct 根）。命中只比 TypeId 就做一次单元访问，不查名字、不读布局；型参单元有两个坐标，载荷改存字段下标、命中时回布局取负载字偏移。偏移超过 16 位的字段不缓存。
  - **不把 `field_access` 挪进 TypeDesc 热区**：命中路径已经不读布局（偏移和种类都在 IC 里），挪动只省未命中 / 型参单元 / GC 遍历时一次依赖 load，却要改 55 处 `TypeDesc` 字面量；留给 R8（统一头 + TypeDesc 由 arena 持有）一起做。
  - JIT 原生单态 IC：守卫 = 寄存器 tag 是 `Object` → `hdr = payload & HANDLE_ADDR_MASK` → `load [hdr+16]`（`Arc<TypeDesc>`）→ `load32 TypeDesc::id` → 与站点 IC entry 0 的 `(TypeId, kind)` 一次比较（空 entry 的 kind 是 `0xFF`，`UNRESOLVED` 接收者也不会误中）；单元在 `hdr + 32 + offset`。只比 entry 0（多态的其余类型走 helper 的 4 槽扫描）。偏移、`Arc` 布局、块头年龄 / 记忆集位都由 `object_tests::jit_layout_matches_the_accessors` 对照 Rust 访问器钉住。
  - JIT 引用写内联：寄存器经 `VALUE_TAG_TO_KIND` 编码（基元 / 栈句柄 / `FuncRef` → helper），SATB 标记中 → helper；分代模式下属主 `gen_age ≥ promotion_age` 且未在记忆集、值年轻（或是数组）→ helper；其余 release store（AArch64 `stlr`、x86-64 普通 store）。判定都在写之前，要屏障的写整条交给 helper，所以 old→young 边与解释器走同一个 `write_barrier_field`。两个判定字节经 `MagrGC::jit_barrier_bytes` 取址（编译期烘焙，堆与 JIT 同属 VmCore）。
  - hoist 只给「接收者寄存器全函数不被写（catch 寄存器也算写）+ 访问在循环内」的 (接收者, 字段, 种类)：入口块内联求一次守卫，没有 helper 调用、不按名解析；删除 `jit_obj_field_slot` / `jit_obj_ref_field_slot`（z42c 剖面里 10.4%）。循环 = 回跳目标与源之间的块。
  - 静态字段：VmCore 全局 `Mutex<Vec<Value>>` 换成 `StaticTable`（`SegVec` 里地址稳定的 16 B 型参单元 + 名字登记表），读写无锁，内存序同实例字段。**不用 seqlock**：每个静态字段的值都是一个 `Value`，型参单元协议本身保证不撕裂；值 struct 类型的静态字段由编译器装箱，单元里只有一个引用字。静态字段是 GC 根（每次回收扫一遍），写入不需要分代屏障，SATB 由单元协议记旧字。cctor 屏障的属主缓存（P0-5b）从每线程 `Mutex<Vec<..>>` 挪进单元旁（一次 acquire load），显式模块重载时清空；`ensure_module_inits` / `ensure_static_owner_init` 的稳态快路内联。
  - 不改字节码 / 格式，用户可见行为不变（`field_ic_tests` 逐例对照解释器，含 NRE 文本）。
- 实测（release z42vm，macOS arm64，与合并基 eb2b431a8 交错，7 次中位数，负载 3–14）：字段微基准（实例字段读写循环 + getter + 换接收者的链表遍历）jit 1117 → 748 ms（0.67×）、interp 0.97×；静态字段读写循环 jit 2211 → 1300 ms（0.59×）、interp 2503 → 2215 ms（0.88×）。bench（new / base）：jit 05 0.99、09 0.68、10 0.80、12 0.73；interp 05 0.98、09 0.95、10 0.97、12 0.94。z42c 工作区构建（5 次中位数）`--jobs 1` 11.51 → 9.94 s（0.86×）、`--jobs 4` 7.11 → 6.30 s（0.89×），产物逐字节一致；`sample` 剖面（`--jobs 1`）里字段 helper 包含时间 15.9% → 1.3%。
- 后续：
  - JIT 的 `StaticGet` / `StaticSet` 已内联。屏障的「已放行」判定放进单元：`StaticCell::ready` 记下整个静态字段屏障（包初始化器 + 属主类型初始化器）判为空操作时的屏障纪元（`CctorRegistry::barrier_epoch`；登记静态构造器 / 包初始化器、包初始化器失败、重跑代际、显式模块（重）加载时递增）。interp 与生成码的快路都是「纪元一次 relaxed load + 单元一次 acquire load，相等即放行」，唯一实现 `VmContext::ensure_static_field_init`（只在结论对所有线程都稳定时记录：重入中的初始化器、失败的类型不记）。放行后按型参单元协议直接访问：读按标签字查表（引用字 / `PRIM(k)`）；基元写在单元已是该种类时只写负载字；引用写在单元从未存过基元时一次 CAS，SATB 标记中走 helper；静态字段是根，不需要分代屏障。null 读（确证字段、值类型惰性补零）、装箱、种类不符、`F32` / 擦除类型的写回落 helper。实测（release z42vm，macOS arm64，与合并基 6b021ceab 交错，负载 11–42）：静态字段读写微基准（`19_static_fields`，每轮 6 读 4 写，7 次中位数）jit 2013 → 838 ms（0.42×）、interp 2460 → 2087 ms（0.85×）；z42c 工作区构建（5 次中位数）`--jobs 1` 10.94 → 10.65 s（0.97×）、`--jobs 4` 7.22 → 7.24 s（1.00），产物逐字节一致。
  - 型参字段与 `F32` 字段仍走 helper；原生 IC 只比 entry 0。

**P1-7 GC**
- 决定：
  - 选 B1：删掉 `keep_major`，minor 只认自己能到达的；epoch 戳保留，作「周期内出生」的标签。
  - 不做「sweep 时释放 payload」的过渡方案。payload 的根治随对象模型改造（阶段 2：payload 并入 GC 槽）。
  - 池超阈值时 decommit。
  - 软上限和 `Z42_GC_MAX_BYTES` 改按真实 footprint 计。
  - 徒劳退避倍数：分代只乘软上限自己的触发；单代没被软上限挤小的闸门晚一步才乘（连续两次徒劳，见 R8 后续 ①② 条）。
- 前置验证：
  - 模型 D 判定 B1 安全，现行策略在活性断言上给出反例（#1146）。
  - 当前 main 关掉 `keep_major`、用 `Z42_GC_SLICE_MS=0.05` 跑 z42.net 全套 5 轮，每轮 50/50。历史记录里的挂死已不再出现。
- 序列：
  - B1：删 `keep_major`，并加断言「Sweeping 期间晋升的条目必须带 epoch」。
  - A2：定长区续用 TLAB 尾巴，复用半活 chunk 的空洞（按槽的 claimed 位图）。不再需要：R9 之后没有定长区，数组也在变长区，空洞复用即 A3。
  - A3：变长区复用空洞。✅ 随 M12（#1258）：TLAB 按 size class 成批从 free list 认领半活 chunk 的死槽、原址复用（`gc/var_region/holes.rs`），数字见 M12 行。
  - A4：decommit（定长区按页对齐，记录 generation 下限）。
  - A5：committed 计数、`stats()` 降为 O(1)、软上限按 footprint。

**P1-8 Builtin ABI v2**（User，2026-10-12；取代上表 P1-8 行里的 manifest 方案）
- 决定：
  - **Rust 侧单一真相**：`#[builtin(name = "__x", pure?, blocking?, reenters?, this = "M"?)]` 写在领域文件的 `#[z42_macros::builtins]` 内联模块里，签名就是普通类型化 Rust 函数；宏生成从调用方寄存器按下标读参的包装器（不经 `Vec` / `SmallVec`）、`BuiltinDesc`（名字、参数种类、返回种类、标志）与模块的 `DESCS`，`builtin_table.rs` 的 `TYPED` 一个模块一行。旧表与新路径并存，按领域迁，迁完删旧表。
  - **不做 manifest，也不在编译期校验 `[Native]`**（User：靠测试，用例不过自然就有问题）。不恢复 E0901 / E0902；`native_decl_tests`（名字、void）保留。
  - **类型化错误** `BuiltinError`：Format / Argument / ArgumentNull / ArgumentOutOfRange / IndexOutOfRange / Overflow / InvalidOperation / NotSupported / InvalidCast / NullReference / KeyNotFound / IO 族（IO / FileNotFound / DirectoryNotFound / PathTooLong / EndOfStream）/ UnauthorizedAccess / Timeout / 任意类 `Exception{class}` / 原样重抛 `Thrown` / `Internal`，各对应 .NET 同名类；缺的 stdlib 类按 .NET 名字与继承补齐（`ArithmeticException`，`OverflowException` / `DivideByZeroException` 改继承它；`ArgumentOutOfRangeException`、`TimeoutException`、`UnauthorizedAccessException`；`Std.IO.IOException` 及 `FileNotFound` / `DirectoryNotFound` / `PathTooLong`，`EndOfStreamException` 改继承 `IOException`）。
  - **内部错误致命**：`BuiltinError::Internal`（Rust 缺陷、不变式被破、声明与签名不符）与栈溢出同一机制（`stack_guard::fatal_error`：致命标志 + z42 栈报告，退出码 3 / `Z42_HOST_ERR_FATAL`），不可 catch。未迁移的旧表 builtin 照旧映射成 `Std.Exception`。两个引擎共用一个失败出口 `abi::error::raise`，JIT 侧缺的 `is_fatal` 检查随之补上。运行期解析不到的 builtin 名 → `MissingSymbolException`。
  - **阻塞即 park，结构上禁止分配**：`#[builtin(blocking)]` 的函数体整段 park 运行，只收 `PlainArg`、只回 `PlainRet`，拿不到 `&VmContext`；需要先用 ctx 准备的返回 `Blocking::new(move || …)`（`Send + 'static` 闭包 park 运行）。结果都在 unpark 之后才转成 `Value`。
  - **`[Native]` 桩在 VM 链接期折叠，两个引擎都做，不在编译器里做**（编译器折叠会把 stdlib 私有的 builtin 名烤进用户包、破坏包版本边界）：一块的 `builtin; ret` 桩与同类型链式转发按函数识别（`Function::forward`，首调时算），调用直接按 FnId 跑 builtin；cctor 屏障照旧在折叠前执行。**栈回溯不变**：不会执行 z42 代码的 builtin 无帧折叠，失败时才从函数现算包装函数那一行；可能执行 z42 代码的（`reenters`、旧表的反射 / 加载上下文 / REPL builtin）压一个轻帧。sret 桩（仅 `GCHandle.Alloc`）不折叠。
  - **`TryParse` 不再靠异常**：`[Native("__int32_try_parse")] extern bool TryParse(string, ref int)` 一族，builtin 经 `RefArg` 写穿出参；`Boolean` / `Char` / `Single.TryParse` 改纯脚本无 try。
  - 同名 builtin 改语义要换名字：已发布种子的 stdlib 跑在新 VM 上（冷启动自举），本 PR 只把抛出的异常换成更具体的子类，形状不变。
- 序列：
  - PR1 ✅（#1255）：全部基础设施（宏、`corelib/abi/`、两个引擎的新调用口 `invoke_builtin`、错误出口、致命路径、blocking、链接期折叠）；迁 convert（9 个 Parse + 9 个新 TryParse + ToString / BitConverter / 装箱 / `__str_compare_to`，`__str_compare_to(null)` 改按 .NET 返回 1）、`__str_length` / `__str_char_at`（越界改抛 `IndexOutOfRangeException`）、线程 sleep / join（blocking 样板）。实测（release，交错中位数，负载 4–14，new / base）：`Int32.Parse` + `Double.Parse` 循环 interp 0.53 / jit 0.74；字符串逐字符求和 interp 0.38 / jit 0.99（JIT 本已内联）；`TryParse` 失败路径 interp 0.23 / jit 0.26；旧表 builtin 的折叠调用（`GetHashCode` / `Equals` 循环）interp 0.47 / jit 0.85；07 / 08 / 05 / 10 interp 0.89–1.00、jit 1.00–1.02（噪声内）；启动 +2%（z42.core 多 8 个类）；z42c 工作区构建 1.00（两轮 0.97 / 1.00），产物逐字节一致。
  - PR2 ✅（#1262）：fs / io / process / thread join 迁 ABI v2。
    - **fs**（`fs.rs` 全部 builtin + `fs_backend` 改返回 `io::Error`）：按 .NET 口径抛（`fs::fs_error(e, path, role)`）——不存在且所在目录存在 → `FileNotFoundException`（`Could not find file '<path>'.`），目录 / 路径的一部分不存在 → `DirectoryNotFoundException`（`Could not find a part of the path '<path>'.`），无权限、把目录当文件读写 → `UnauthorizedAccessException`，`ENAMETOOLONG` → `PathTooLongException`，已存在 → `IOException`（`The file '<path>' already exists.`），其余 → `IOException`（`<OS 原文> : '<path>'`）；空路径 → `ArgumentException`，null → `ArgumentNullException`；`Exists` 对 null / 空路径答 false。与 .NET 的出入（保留 z42 现行语义）：`File.Delete` 缺文件抛 `FileNotFoundException`（.NET 静默）、`Copy` / `Move` 覆盖已有目标、`GetLastWriteTime` 缺文件抛（.NET 返回 1601 年）。读 / 写 / 打开 / 新建 / 原子写（含 `F_BARRIERFSYNC` / `fdatasync`）/ 目录枚举 / glob / 临时文件一律 park（`blocking` 或 `Blocking::new(..).then(..)`）；只做 `stat` 的探测（`Exists` / `GetSize` / `GetLastWriteTime` / `Which`、FileStream `Length` / `Position` / `Seek`）不 park（见实测）。FileStream 句柄槽改存 `Arc<File>`：表锁下只克隆句柄，系统调用在锁外；`__file_read` 改一次原子块拷贝写回 `byte[]`（原来逐字节 `set_boxed`）。
    - **io**：`__println` / `__print` / `__eprintln` / `__eprint` 标 `reenters`（实参的 `ToString` 是 z42 代码，折叠调用压轻帧）；TestIO / 宿主 sink 在 unpark 状态下收（宿主回调可能重入宿主 API），只有写进程自己的 stdout / stderr 的那一下 park。`__readline` `blocking`（EOF 仍返回 `""`，形状不变）；`__repl_readline` 标 `reenters`、函数体显式 park（行编辑器在读的中途以本 ctx 回调补全器，回调内 `NativeUnparkGuard`）。
    - **process**：新名字从 Rust 抛、等子进程一律 park——`__process_run_sync` / `__process_start` / `__process_wait` / `__process_try_wait` / `__process_kill` / `__process_stdin_write` / `__process_stdin_close` / `__process_pid` / `__process_close` / `__process_stdout_read` / `__process_stderr_read`。映射：启动失败（含 cwd 不存在）→ `Std.IO.ProcessStartException`，改继承 `IOException`（.NET 抛 `Win32Exception`，z42 没有）；超时 → `ProcessTimeoutException`，改继承 `TimeoutException`，`Program` / `TimeoutMs` 由 Rust 填（新 `BuiltinError::Detailed`：按序取已加载的类 + 公开字段）；句柄失效 → `ProcessHandleInvalidException`，改继承 `InvalidOperationException`；写未重定向（或已关）的 stdin → `InvalidOperationException`（.NET 原文，句柄保持有效——原来连带作废句柄）；`Stdio.ToFile` 用于 stdin → `NotSupportedException`；输出目标文件建不出 → fs 映射；管道 I/O 错（`EPIPE`）→ `IOException`；畸形参数包 → `ArgumentOutOfRange` / `Argument`。结果数组 `[exitCode, stdout, stderr, stdoutBytes, stderrBytes]`（字节是打包 `byte[]`）。
    - **thread**：`__thread_wait` 失败（worker 抛 / panic / 重复 Join）从 Rust 抛 `Std.Threading.ThreadException`（.NET 的 `Join()` 自身不失败、限时 Join 返回 bool，故沿用 z42 类，不涉及 `TimeoutException`）。
    - **种子兼容**：旧名字（`__process_run` / `__process_spawn` / `__process_handle_*` / `__thread_join`）形状不变，留在 `process_legacy.rs` / `threading.rs`；**z42.core 的 `[Native]` 包装也不改形状**——`ThreadNative.Join`、`ProcessNative.Run` / `Spawn`、`ProcessHandleNative` 保留，新的 z42.io / z42.threading 改调新增的 `ThreadNative.Wait`、`ProcessNative.RunSync` / `Start`、`ChildProcessNative.*`（预建的新 core 与种子带来的旧 io / threading 同处一个 libs 目录：第一版直接改了 `ThreadNative.Join` 的返回类型，旧 `Thread.Join` 把 `void` 当数组读，z42c 自举当场挂）。四条 `STAGE2-DEBT` 挂账。
    - **ABI 扩展**：参数 `Vec<u8>` / `Option<Vec<u8>>` / `Vec<String>`（拷出，可用于 `blocking`）；返回 `Vec<u8>`（打包 `byte[]`）/ `Vec<String>`；`Blocking::ready`（不 park）/ `Blocking::then`（parked 部分产出 `File` / `Child` / 字节，unpark 后拿 ctx 登记句柄、写 GC 数组）；`BuiltinError::Detailed`；`io::ErrorKind::Unsupported` → `NotSupportedException`。
    - **GC 修复（单独提交 `fix(gc)`，main 上已存在）**：`native_park_incr` 每次 `retire_thread_tlab`——半满 chunk 退回、下次分配认领一整块新的；退回的 chunk 要等回收才复用，空闲部分又不算占用、触发不了回收，「park 一次、分配一次」每轮烧一块 chunk（main 上 `Thread.Sleep(0)` + 一个字符串 10 万轮：RSS 1.8 GB；本 PR 的文件读写循环 2 万轮：1.4 GB）。改为 park 时把 claim 寄存在 `VmContext.parked_tlab`、unpark 时取回接着填，期间若有回收由 collector 在 mark 前代为 retire。同一 sleep 循环 RSS 18 MB。
    - 实测（release，交错 9 次中位数，负载 3.4–5.5，new / base）：FileStream 4 线程各读 1 MiB × 40 轮 interp 0.36 / jit 0.38（锁外读 + 块拷贝）；`Thread.Sleep(0)` + 分配 10 万轮 interp 0.16 / jit 0.14（GC 修复）；`File.Exists` + 分配 20 万轮 1.00 / 0.99；单线程小文件读写循环（每轮 8 次 I/O builtin，2 万轮）墙钟 0.95–1.07（系统调用占 95%，噪声内），user CPU 0.13 → 0.15 s（每次 park 的握手 + 闭包装箱），RSS 21 → 22 MB。`File.Exists` 一度也 park：同一循环 2.6×（GC 修复前，park 的固定成本远大于一次 `stat`），所以只做 `stat` 的探测不 park。z42c 工作区构建（5 次中位数）`--jobs 1` 11.68 → 11.71 s（1.00）、`--jobs 4` 7.79 → 7.83 s（1.01），峰值 RSS 581 / 650 MB 不变，13 个产物逐字节一致——z42c 的文件 I/O 是一次性 `ReadAllText` / `WriteAllTextAtomic`，几乎不用 FileStream，句柄表锁本不是它的瓶颈。
    - 未做：net / tls、monitor、反射、ext 压缩库、io 剩余旧表项（TestIO sink / `__concat` / `__len` / `__contains`）、旧名字在种子不再声明后的清理（STAGE2）、z42c 里 `catch (Exception)` 包着文件操作的地方收窄到 `IOException`（要等种子带上新 VM，见轴 ③）。
  - PR3 ✅（#1214）（分支 `vm/builtin-abi-v2-net`）：net / tls / monitor 迁 ABI v2。
    - **net**（`network/{tcp,udp}.rs`，新名字 `__tcp_*` / `__udp_*` / `__dns_lookup`，28 个：connect 与 connect-with-timeout、listen 与 listen-with-options 各并成一个，listen / bind 只回槽位、端口另取（`__tcp_listener_port` / `__udp_local_port`））：返回普通值、失败从 Rust 抛，映射在 `network/error.rs`——

      | 情形 | 异常 |
      |---|---|
      | OS socket 错误（拒绝 / 重置 / 占用 / 不可达 / …） | `Std.Net.Sockets.SocketException`，新增 `SocketErrorCode`（新增枚举 `SocketError`，.NET 的 Winsock 码：Unix errno 按 .NET `SocketErrorPal` 映射，Windows 直接用 WSA 码）/ `ErrorCode`；消息 = OS 原文（去掉 Rust 的 ` (os error N)`），connect 失败后缀对端地址（同 .NET 6+） |
      | 读写 / 连接超时（`SO_RCVTIMEO` 到点的 `EAGAIN`、`connect_timeout`） | `SocketException(TimedOut)`，消息取 OS 的 `ETIMEDOUT` 原文——.NET 的 `Socket.Receive` / `Connect` 也是 SocketException，不是 TimeoutException |
      | 名字解析失败 | `SocketException(HostNotFound)`（Windows 的 `TryAgain` / `NoRecovery` / `NoData` 原样），消息 = 解析器原文 |
      | 关闭 / 未知句柄，阻塞中被别的线程关掉 | `SocketClosedException`，改继承新增的 `Std.ObjectDisposedException : InvalidOperationException`（`ObjectName = Std.Net.Sockets.Socket`）；与 .NET 的出入：accept 中被 Stop 仍报它（.NET 是 `SocketException(Interrupted)`），HttpServer 的 serve 循环靠它收尾 |
      | 端口 ∉ 0–65535、TTL ∉ 0–255、keepalive 调参 < 1 | `ArgumentOutOfRangeException`（原 SocketException；两个 z42.net 用例随之改） |
      | 组播地址不是 IPv4 字面量 / 不在组播段 | `FormatException` / `SocketException(InvalidArgument)` |
      | wasm32 | `NetUnsupportedException`，改继承新增的 `Std.PlatformNotSupportedException : NotSupportedException`（`network/wasm.rs` 同名同元数的桩） |

      解析 / connect / accept / 读写 / UDP 收发全部 `Blocking::new(..).then(..)`：parked 部分只碰 `Arc<TcpStream>` 等 Rust 数据，`then` 在 unpark 后登记槽位、块拷贝进调用方 `byte[]`（原来逐字节 `set_boxed`）、核对阻塞期间句柄是否被关（关了优先报 SocketClosed）；`setsockopt` 类不 park。可中断 accept（`ListenerSlot` + poll 循环）抽到 `network/listener.rs` 新旧共用。z42.net：`NetworkStream` 把 SocketException 包成 `IOException`（`Unable to read data from the transport connection: …`，`InnerException` 为原异常），同 .NET；`NetTcpDecode` / `UdpDecode` 删除。
    - **tls**（`tls.rs` 新 `__tls_*`，旧的移到 `tls_legacy.rs`）：解析 / TCP 连接 → SocketException；握手被拒（证书 `InvalidCertificate` / `NoCertificatesPresented` → `The remote certificate is invalid according to the validation procedure: …`，其余 rustls 错误 → `Authentication failed: …`）→ 新增 `Std.Security.Authentication.AuthenticationException`（z42.net）；握手途中 EOF / 重置 / 超时 → `IOException`（.NET 原文 `Received an unexpected EOF or 0 bytes from the transport stream.` / `Unable to read data from the transport connection: …`）；流读写 → `IOException`；不能作 SNI 的主机名 → `ArgumentException`；**无 `close_notify` 的关闭读作 EOF（0）**，同 SslStream（旧 builtin 报 socket_err）。
    - **monitor**（`__monitor_*` 原名迁：形状不变，只换更具体的异常）：同线程重入 → 新增 `Std.Threading.LockRecursionException`（.NET 不可重入锁 `ReaderWriterLockSlim` NoRecursion 的异常；Monitor 在 .NET 本身可重入，z42 的不可重入）；非持有者 `Exit` / `Wait` → 新增 `Std.Threading.SynchronizationLockException`（.NET `Monitor.Exit` 原文）；句柄为 null / 不是 Monitor → `ArgumentNullException` / `ArgumentException`（`MonitorNative` 是公开类，按用户可触发处理，不算内部错误）。`enter` 的重入检查 / 快路径 / 自旋在 builtin 里 unpark 做完，要等时返回 `Blocking::new`（闭包只握 `Arc<Monitor>` + `ThreadId`）；`wait` 整段 park（一次拿内部锁完成查持有 → 释放 → 等信号 → 重新持有）。原来靠「`_park` 先声明后 drop」的声明顺序保证「先放内部锁再出 park」，现在由闭包返回结构保证。`profile-contention` 探针改为 `Send` 的 `MonitorContention`（零大小于默认构建）。
    - **fs**：已关闭的 FileStream 句柄改抛 `ObjectDisposedException`（`Cannot access a closed file.`，仍是 InvalidOperationException）；`BuiltinError::object_disposed`。
    - **种子兼容**：旧 `__net_*`（28 个 TCP / UDP / DNS + 6 个 TLS）原样留在 `network/legacy/` 与 `tls_legacy.rs`，句柄表共用；z42.core 的 `NetTcpNative` / `UdpNative` / `NetTlsNative` / `DnsNative.Lookup` 不动，新 z42.net 调新增的 `TcpSocketNative` / `UdpSocketNative` / `TlsSocketNative` / `DnsNative.Resolve`；`STAGE2-DEBT(legacy-net-decls)`（core 声明）与 `STAGE2-DEBT(legacy-net-builtins)`（builtin 表）。
    2026-10-20：core 侧旧形状声明全数删除（NetTcpNative / NetTlsNative / UdpNative / DnsNative.Lookup，
    连同 ProcessNative.Run/Spawn、ProcessHandleNative、ThreadNative.Join）——种子代消费者已于
    #1262 / #1271 迁移完毕；运行时表项与 legacy 实现仍挂原 STAGE2-DEBT，待种子不再声明旧名后删。
    - 测试：Rust——映射（拒绝 + 端点、占用、超时、HostNotFound、未知句柄、越界、组播、errno 表）、自签证书（`tls_test_data/`，P-256，至 2126 年）被拒 → AuthenticationException、对端不说 TLS、握手 EOF / 超时；GC 在线程阻塞于 accept / TCP read / UDP recv_into / TLS 握手（覆盖 connect 的 parked 闭包）/ Monitor enter / wait 时完成（< 1 s，`parked_count == 1`）。TCP connect 本身的阻塞用 TEST-NET-1 尽力观测（本机有 fake-IP 代理时跳过，DNS 失败用例同理）。stdlib：`z42.net/tests/{socket_exceptions,tls_exceptions}.z42`、`z42.threading/tests/monitor_exceptions.z42`。Windows：读阻塞被 close 唤醒只在 Unix 成立，Rust 用例在 Windows 上改由对端关闭结束那次读。
    - 实测（release，base / new 交错，中位数 + 四分位，负载 4–8；同一份 zpkg、各用自己的 stdlib）：回环 TCP echo 吞吐（64 KiB × 4096 往返 = 256 MiB）interp 2482 → 560 ms（0.23）、jit 0.23——`__tcp_read` 改一次块拷贝写回 `byte[]`（原逐字节 `set_boxed`）；回环 ping-pong 6 万次 32 B 往返 interp 0.98 / jit 0.93；有界 Channel(16) 单生产单消费 100 万项 interp 0.97 / jit 0.91；无争用 `Mutex.Lock` 200 万次 1.01 / 1.00；**4 线程争用 `Mutex.Lock` 各 30 万次 interp 1.15 / jit 1.05**（base interp 呈双峰 634–839 ms，new 稳定 734 ms；两边 CPU 剖面几乎相同——`begin_enter` 自身样本比旧 `builtin_monitor_enter` 还少，差在 `swtch_pri` / `cvwait` 上，即锁交接时序；把 park 改回内联 `NativeParkGuard` 对照实验数字不变，排除 `Blocking` 闭包装箱；换成同一份 stdlib 结果不变，排除 z42.core 的新类）——未定位，记为后续项。中途修正：`wait` 先 unpark 查持有 + 每次克隆 `Arc<Monitor>` 时 Channel 慢 4–6%，改为 wait 一次拿内部锁、不阻塞的路径只借用 `Arc` 后转为上面的 0.91–0.97。z42c 工作区构建（5 次中位数）默认 jobs 6.20 → 6.19 s（1.00）、`--jobs 1` 9.91 → 9.89 s（1.00），32 个产物逐字节一致。
  - 后续 PR 按领域迁：反射（删 `LEGACY_REENTERS`）、ext 压缩库（`wrap_*` 胶水）、string / math / object / gc 剩余旧表项、io 剩余（TestIO sink / `__concat` / `__len` / `__contains`）；旧表迁空后删 `Native::Val` / `Void`、`PART1` / `PART2`、`exec_builtin` 的 `&[Value]` 入口改为只服务测试；sret 桩与 `CallIndirect` 的折叠。

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
    - R1 ✅（#1192）：对象字段单元。基元同宽 relaxed；直接引用字段（含 `string` / `object` / 接口 / 委托）一律 8 B 自描述字（低 3 位种类，0 = null，release 写 / acquire 读，标记期写改 swap 供 SATB）；擦除泛型把基元写进 `object` 字段时装进单元素盒子（种类 7）；GC 只经 `visit_refs`；JIT 引用读改为 acquire load + 种类查表，`string` 字段也走内联。型参字段、内联 struct 引用叶子、合成布局仍在 16 B 侧表。实测（100 万个活对象，含 `object[]` 容器）：1 个 string 字段 GC 用量 88 → 72 B/个、RSS 160 → 122 B/个；2 个引用字段 112 → 80 B、168 → 153 B；z42c 工作区构建峰值 RSS 829 → 805 MB，墙钟持平。
    - R2 ✅（#1203）：型参字段（`T F;` / `T? F;`）出侧表，改为对象自己的 16 B 单元（`metadata/types/tparam_cell.rs`）：自描述标签字在编译器给的 8 B 槽位，基元负载字追加在编译器布局之后（编译器偏移、子类 base_shift 都不动）。标签字 = `0`（null）/ 直接引用字 / 种类 7 盒子 / `PRIM(k)` / `NULL(k)`；引用与 null 不用负载字。**负载字一生只装一种基元**：种类由唯一一次 CAS 认领定下（已知基元实参在 `ObjNew` 零值初始化时即认领；字面 `T` / 无实参的实例在第一次存基元时认领），之后标签字不再回到「从未存过基元」的状态，读者 acquire 到 `PRIM(k)` 再 relaxed 读负载字不会撕裂。不符写入对用户透明：`T = int` 收到 null → `NULL(k)`，不分配；已定种类后来了引用或别种基元、直接引用后来了基元、栈句柄 → 装箱进标签字，之后留在标签字。**种类不在分配时按实参写死、改为首次基元写认领**：擦除代码里的 `new Node<T>()`（`LinkedList<int>` 的节点）实参是字面 `T`，按引用编码会每次存 int 都装箱；运行期解析开放实参又会改变反射 / `default(T)` 的可见行为。慢路频率（临时计数器）：z42c 工作区构建型参单元写入 0 次；e2e 全量（interp + jit）写入 36 次、装箱 2 次（`generic_class_returns_blob` 把值 struct 句柄存进 `T` 字段）。GC 仍只经 `visit_refs`；SATB 经 CAS / 标记期 swap 拿旧字。合成布局的引用字段与内联 struct 引用叶子仍在侧表（R3）；JIT 型参字段读写仍走 helper（标记字按引用字查表会读成 null）。实测（100 万个活对象，含 `object[]` 容器，GC 用量 / RSS 密度，分代）：擦除分配的 `Box<int>` 88 / 223.8 → 80 / 223.2 B，`Pair<int,string>` 112 / 282.9 → 96 / 247.5 B，`LinkedList<int>` 每节点 104 / 221.0 → 96 / 213.4 B（STW：235.0 → 205.5 B）；闭合实例化（本包描述符的 `Box<int>`）本就不用型参单元，不变。z42c 工作区构建墙钟 / 峰值 RSS 持平（14.91 / 14.92 s，811 / 810 MB），产物逐字节一致；bench 05 / 10 / 12 / 08 / dict_ops 持平（±1.5%），型参字段 get/set 微基准 jit +1–2%、interp ±1%，`LinkedList<int>` 建表 + 遍历 jit +1–3%、interp 持平（每节点多一次认领 CAS）。
      - 另见 M14 ✅ #1216：泛型实例的类型实参改为按实例化驻留，不再每对象一个 `ObjExtras` 盒子（闭合 `Box<int>` RSS 210 → 155 B/个，与非泛型同形类持平）。
    - R3 ✅（#1214）：数组元素分模式，分配时定下（`objops::array::elem_mode`，`ArrayNew` / `ArrayNewLit` / `Array.CreateInstance` 共用）：PRIM（打包基元，同宽 relaxed 原子）/ REF（8 B 引用字，新块类型 `ArrayRef`）/ MIXED（擦除 `T[]`、enum、非 blob struct、查不到的类型：16 B 型参单元）/ STRUCT（每个数组一块 blob）/ FROZEN（闭包环境与种类 7 值盒子，发布前写一次）。**MIXED 不加分片锁**（User 认可，替代上面「全局分片锁兜底」的决定）：复用 R2 的「负载字一生只装一种基元、CAS 认领一次」协议，16 B 元素读不撕裂；基元实参的 `T[]`（`List<int>` 的 backing）只在数组上记一个零种类、不写任何字，未碰过的单元读作该零值，预留容量的页面不被碰。struct 引用叶子改 8 B 引用字：新 `metadata/types/struct_blob.rs` 一套 blob 布局供装箱 struct、对象内联 struct 字段与 `struct[]` 元素共用，**`refs` 侧表删除**，`ObjStorage` 只剩一块零初始化字节（新 `metadata/types/cells.rs`：`Cells` / `CellsMut` 原子单元视图）；合成布局的引用字段改排型参单元。屏障：`set_boxed` / `store_elem` / `try_set_boxed` 返回 `FieldWrite`，单元素经 `objops::array::barrier_elem`，批量写（`Array.Copy` / 排序）经不分配的 `array_bulk::barrier_after_range_store`（`ArrayObj::visit_elem_refs`）。JIT：REF 与 MIXED 的 `ArrayGet` 内联 acquire 读，含 MIXED 基元标记字与零种类（`tparam_cell::PRIM_KIND_TO_VALUE_TAG`）。gc-modes 的 STW 腿预算 32M → 16M（数组变小、回收变少：基线正好 40 次 = 下限，R3 降到 35；16M 下 70 次）。实测（100 万元素，GC 用量 / RSS 密度 B/元素）：`object[]`、`string[]` 16 / 15.7 → 8 / 7.6；`struct{int;string}[]` 用量 32 → 16（RSS 377 → 361）；`List<T>` / `Dictionary` 走 MIXED、用量不变（M13 之后才打包），`Dictionary<string,int>` STW RSS 341.8 → 253.6。z42c 工作区构建墙钟 14.77 → 14.61 s、峰值 RSS 818 → 759 MB（中位数，5 次），产物逐字节一致。bench（负载 3.4–6.5，new / base）：jit 05 0.855、list_ops 0.939、sort 0.894、fa40 0.901、refarr 0.950、reflist 0.939、08 0.941，dict_ops / 10 / 12 持平；interp 数组密集的 −1% 到 +5%（sort 1.049、dict_ops 1.035，每对象锁还在时多付了单元协议，R4 去锁后应收回）。慢路频率（临时计数器）：e2e 全量新增 48 次种类 7 装箱，z42c 构建 0 次。
  - R4 ✅（#1225）：删掉每对象 Mutex，顺带修掉 D4（JIT 绕过锁）与 `Array.Copy` 的 ABBA 死锁面。`RegionEntry` 直接装 `T`，`GcRef::borrow()` 给 `&T`（`borrow_mut` 删除）；`CellsMut` 并入 `Cells`，对象 / 数组的所有写原语改 `&self`。普通切片视图全部去掉：`bytes()` / `bytes_mut()` / `as_bytes()` 换成原子分块拷贝（`Cells::copy_out` / `copy_in`：按对齐取 8 / 4 / 2 / 1 B，从不劈开单元，8 B 块 acquire），PRIM 数组的 `copy_from_slice` / `copy_within` 换成按元素宽度逐元素原子移动（`copy_prims`，memmove 方向），数组克隆读源块也走 `copy_out`；`BoxedStruct` 拆箱读改单元读。构造期独占：`ObjNew`（interp 堆 / 栈、JIT）与 `Activator.CreateInstance` 先在本地建好整个对象（`ScriptObject::new_instance`：驻留的泛型实参句柄 + 型参字段零值），再 `MagrGC::alloc_script_object`，发布后 `meta` 字不变；栈数组写入走 `set_boxed_exclusive`（stack arena 锁下）；FROZEN 数组发布后写入报错。**`lock(obj)` 不需要过渡机制**：语言没有 `lock` 语句、也没有按任意对象加锁的 API，z42 侧的锁全是 `Std.Threading.Mutex<T>` / `RwLock<T>` / `Channel<T>`，底座是对象自己的 `NativeData::Monitor`（自带内部锁），不依赖每对象 Mutex。`RegionEntry<ScriptObject>` 64 → 56 B，`RegionEntry<ArrayObj>` 72 → 64 B（在 M8 之上；`region_entry_stays_lean_for_script_objects` 钉住）。实测（release，交错 5 次中位数，负载 5.8–8.2，new / base）：interp 05 0.907、08 0.966、09 0.904、10 0.931、12 0.894、dict_ops 0.954、list_ops 0.910、sort 0.955、fa40 0.904、refarr 0.966、reflist 0.938；jit 05 0.989、08 0.981、09 0.832、10 0.997、12 0.827、dict_ops 0.934、list_ops 0.920、sort 0.897、fa40 0.786、refarr 0.726、reflist 0.953（R3 留下的 interp 数组密集 +1–5% 全部收回）。100 万个小对象（装在 `object[]` 里）RSS 每个 −8 B：空类 STW 82.3 → 74.3、分代 135.4 → 124.2，`{int}` 113.3 → 105.3 / 148.2 → 145.2，`{long}` 89.1 → 81.2 / 123.7 → 115.7，`{int;string}` 128.3 → 120.3 / 135.9 → 126.6；`GC.UsedBytes` 是不含槽头的估算，不变。z42c 工作区构建墙钟 12.99 → 12.48 s、峰值 RSS 780 → 753 MB（中位数，5 次，负载 5.8–7.1），产物逐字节一致。并发：新增 `z42.threading/tests/unsync_cell_races.z42`（不加锁的字段 / 数组 / 型参单元并发写，读到的每个值都完整；两条线程互相 `Array.Copy` 两个数组不卡住；并发发布 + 强制回收），interp 与 jit 均通过。
  - R5：Monitor 侧表。
  - R6 ✅（#1247）：对象搬进变长区。对象 = 变长区里**一个** `BlockType::Object` 块（新标签值 7）：16 B 块头 + 16 B 对象头 + 字段单元，一次分配；`ScriptObject` 改为 `#[repr(C)]` 动态大小类型 `{type_desc, meta, fields: [UnsafeCell<u8>]}`，`&ScriptObject` 是盖住对象头 + 字段的胖引用（元数据 = 字段区长度，只在持有整块 provenance 的指针上构造、长度取自块头），字段访问从不越出引用范围（Miri Stacked Borrows 下干净）；对象头经定长前缀 `ObjHeader` 写入；定长槽 `Region<ScriptObject>`、`ObjStorage` 的单独 malloc、`Tlab.obj` 删除。`Value::Object` / `BoxedStruct` 载荷改为 `ObjRef`（对象块的 `VarGcRef`，8 B，位与引用字种类 1 / 6 同形，方法集照 `GcRef`），`Value` 仍 16 B、判别值不变，不改字节码 / 格式；JIT 没有硬编码对象布局（字段基址仍经 helper，块不移动）。构造：`MagrGC::alloc_instance` 取清零块、写对象头、在**未发布**的块上跑 `install_type_args`，然后才记账 / 交出句柄（取块到发布之间不分配）；栈对象与堆外构造（反射、带初值 / 原生底座的 `alloc_object`）用同形的脱离块 `OwnedObject`，`alloc_script_object` 整块拷入。死对象：drop glue 的 `Object` 臂在 tombstone 时释放对象头的 `Arc<TypeDesc>` 与 extras 盒子（返回 extras 字节数，变长区扣回占用账；glue 只对类型掩码里的块调用）。分代：变长区没有卡表，对象改用**记忆集**——老对象收到年轻引用 / 带着年轻引用晋升时对块头 bit 5（`REMEMBERED_BIT`）做原子 or，置位的一方 push 进 `ArcMagrGC::remembered`（mutator 路径不拿 region 锁）；minor 穿过记忆集里的对象播种年轻孩子，不再有年轻孩子就清位出集；晋升线降低时把所有持有年轻孩子的老对象入集；`Z42_GC_VERIFY_CARDS` 一并核对。侧表：finalizer（`obj_finalizers`）与软引用按块头地址记账、块头置 bit 7（`SIDE_BIT`）；带 bit 5 / 7 的块 tombstone 时地址进 `flagged_dead`，`process_flagged_dead` 在**任何 chunk 回收之前**（各清扫尾巴、增量清扫每个桶、`finalize_now`）触发 finalizer、删软引用 / 记忆集条目。弱引用 `WeakObjRef` 记下所在 chunk 的复用 epoch（chunk 进池或被释放时 +1），由堆 upgrade：epoch 变了直接判死、不读内存——大对象的专用 chunk 照旧在死后直接释放。`ObjRef::borrow` 的悬垂守卫是一次 relaxed 读 generation + 冷路径 panic（每次 tombstone 都 bump generation，alive 位不额外提供信息；写成 `assert!` 时 interp 热循环每次迭代多约 40 条指令，10_mono_vcall interp +5%）。顺带发现软引用注册表把数组目标按 `RegionEntry<Vec<Value>>` 读存活位的 bug（已单独合入 #1239）。实测（release，100 万个小对象装在 `object[]` 里、`GC.Collect()` 后读，(N=1M − N=0) / 1M，各 5 次中位数，负载 10–35，base → new）：块大小空类 32 B、`{int}` / `{long}` 40 B、`{int; string}` 48 B；`GC.UsedBytes` 每个 48 → 32、52 → 36、56 → 40、64 → 48（改按整块计）；RSS 每个 STW 空类 90.1 → 64.6、`{int}` 120.8 → 72.8、`{long}` 97.3 → 72.8、`{int; string}` 108.8 → 80.8，分代 87.2 → 65.2、110.4 → 62.5、82.3 → 69.1、86.3 → 72.7（RSS 里含 `object[]` 每元素 8 B 与回收期标记队列的峰值，两边相同）。bench（交错 7 次中位数，new / base）：jit 05 0.986、08 1.006、09 0.775、10 0.983、12 0.782；interp 05 0.993、08 1.012、09 0.826、10 0.968、12 0.861（08 一次只跑 26–30 ms）。z42c 工作区构建：峰值 RSS 中位数 647 → 619 MB（7 次），CPU user 23.6 → 23.6 s；墙钟在负载 15–26 下 11–25 s 抖动、两边分不出（另一轮 4 次 11.28 → 10.69 s），产物逐字节一致；GC 阶段（各一次构建）minor 171 → 146 次，minor mark + sweep 合计 997 → 953 ms，记忆集每次 minor 至多播种 2 394 个对象。未做：块头 + 对象头仍是两个 16 B 头（R8 合一）；TLAB 仍只认领整块空 chunk，半活 chunk 的空洞只有锁路径的 free list 复用（M12）。改为 DST 之后（相对 main，交错 7 次中位数，负载 15–40）：jit 05 1.006、10 0.801、12 0.800；interp 05 1.073、10 1.078、12 1.003——interp 05 / 10 的 +7–8% 指令数只多约 1%，主要是超大 `exec_function_body` 的代码布局 / 寄存器分配扰动（函数与块强制 64 B 对齐后 10 interp 剩约 6%、05 interp 约 1%），z42c 工作区构建 CPU / 墙钟不变。**后续**：① 把对象相关分支移出 `exec_function_body`（解释器专项，消掉这次布局扰动）——✅ 已做。根因：`exec_instr` 整个内联进执行循环，内联进来的 helper 和分发循环同在一次寄存器分配里；按源码行归因（line-tables 构建 + `atos -i`），45.4 KB / 11 351 条指令里 `ArrayNewLit` 959、`CallNative` 638、`PinPtr` 476、`ObjNew` 446、`AsCast` 375、`ArrayNew` 320、ref copy-in 的 `deref_ref` 415 条，这些分支占住被调用者保存寄存器，循环携带的值只能落栈——分发头每条指令 4 次栈读 + 2 次栈写 + 一次 `func.resolved` 的 acquire 读（19 条指令）。改法：上述分支的 helper 一律 `#[inline(never)]`（对象 / 数组分配、`IsInstance` / `AsCast`、`StaticGet` / `StaticSet`、取地址、`Typeof` 与方法级泛型、值 struct、原生互操作、`ToStr` / `StrConcat` 与 `Add` 的字符串分支），分支里只留操作数解码和一次直接调用；循环自己的慢路（ref copy-in、callee 抛出与 `Throw` 的回卷、内部错误定位、按标签跳转）移到 `interp/exec_body_cold.rs`；`func.resolved` 每次激活读一次、传给 `exec_instr`。结果：函数 45.4 → 21.9 KB，栈帧 1 168 → 736 B（不再保存 d8 / d9），函数内栈写 / 栈读 827 / 958 → 167 / 297 条，分发头 13 条指令、只剩 1 次栈读（一条 `ldp`）。逐项实测后留在内联里的：`FieldGet` / `FieldSet` / `ArrayGet` / `ArraySet`——也移出时 05 / 10 再快 2–4%，但每次访问多一次调用，09 / 12 指令 +5%、周期 +2–5%；只移出 `FieldSet` + `ArraySet` 同样 12 +5%；`Call` / `VCall`——移出时各基准 +2–6%。`Frame::set` 的扩容路径改成不带值（省掉先在栈上造值）反而 +3–4%（寄存器分配又漂移），未采用。实测（release，交错 7 次中位数，周期数 new / main，负载 21–28）：interp 05 0.975、08 0.976、09 0.967、10 0.916、12 0.972、dict_ops 0.922、list_ops 0.930、sort 0.969、11 0.996、18 0.953（指令数 0.92–0.97，11 1.004）；jit（5 次）指令数 1.000、周期 0.97–1.01。z42c 工作区构建（产物逐字节一致）：interp 墙钟中位数 10.57 → 10.03 s、CPU user 21.3 → 20.2 s（3 次），jit 7.57 → 7.46 s（5 次，持平）；② Miri：默认 Stacked Borrows 下 `gc::var_region`、对象测试、`metadata::types` 的对象 / 型参 / 引用字测试干净；`-Zmiri-strict-provenance` 过不了是 R1 起引用字以整数位存、解码用 exposed provenance 所致（32 位上字里要装指针 + generation，不能改 `AtomicPtr`）；另有一处**早于 R6** 的 Stacked Borrows 违规——定长 `Region` 经 `ChunkPtr::deref_mut` 对整块 chunk 做 `&mut` 重标记，使先前取的槽指针（`alloc_array_obj`）失效，R9 把数组搬出 `Region`、删掉定长区后消除。**停顿回归（已修）**：CI A/B 报 13_gc_large_heap jit 最大停顿 36.9 → 53.2 ms（各 3 次）。本机各 15 次交错复测确认是真的：R6 前 20.5 ms（19.1–22.4）→ R6 25.6 ms（23.5–26.7），p99 11.0 → 12.6 ms。来源是自适应晋升降线时的 `remember_every_old_object_with_young`：停顿里逐个遍历全部对象块，13 上 21.6–23.0 ms，就是那次收尾 major 的切片本身（R6 前对象有卡表，降线只 `dirty_every_card`）。改为只看新线刚划成老的那批对象——年轻集合里已到线的对象（`remember_lowered_cohort`，`O(年轻集合)`；旧线下已老的对象的老→新边早被屏障 / 晋升记过）：该阶段 0.9 ms（20 642 个对象），13 分代最大停顿 25.6 → 11.9 ms（11.3–13.6，叠在 M12 之上；只有 M12 时 22.9 ms），p99 12.6 → 10.9 ms。
  - R7：侧表换成位图（即 M8，40 B）。
  - R8 ✅（#1268）：块头与对象头合成**一个 16 B 头**，所有变长块同形：8 B GC 字（`GcWord`：generation u16、mark、`type_tag`、payload 长度 u32）+ 8 B 种类字（对象的 `Arc<TypeDesc>`，其余块恒空）。对象 = 头 + 字段单元；泛型实例与带原生底座的对象在 payload 末尾多一个 meta 字（`type_tag` bit 6 `OBJ_META_BIT`，与字符串的 ASCII 位同位不同义；非对齐整字读写；分配点就知道要不要，`OwnedObject` 构造期补实参 / 原生底座时把块加长一个字），其余对象不再付 meta 的 8 B。`ScriptObject` 改为盖住整块的 DST `{gc: GcWord, type_desc, payload: [UnsafeCell<u8>]}`（元数据 = payload 长度，覆盖字段与 meta 字；`payload_len()` 扣掉 meta），`ObjHeader` 是同一 16 B 的定长视图（`ObjHeader::install` 只写类型字），`ObjRef::as_ptr` = 块头地址。**头里装下类型字靠三处让位**：① 存活位并进 generation 最低位（活块奇数、tombstone 变偶、复用变下一个奇数；句柄存的就是这 16 位，`borrow` / `resolve` 一次相等比较同时判死与复用；tombstone 用 CAS 奇→偶同时做陈旧检查与抢先；M12 填进空洞的块保持偶数直到 `publish` 以 release 写入奇数）；② size class 由 `size` 推出（`chunk::payload_class` / `payload_footprint`，chunk 遍历按它步进；专用 chunk 由 `Chunk::dedicated` 记；M12 同 class 填回不改 footprint，并发遍历读到新旧 `size` 都一样跨过）；③ 所属 chunk 不存：清扫 / 年轻集合 / TLAB / 空洞批 / free list 本来就带 chunk 下标（`tombstone_in(ptr, gen, ci)`），从裸句柄出发的弱引用 `downgrade`、`finalize_now`、OOM 退款经 `VarRegion::chunk_of`（chunk 基址的 `BTreeMap` 索引，`O(log chunk 数)`）。per-chunk `reuse_gen` / `max_span_per_chunk` 改为 16 位按模运算（新 epoch 起点 = `reuse_gen + max_span + 1`），`GEN_REUSE_SPAN` 改为活 generation 的最大跨度 65 534（每 epoch 每槽至多 32 768 次占用）。drop glue 收块头指针，`Object` 臂释放类型字与 extras 盒子后把种类字清空。`MIN_BLOCK` 32 → 16（新增 16 / 24 两档，最低八度只留 8 对齐的步长），否则空类 / `{int}` / `{long}` / `{int; string}` 全落在 32 B。**类型字仍是计数的 `Arc<TypeDesc>`**：回落描述符按分配新建、只被加载上下文持有的类型、fixup 的 copy-on-write 新版本生存期各不相同，换成不计数的细指针 / 32 位类型号要先有只增不减的描述符 arena（P1-4 留下的「`field_access` 挪进 TypeDesc 热区」同样没做）。JIT：字段 IC 守卫仍是两次 load（类型字在 +8、`TypeDesc::id`），字段从 +16 起，`type_tag` 在 +3；字符串内联读的长度偏移不变（+4）、标志字节 +10 → +3，都经 `obj_jit_layout` / `vstr::jit_layout` 常量，`jit_layout_matches_the_accessors` 钉住。不改字节码 / 格式，用户可见行为不变。Miri：`-Zmiri-strict-provenance` 下 `gc::var_region` 60 个测试干净；默认模式下 `gc::arc_heap_tests::{objects, holes}`、`metadata::types::{object, owned_object}` 干净（`jit_layout_matches_the_accessors` 原先把类型字按整数读再转指针，是测试代码自身的无 provenance 解引用，改为按指针读；它用 `ObjRef::leak` 故意泄漏，需 `-Zmiri-ignore-leaks`）。实测（release，macOS arm64，与 main 76ba8850e 交错，负载 6–13）：块大小空类 32 → 16 B、`{int}` / `{long}` 40 → 24 B、`{int; string}` 48 → 32 B；100 万个小对象装在 `object[]` 里、`GC.Collect()` 后读，(N=1M − N=0) / 1M，各 11 次中位数：`GC.UsedBytes` 每个 32 → 16、36 → 20、40 → 24、48 → 32；RSS 每个 STW 65.6 → 49.4、73.6 → 57.6、73.6 → 57.6、81.6 → 65.6（都是 −16 B），分代 71.0 → 65.9、65.8 → 87.7、68.5 → 86.5、77.4 → 71.0——分代的峰值落在哪次回收（标记队列 / 增量周期开着时 `GC.Collect` 的位置）由按字节计的 nursery 决定，对象变小就挪了位置：同一程序（main，`{int}`）只改 nursery 就是 68.4（8M）/ 86.2（默认）/ 110.8 MB（32M）；关掉增量 major（`Z42_GC_INCREMENTAL=0`）时四种形状都变小：73.3 → 49.9、76.3 → 71.7、76.0 → 70.5、76.7 → 73.3。

    R8 之下有两个先行的、在 main 上各自成立的提交：**年轻代策略按条目与实测代价判定**（见 GC 调参页「年轻代策略」；没有它时 R8 让 `13_gc_large_heap` 首个 major 后那次本该判徒劳的 minor 被判有效，年轻代涨到 76.5 万条，最大停顿 12 → 16–19 ms）与**标记经 16 格预取环**（见 GC 页「标记的预取环」）。后者是 09 回退的根因修复：全是长链的堆按内存延迟标记（每对象一次缺失、一次只在途一个），硬件能藏掉多少缺失取决于布局巧合——main 的 80 B 对象跨行、相邻两条链的对象共用缓存行，R8 的 64 B 对象恰好一行一个、谁也不共用，于是同一个标记器在 R8 上全量标记每对象 40 → 55 ns（指令数相同、cycles +37~44%）。纯 Rust 微基准（LIFO 标记栈、出栈 CAS、读引用字，不经 z42）复现了这个形状：链距 3000 时 main 布局 25–28 ns/对象、R8 布局 65–68 ns；同一微基准里预取环（16 格）把两者都压到 7–14 ns。排除过的解释：slab coloring（按 chunk 错开块起点，微基准里无效——不是 2^18 链距的组别名）、标记 CAS 换普通 store、先读字段再 CAS、mark 字节换位置、`type_tag` 读，都不改变结果。加了预取环之后 R8 的标记反而更快：120 次 STW 全量标记 100 万个链式对象（链距 3000）cycles main 布局 5.68G、R8 5.13G。

    实测（release，macOS arm64，交错 11 次中位数，负载 9–14；`main` = 76ba8850e，`pol` = 年轻代策略，`pf` = 策略 + 预取环，`r8` = 策略 + 预取环 + R8，比值相对 main；停顿为 `GC.PauseStats` 的 max / p99 ms）：

    | | main | pol | pf | r8 |
    |---|---|---|---|---|
    | 13 分代 jit 墙钟 / 停顿 / 峰值 RSS | 1391 ms / 12.0 / 11.2 / 354 MB | 0.837 / 8.2 / 6.6 / 385 | 0.811 / 8.7 / 7.7 / 357 | 0.745 / 9.1 / 7.4 / 405 |
    | 13 分代 interp | 1765 ms / 13.2 / 10.9 / 360 | 0.909 / 10.5 / 6.4 / 368 | 0.882 / 9.7 / 7.9 / 360 | 0.859 / 8.5 / 8.1 / 360 |
    | 13 STW jit 墙钟 / 停顿 | 1081 ms / 81.3 | 0.996 / 79.9 | 0.881 / 55.0 | 0.864 / 60.8 |
    | 13 STW interp | 1486 ms / 78.5 | 1.011 / 79.4 | 0.912 / 54.5 | 0.895 / 59.8 |
    | 12 分代 jit / interp | 180 / 441 ms | 1.001 / 0.997 | 0.991 / 0.994 | 1.000 / 1.022 |
    | 12 STW jit / interp | 159 / 419 ms | 1.005 / 1.006 | 0.995 / 1.004 | 1.040 / 1.028 |
    | 12 ×4 长度 分代 jit / interp | 663 / 1704 ms | 1.002 / 1.000 | 1.002 / 0.997 | 1.015 / 1.021 |
    | 12 ×4 长度 STW jit / interp | 605 / 1649 ms | 0.998 / 0.997 | 0.995 / 1.003 | 0.961 / 0.998 |
    | 09 分代 jit / interp | 124 / 246 ms | 1.031 / 1.006 | 0.901 / 0.947 | 0.980 / 0.985 |
    | 09 STW jit / interp | 129 / 251 ms | 1.000 / 1.000 | 0.748 / 0.860 | 0.767 / 0.890 |
    | 09 ×4 长度 分代 jit / STW jit / 分代 interp（7 次，相对 pol） | 687 / 783 / 1155 ms | 1 | 0.648 / 0.523 / 0.806 | 0.643 / 0.444 / 0.811，RSS 506 → 406 MB |

    相对 main 与只有策略的 `pol`，R8（叠在预取环上）在 09、13 STW 停顿上都更好，12 短跑在 +0~4% 之内。相对 `pf`（同样有预取环）还剩的差距都是**回收边界的量化**：闸门按字节计，R8 的对象小，同样的字节闸门装下更多对象——09 的第一次全量标记要标 105 万个对象而 `pf` 是 84 万，13 STW 少一次 major 而最大的那次多清扫一截（major 次数 7 → 6，总 GC 时间更少、墙钟更快），12 STW 同样两次 major 每次多清扫 42% 的块；跑长 4 倍后 09 / 12 都与 `pf` 持平或更快（09 ×4 STW −15%、RSS −20%）。12 分代另有一次一次性的 major：R8 的 nursery 每次覆盖更多对象，`promotion_age - 1` 那一档幸存率读成 100%，自适应晋升把线降到 2，降线要靠一次 major。13 分代 jit 的峰值 RSS（`pf` 357 → `r8` 405 MB）随年轻代策略的 tenure 节奏走：关掉自适应 nursery 与超预算判定（`Z42_GC_PAUSE_TARGET_MS=0`）时 `pf` 365、`r8` 364 MB，interp 下两者都是 360 MB——是策略的 tenure 取舍，不是 R8 的布局。drop glue 改成只读一次 meta 位、不经 `Drop`（`ScriptObject::destroy`）。
    GREEN：`xtask test` 全部 stage 通过，`xtask test runtime` 1867 个测试通过。
  - R8 后续：① 回收闸门按字节计，对象变小后边界落在不同的对象数上（09 / 12 / 13 STW 的短跑量化差）——若要做到「同一程序同一决策」，major allowance 与初始 nursery 也得按条目计（✅ #1275）；② 自适应晋升降线要靠一次 major（✅ 见下一条）；③ 类型字换成不计数的引用需要描述符 arena；④ R9 数组头与元素合一块时沿用同一个头（种类字放元素描述）——✅ 见 R9 条。
  - R8 后续 ①②（分支 `perf/gc-entry-gates`，叠在 R8 上，四个提交）：
    - **默认闸门按分配次数陈述、按原字节值封顶**（`gc/arc_heap/entry_unit.rs`，见 GC 调参页「默认闸门按条目封顶」）：每次策略询问采样每次分配的字节，unit = min(16M, 256 Ki 次分配)，默认 nursery、major 下界（4 × unit）、切片间隔、停顿预算的界都用它；首次询问武装在 4M 探针。256 Ki = 64 B 一次的 16M，所以 09（64 B 节点）、13（每次分配 82–86 B）、z42c（120–190 B）闸门不变，只有 12（41–43 B，unit ≈ 10.5M）变：分代峰值 RSS 61 → 55 MB、STW 112 → 88 MB，墙钟持平。**参考尺寸取 64 B**（User 决定：墙钟代价小，RSS 已由结转收回大部分）；取 128 B（unit = 128 Ki 次）时 09 / 13 也按对象数触发——13 分代峰值 RSS 383 → 300 MB、墙钟 +17%，12 分代 RSS 61 → 50 MB、+3%（5 次中位数，下面两条修复之前测的）。
    - **降线不要 major**：定下降线的那次 minor 之后（没有周期开着）就降线，新线判老的那批整批摘出年轻集合，只给这批做晋升屏障（持有年轻孩子的对象 / 数组进记忆集），不再 `dirty_every_card`、不再为降线开 major。z42c 工作区构建：降线在那次 minor 上加 1.7 ms（56 486 个条目）；R8 上它等第一个周期收尾，那一片 4.7 ms，现在该周期最大切片 2.0 ms。12 分代（旧字节闸门下会降线）0.3 ms、整跑不再开周期。
    - **单代退避晚一步**：13 STW 的最大停顿不是对象变小造成的，是退避：活集搭建期那次回收徒劳，下一次（有产出的）回收之后闸门 ×4，一次清扫 256 MB。没被软上限挤小的增长闸门改为连续两次徒劳才乘倍数（`backoff / 4`），挤小的照旧。
    - **周期期间的晋升结转**：增量周期收尾不再把晋升字节计数清零，留下快照之后晋升的部分（周期期间出生的都按黑分配，这个周期收不回）。年轻代策略在 13 分代上多出的峰值 RSS 主要来自这里：tenure 大多跑在周期里，清零把它们忘了。
    实测（release，macOS arm64，交错 11 次中位数，负载 7–29，比值相对 R8；`A` = 前三条，`B` = 四条；STW 行是最终的「晚一步」版本，13 / binary-trees 上它与完全不乘的决定相同；停顿为 `GC.PauseStats` 的 max ms）：

    | | R8 | A | B |
    |---|---|---|---|
    | 13 分代 jit 墙钟 / 峰值 RSS / 最大停顿 | 1343 ms / 372 MB / 10.7 | 0.954 / 376 / 9.5 | 0.982 / 320 / 9.2 |
    | 13 分代 interp | 2105 ms / 362 / 21.7 | 0.829 / 364 / 11.8 | 0.934 / 323 / 10.1 |
    | 13 ×4 长度 分代 jit / interp | 4484 / 6193 ms，333 / 334 MB | 1.003 / 0.995，333 / 332 | 1.051 / 1.033，309 / 306 |
    | 13 STW jit / interp 墙钟（最大停顿，RSS） | 1014 / 1500 ms（62.1 / 62.9，484 / 481 MB） | 1.025 / 1.038（37.3 / 42.7，269 / 267） | 同 A（STW 不涉及第四条） |
    | 13 ×4 长度 STW jit / interp | 4120 / 5586 ms（63.9 / 63.5，482 / 479） | 0.915 / 0.963（39.6 / 39.4，259 / 257） | 同 A |
    | 12 分代 / STW jit（RSS） | 211 / 182 ms（61 / 112 MB） | 0.977 / 0.999（55 / 88） | 0.970 / 1.006（54 / 88） |
    | 12 ×4 长度 分代 / STW jit | 722 / 611 ms（62 / 112） | 1.006 / 1.003（57 / 88） | 0.980 / 1.002（57 / 87） |
    | 09 分代 / STW jit | 147 / 109 ms | 1.021 / 0.996 | 1.085（同样 6 片，噪声）/ 同 A |
    | 09 ×4 长度 分代 / STW jit | 466 / 384 ms | 0.969 / 1.119 | 0.999 / 同 A |
    | binary-trees 深度 18 分代 jit（RSS，最大停顿） | 7092 ms（248 MB，10.1） | 1.027（238，10.8） | 1.083（175，11.4） |
    | binary-trees STW jit / interp（RSS，最大停顿） | 7513 / 13510 ms（193 / 194 MB，58.5 / 56.9） | 1.087 / 1.051（139 / 137，44.8 / 44.7） | 同 A |
    | z42c 工作区构建 CPU user / 峰值 RSS（两轮各 7 次） | 13.7 / 446，12.2 / 473 MB | 13.2 / 462，12.0 / 458 | 14.0 / 443，13.0 / 436 |

    （09 / 12 interp、09 ×4 interp 分代都在 ±1% 内；09 ×4 STW interp 1.057；13 STW 短跑 jit 另两轮 1.016~1.036。完全不乘倍数时 09 ×4 STW jit / interp 1.49 / 1.20。）z42c 墙钟在负载下 7.2–21.5 s 抖动、分不出，产物逐字节一致（45 个）。小对象密度表（`GC.UsedBytes` / RSS 每对象，7 次中位数）三者相同：STW 16 / 20 / 24 / 32 B → 49.5 / 57.5 / 57.5 / 65.6 B，分代 65.9 / 67.2 / 52.6 / 68.6 B（B 的 `{int}` 分代一格 86.4 B，是峰值落在哪次回收上）。M9 当时的流失场景（binary-trees、13 ×4）：分代峰值 RSS 不升（B 降 29% / 7%），最大停顿持平。
    **User 决定**：保留第四条（结转：13 分代 RSS −12~15%、binary-trees −29%，墙钟 +3~13%、z42c CPU +2~7%）与第三条（STW 下 13 最大停顿 −38%、RSS −45%，09 ×4 长度 STW +6~12%、binary-trees STW +5~9%），参考尺寸 64 B。未做：按下一次 major 的实测代价（标记 ns/条目、清扫量）定 allowance——13 的回退都已由上面两条解释，加一个按时间的闸门会让触发点随机器负载漂移。
  - R9 ✅（#1307）：数组 = 变长区里**一个**块：16 B 统一头（GC 字 + 种类字）后面直接是元素，一次分配。定长区 `Region<T>`（`gc/region/`）、`GcRef` / `WeakGcRef`、`Tlab.arr`、数组卡表与数组清扫阶段全部删除，R6 记下的定长 `Region` Stacked Borrows 违规随之消失。`ArrayObj` 是盖住整块的 `#[repr(C)]` DST `{gc, kind, payload: [UnsafeCell<u8>]}`（元数据 = payload 长度，只在持有整块 provenance 的指针上构造）；`Value::Array` 载荷改为 `ArrRef`（数组块的 `VarGcRef`，8 B，方法集照 `ObjRef`），`Value` 仍 16 B，不改字节码 / 格式。**种类字放元素描述**：`ArrayDesc {mode, zero, elem_type}` 按「元素类型 × 模式」驻留在 `ElemTypeInfo` 里（CAS 发布、永不释放），128 B 对齐，种类字低 7 位装模式（bit 0–3）与 MIXED 的零值种类（bit 4–6）——取长度 / 宽度 / 零值、JIT 的打包与引用数据 helper 只读头，不做「头 → 描述符」的依赖 load（不打标签时 list_ops jit +4.4%，打标签 + helper 一次取齐后 +1~2%）。块类型按模式分：`ArrayPrim`（不含引用）/ `ArrayRef` / `ArrayCell`（MIXED）/ `ArrayValue`（FROZEN）/ `ArrayStruct`；长度 = payload >> 模式位移；STRUCT 数组 payload 前 8 B 是计数的 `Arc<StructTypeLayout>`，drop glue 的 `ArrayStruct` 臂释放它。分配只有一个漏斗 `MagrGC::alloc_array_with(desc, len, layout, init)`：取块、装种类字、在**未发布**的数组上跑初始化、发布、记账；payload 超过 u32 时按 OOM 返回 null。栈数组改为解释器栈区里的 `StackArray { elems: Vec<Value> }`，不再造 `ArrayObj`、也不再需要构造期独占写。分代：数组没有卡表了，含引用的数组与对象一样进**记忆集**（`BlockType::may_be_remembered`；字符串、闭包、原始类型数组不进）；降线时的晋升屏障、`Z42_GC_VERIFY_CARDS` 一并覆盖数组。侧表与对象同一套：finalizer、软引用按块地址 + `SIDE_BIT` 记账，弱引用 `WeakArrRef` 记 chunk 复用 epoch。M12 填洞时把种类字一并清掉（死数组的头里留着描述符）。对象身份哈希改为 MurmurHash3 的 fmix64（Fibonacci 乘法对 32 B 步长的块地址混合不足，低位分布测试不过）。`GC.UsedBytes` 对所有块统一按「头 + payload」计。
    **按块的卡表没有做，记忆集按数组整块重扫**：旧卡表每张脏卡同样整数组重扫（外加相邻卡），微基准（100 万元素的老 `object[]` 持续收进年轻元素）两边每次 minor 都约 6 ms 标记，最大停顿约 22 ms、总 GC 约 105 ms（20 次），持平——按卡扫是另一项优化，不是持平的前提。
    实测（release，macOS arm64，与 main 20f84a52a 交错 7 次中位数，负载 9–40）。100 万个数组装在 `object[]` 里、`GC.Collect()` 后读，(N=1M − N=0) / 1M，main → R9（RSS 含 `object[]` 每元素 8 B 与回收期标记队列的峰值）：

    | 形状 | 块 | `GC.UsedBytes` | RSS STW | RSS 分代 |
    |---|---|---|---|---|
    | `long[8]` | 80 B | 120 → 80 | 181.3 → 138.3 | 153.8 → 99.0 |
    | `int[0]` | 16 B | 56 → 16 | 113.2 → 48.6 | 109.2 → 64.9 |
    | `object[4]` | 48 B | 88 → 48 | 171.4 → 80.7 | 133.3 → 76.6 |
    | `byte[16]` | 32 B | 72 → 32 | 129.3 → 64.7 | 115.7 → 70.0 |
    | `string[2]` | 32 B | 72 → 32 | 129.3 → 64.7 | 117.4 → 60.7 |

    bench 墙钟 R9 / main（输出逐字相同；fa40 = fannkuch n=10，refarr / reflist 是 `object[]` / `List<object>` 读写循环）：

    | | 05 | 08 | 09 | 10 | 12 | 13 | dict_ops | list_ops | sort | fa40 | refarr | reflist |
    |---|---|---|---|---|---|---|---|---|---|---|---|---|
    | jit | 0.983 | 0.993 | 0.985 | 1.007 | 0.983 | 0.759 | 0.978 | 1.024 | 0.992 | 0.930 | 1.017 | 0.978 |
    | interp | 1.037 | 0.998 | 0.969 | 1.009 | 1.078 | 0.815 | 0.992 | 0.999 | 0.997 | 0.968 | 0.967 | 0.948 |

    interp 05 / 12 各 15 次复测 1.018 / 0.987。13 最大停顿（`GC.PauseStats` max）分代 jit 8.6 → 7.9 ms、interp（15 次）12.9 → 7.6 ms；STW jit 41.0 → 31.0 ms、interp 38.4 → 29.0 ms，墙钟 0.762 / 0.839。interp list_ops / 10 曾 +5–6%，10 不碰数组——是 `exec_function_body` 的布局扰动，叠上解释器对象分支外移之后消失。z42c 工作区构建（7 次，负载 10–16，`--no-incremental`）：墙钟 7.56 → 6.85 s、CPU user 13.58 → 11.95 s、峰值 RSS 429 → 333 MB，47 个产物逐字节一致。Miri：`-Zmiri-strict-provenance` 下 `gc::var_region` 干净；默认模式下 `gc::arc_heap_tests::{objects, holes, array_modes}`、`metadata::types::{object, owned_object, array_elems, array_sort}` 干净（故意泄漏的夹具需 `-Zmiri-ignore-leaks`）。

**需 User 决策**（⏸）：
- ~~删除 ConcurrentMarkSweep~~ —— 已定（User 2026-10-07：删）并已删除。`gc-mode` 只剩 `stw` / `generational`（及 `-mark-sweep` 别名）；传 `concurrent` 按非法枚举值处理：`--set` 致命（退出码 2），环境变量 / 配置文件警告并回落默认 `generational`，错误信息列出合法取值。
- 可 catch 的栈溢出（当前是致命错误；前置条件见 vm-architecture.md「原生栈预算」）。
- `int` 等窄整数的算术溢出不回绕到本宽度：`int.MaxValue + 1`、`int.MinValue / -1` 两路都得 `2147483648`，值仍按 i64 存、运算后不截断。要不要按声明宽度回绕、在哪一层截断（编译器插 Convert，还是 VM 按类型运算），属于语言语义决策。
- 栈闭包（D6）——**已决（User，2026-10-07）：删除运行时支持**。z42c 的逃逸分析从不给 `MkClos` 标栈分配，栈闭包路径整条不可达；`Value` 的栈闭包变体、帧 `env_arena`、GC 对它的扫描、`CallIndirect` 分支都已删除，闭包恒堆分配。以后要消掉不逃逸闭包的分配，走 JIT 标量替换，不再加值变体。
- GC 退避上限与「周期内的 minor 不判徒劳」（B1 落地后先重测，可能不再需要）：吞吐换停顿。13_gc_large_heap 分代模式下最大停顿 73.5→50 ms、p99 58.5→24 ms、不再出现 x64，代价是墙钟慢约 30%（`--large` 2.4 倍）。

**阶段 1 预期**【推断】：z42c 构建再快 1.3–1.5 倍（字段约 10%、帧约 10%、跨包 VCall，加上并行阶段吃到 JIT）。——**收官验证（§1.7）**：达成并超出（jobs=1 墙钟 −25%、并行 −40%，RSS ÷3），微基准累计 1.2–3.9×，13 场景分代不再劣于 STW。

### 内存 / GC 持续优化（User，2026-10-07：内存占用和 GC 离其他脚本语言差距还很大，持续发掘）

依据 §1.6，按收益排序。M 前缀与 P1-7 的 GC 方案互补。

| ID | 杠杆 | 预期 | 依赖 | 状态 |
|---|---|---|---|---|
| M1 | 数组直接在 GC 块里零初始化，去掉临时 `vec![Value; n]` | 每元素峰值 −16 B；`List<int>` 77.6 → 约 50 B | — | ✅ #1166 |
| M2 | 数组元素类型名不再每个数组一份 `Arc<str>`；压缩 ArrayBacking | 每数组 −24~−40 B | — | ✅ #1179 |
| M3 | `"k" + i` 直接格式化，不产生 ToStr 临时串 | 每次拼接少 40 B 垃圾 | — | ✅ #1180 |
| M4 | `stats()` 不再收集全部活对象 | 观测不再额外占用 16–32 B/对象 | — | ✅ #1172 |
| M5 | chunk 大小贴合分配器档位 | 每对象、每数组 −8 B | — | ✅ #1177（chunk 从页对齐 slab 切） |
| M6 | 按真实占用记账，软上限 / `Z42_GC_MAX_BYTES` 按真实占用判定（即 P1-7 A5） | 上限真正约束 RSS | — | ✅ #1177 |
| M7 | 池中空 chunk 超阈值 decommit（即 P1-7 A4） | 稳态 RSS 下降 | — | ✅ #1177 |
| M8 | 用位图 / 区间替代 young_list、all_blocks、var young 等侧表 | 分代模式每对象 −8 B、每串 −16 B；实测（分代）每对象 98 → 81 B、每串 115 → 89 B，槽头 72 → 64 B，z42c 构建 RSS 788 → 754 MB、墙钟持平；09 STW full mark +10%（64 B 步长，待查） | — | ✅ #1198 |
| M9 | 年轻集合增长加上限（futility 时提前晋升或转 major，而非放大 nursery） | 流失场景 RSS −13%，最大停顿 94 → 18 ms | 09_alloc_ctorless 吞吐退化（⏸） | ✅ #1190（按回收收益判定）；**2026-10-13 年轻代策略按条目与实测代价判定**：徒劳线兼看条目（回收 < 扫描条目的 1/16 也算徒劳），新增「超预算」判定——暂停模型的条目预算减去本次 nursery 将带来的新条目，就是留给幸存者的余量，幸存者达到余量且不少于扫描量一半就安排 tenure（对象变小时判定不漂移，R8 前置）。13 分代 jit 最大停顿 12.3 → 7.9 ms、p99 11.3 → 6.7 ms、墙钟 −10%（interp 13.5 → 8.6 ms、−7%），**代价**：13 分代峰值 RSS 344 → 380 MB（+10%，tenure 把少量年轻垃圾送进老年代、major 次数 208 → 134 片）——按 User 2026-10-13 决定先合；**2026-10-14**：多出的 RSS 来自增量周期收尾把晋升计数清零（周期里 tenure 进来的量被忘掉），改为结转之后 13 分代 jit 峰值 RSS 376 → 320 MB、墙钟 +3~13%（结转与退避已随 #1275 合并，User 决定保留） |
| M10 | 字段 payload 内联进 GC 槽 | 每对象 −16~−24 B；死 payload 随槽一起释放 | — | ✅ 对象模型 R1–R4（#1192 #1203 #1214 #1225）＋ R6（#1247）＋ R8（#1268）＋ R9（#1307）；收官密度见 §1.7 |
| M11 | 对象头 72 → 约 24 B：去掉每对象 Mutex，标记 / 存活 / 年龄 / 代号合成一个字，稀有字段移到侧表 | 每对象 −40~−48 B | 内存模型已定（见「对象模型（M10 + M11）已定方案」），按 R0–R9 推进 | ✅（R0–R9 完结）R4 #1225 去掉每对象 Mutex（槽 64 → 56 B）；R5 Monitor 侧表暂无消费方（语言没有 `lock(obj)`，锁都在 `NativeData::Monitor`），随 `lock` / `Volatile` 暴露再做；R7 即 M8（#1198）；R8 ✅ #1268 一个 16 B 头（空类 16 B、`{int}` 24 B、`{int; string}` 32 B，见上文 R8 条）；R9 ✅ #1307 数组同一个头 + 元素一块（`long[8]` 80 B、`object[4]` 48 B，见上文 R9 条） |
| M12 | 空洞复用（TLAB 认领半活 chunk） | 流失场景 RSS 降 2–4× | M10 | ✅ #1258 变长区：TLAB 每个 size class 一批（64 个）从 region free list 取死槽、锁一次、无锁原址填回，保留 tombstone 时 +1 的 generation（只按 class 原址复用，不切 span：块头位置、陈旧句柄的 generation 守卫、free list 条目都不需要新规则）；取时记账 + 进年轻集合、retire 时撤销没用上的；填进空洞的块 `alive` 延后到 funnel 写完 payload 才置位（release）；flagged-dead 非空时不发槽；槽的 generation 超过 chunk `reuse_gen` 65 535 后不再复用（16 位句柄快照不回绕）。实测（release jit，交错 11 次中位数，负载 3–5，base → new）：13_gc_large_heap 峰值 RSS 分代 728 → 341 MB、STW 597 → 479 MB，稳态 647 → 334 / 585 → 479 MB；12_gc_churn 峰值 84.4 → 66.8 / 126.4 → 109.1 MB；z42c 工作区构建峰值 RSS 695 → 455 MB（6 次，产物逐字节一致）；墙钟 12 分代 −4%、12 / 13 STW 持平、09 +1%、13 分代 +6~7%（cycles +6%：局部性 + nursery 策略级联，固定 nursery 时反而 −12%），z42c +2.4%（cycles +2.3%，指令 +0.5%）。墙钟代价按 User 2026-10-13 决定先接受（「现在就合，局部性另做」）。后续：① 局部性——每 chunk 死槽位图、按连续空闲段认领（同一段内顺序 bump，不同大小的块仍挨着）；② 自适应 nursery 对 minor 成本的判定，避免局部性变差时 nursery 收缩级联成更多晋升 / major；③ 定长区数组头（A2）不再需要：R9 之后数组也在变长区，同一套空洞复用。 |
| M13 | 泛型 `T[]` 的实参为原始类型时按类型打包 | `List<int>` / Dictionary 值每元素 16 → 4 B | 已定排在 M10 / M11 之后；编译器配合 | ✅ #1310：纯运行时方案、零格式 bump——分配点把型参解析成基元实参时（类级经接收者的驻留 `TypeArgs`（M14 条目上预算每实参的 `ElemKind`），方法级经 `frame.method_type_args`，interp / JIT 共用 `class_type_param_elem`）按该宽度分派 PRIM 块，引用 / struct / 未解析实参保持 MIXED，`element_type` 名字与反射输出不变；`ElemKind::of_type_param` 与 `default_value_for` 同词汇（含 FQ 包装名）；`sort_prims_prefix` 补 PRIM 臂；`SetValue` 错误种类从「单元什么都收」变抛 `InvalidCastException`（与 `int[]` 一致）；`ArrayNewLit` 无型参载荷，字面量站点维持 MIXED。实测（release，交错 5 次中位，负载 9–15）：`List<int>`(1M) used 16.0 → 4.0 MB、`List<long>`/`List<double>` 16.0 → 8.0、`List<string>`（对照）不变、`Dictionary<string,int>`(0.5M) 46.0 → 36.6 MB（−20%，值侧打包）；z42.core bench（dict / list_add / list_sort）两轮全 overlap；z42c 工作区构建墙钟 0.984×、峰值 RSS 348 → 337 MB，47 个产物逐字节一致 |
| M14 | 泛型实例的类型实参按实例化进程级驻留（`TypeArgs`，8 B 细指针，同 M2 套路），对象的冷元信息合成一个 8 B 标签字，只有带原生底座的对象才分配 `ObjExtras` 盒子 | 每个带实参的泛型实例 −55~−105 B RSS（实测：闭合 `Box<int>` 210 → 155 B、`Pair<int,string>` 258 → 153 B、`LinkedList<int>` 节点 214 → 158 B）；泛型分配密集负载快 30–40%，`LinkedList<int>` 建表快 7–10%；z42c 工作区构建墙钟 / RSS 持平、产物逐字节一致。已知边界：擦除分配（`new Node<T>()`）的实参仍是字面 `T` | — | ✅ #1216 |

预期：M1–M8 落地后，对象约 85 B、数组约 190 B、`List<int>` 约 50 B；M10 + M11 之后，对象约 35–40 B（约 .NET 的 1.3×）；再加 M12，流失场景的 RSS / 活集从约 8× 降到 2–3×。

### 对标差距（User，2026-10-07：全面对标，争取各方面都有不错的表现）

跨语言对标基准今后放在独立仓库 z42-lang/Benchmarks，届时系统补充；下表是本地一次测量的结果（24 条负载，M2 Ultra，load 4–9，z42 jit + 分代 GC，对照 Python 3.9 / Node 26 / Ruby 2.6 / .NET 10 / Java 21）。先处理连 CPython 都不如的几项：

| ID | 负载 | z42 / Python | z42 / Node | 初步判断 | 状态 |
|---|---|---|---|---|---|
| T1 | json | 28× | 40× | 待剖析 | ✅ #1186（json 1881 → 669 ms） |
| T2 | str_builder / str_split_join | 7.9× / 13× | 11× / 10× | StringBuilder 是纯脚本实现，每次 Append 生成一个 GC 字符串。JIT 已内联 `s.Length` / `s[i]` / `CharAt`（全 ASCII 串直接 load，[jit.md](internals/src/runtime/jit.md#字符串原语内联)）：`char` 比较也已原生：str_builder 1012 → 547 ms（Python 242，≈ 2.3×），逐字符扫描段 493 → 63 ms；剩余在 `Append` 的方法调用帧 / 入口字段 hoist / `string[]` 写屏障与扩容，以及 GC | ✅ #1186 #1226（原生 Split / Join；JIT 内联 Length / 字符访问） |
| T3 | dict_ops / sort | 4.8× / 3.7× | 7.3× / 7.4× | 待剖析（泛型装箱、比较器调用、哈希路径）。**Dictionary / HashSet 按插入顺序遍历（User 2026-10-07）**：改为 CPython 式插入有序紧凑哈希表（稠密 entries + `int[]` 索引表，首槽取 hash 低位、冲突后 perturb 探测）；dict_ops n=2M 2.02 → 1.75 s、RSS 286 → 165 MB，n=5M 超过 240 s → 4.7 s；sort 未动 | ✅ #1176 #1183（基元原生排序 sort 1883 → 180 ms；插入有序紧凑字典） |
| T4 | large_heap | 1.6× | 5.9× | GC 策略与内存复用（P1-7 / M 系列） | ✅ 随 P1-7 / M 系列（收官 §1.7：13 分代墙钟 943 ms、停顿 7.5 ms、RSS 更低） |
| T5 | closures | 1.3× | 8.3× | 闭包调用按名（P1-2 PR 8）。**PR 8 后重测**（2026-10-08，n=2M，jit，7 次交错中位，负载 12–37）：z42 365 → 330 ms（−9%），Python 392 ms、Node 61 ms ⇒ 约 **0.84× Python / 5.4× Node**（同机同轮的 PR 8 之前：0.93× / 6.0×）。调用路径上已无名字哈希；剩下的大头是每次迭代的 env 数组 + 闭包块分配与 minor GC、以及原生调用的建帧 | ✅ #1211（P1-2 PR 8） |

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

- **微基准**：先 `z42c --emit-zbc src/benches/scenarios/<n>.z42 <n>.zbc --opt-all`，再 `z42vm --mode jit|interp <n>.zbc <Namespace>.Main`，取 5 次中位数。
- **真实负载剖面**：
  1. 在 `src/compiler` 下执行 `z42c build --workspace --release --output-dir <tmp> -q`，事先把 stdlib 的 zpkg 拷进 `<tmp>`；
  2. 运行期间用 macOS 的 `sample <z42vm pid> 18 -file out.sample` 采样；
  3. 按函数名统计包含时间（对每条调用链只计最外层匹配）。
  - `Z42_PORTABLE_VM=<z42vm>` 可以让同一个 z42c 跑在指定的 VM 上，便于新旧对比。
- **GC**：`Z42_GC_TRACE=1`（每次回收一行）、`Z42_GC_PHASES=1`（trip 行带退避倍数 `xN`），`--set gc-mode=stw|generational`。
- **惰性加载**：`Z42_LOG=z42::metadata::lazy_loader=debug`，统计 `lazy-loaded zpkg` 行数。
- **启动**：`/usr/bin/time -l`（RSS），用 Python `subprocess` 循环计时取中位数。