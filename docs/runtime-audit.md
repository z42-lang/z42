# z42 运行时审计与改进计划（临时迭代底稿）

> **临时底稿**：本轮运行时改进迭代的工作文档，不属于三本书。迭代完成后删除本文件（同时移除 `docs/README.md` 里的入口链接）；届时仍未完成的项转入 `docs/internals/` 对应机制页的「待办 / Deferred」（类别登记见 [doc-system.md](agent/rules/doc-system.md)）。
> 范围：`src/runtime`，基线 origin/main（2026-10-06）。方法：按子系统（值模型/IR、加载链路/启动/配置、解释器、JIT、GC、corelib/native/host/VmContext、构建打包）逐个只读审计 + 实测（release z42vm，macOS arm64：`sample` 采样、GC trace、计时、复现程序）。
> 证据级别：**【复现】** 有复现程序（附录 A）；**【实测】** profile / trace / 计时（方法见附录 B）；**【确认】** 读代码确认；**【推断】** 有代码依据但需测量。
> 计时环境负载较高，绝对值可能偏高 20–40%，相对比例可信。文中行号对应基线版本，随修复漂移。

---

## 裁决记录（2026-10-06，User）

| 议题 | 裁决 | 依据 |
|---|---|---|
| 本文件位置 | 直接放 `docs/` 根目录；迭代完删除；做不完的项转入 internals 相关机制页备忘 | doc-system.md 已登记「临时迭代底稿」类别 |
| 整数 `long.MinValue / -1`、`% -1` | **wrapping**：`MIN / -1 = MIN`，`MIN % -1 = 0`，永不因此抛异常；除零仍抛 `Std.DivideByZeroException` | 与已定的「加减乘溢出 wrapping」一致；由 `semantics.rs` 统一实现，interp / JIT helper / JIT 冷路由三路共用 |
| 栈溢出 | **不支持 catch，作为致命错误**：每个 z42 帧入口检查剩余原生栈，不足时作为 VM 内部错误一路返回（z42 `catch` 拦不住；宿主 API 拿到错误码；z42vm 打印 z42 调用栈后以非零码退出）；VM 创建的线程栈大小可配置；`SA_ONSTACK` + `sigaltstack` 兜住原生代码自身的溢出 | User 规则：只要存在不能捕获的场景，就不支持捕获。核实结论见下 |
| 批量授权 | §7 阶段 0 清单全部批量授权：逐个 PR → 完整 GREEN → auto-merge → 自动开始下一个；命中 workflow.md「批量授权模式」的中断条件即停下询问 | workflow.md |

**栈溢出为何不能完备支持 catch**（核实结论）：

1. **重入路径会拍平或吞掉异常**：`interp::run_returning`（`interp/entry.rs:49`）把 z42 抛出的异常转成 anyhow 字符串，原类型丢失；`interp::dispatch::obj_to_string`（`dispatch.rs:251/272/293`，字符串拼接、插值、`Console.WriteLine(obj)` 都经过它）把 ToString 抛出的异常吞成 `"<exception: …>"` 字符串后继续执行；builtin 错误通道（`anyhow::Error` → `Std.Exception`）同样丢失异常类型。
2. **两次检查之间的原生代码栈用量无界**：例如 `Value` 的递归 Debug 格式化（`gc/refs.rs:611-626`，被大量 `bail!` 消息使用）、调用点同步执行的 Cranelift 编译。溢出一旦发生在原生代码里，就只能命中 guard page，无法安全地转成异常。
3. **JIT helper 是 `extern "C"`**：任何 panic 都会直接 abort，所有路径都得逐个审计、改成返回码。

可 catch 的 `StackOverflowException` 记为 Deferred，前置条件：类型化的 builtin 错误通道（Builtin ABI v2）、修好上述重入点、原生递归有界、JIT 编译不在深栈上执行。

---

## 0. 结论速览

1. **头号系统性问题：运行时的"身份"全程按名字。** 函数/类型/字段/方法从解码起就被还原成 `String`，运行期再靠一堆哈希表和旁路缓存"重新压缩"。wire 格式本来是紧凑的下标与 token，加载器先解压成字符串、执行时再哈希回去。实测 z42c 构建里，仅"cctor 是否已运行"的按名检查就占 **约 11–14%**（当前代码约 10.7%，其中一半是对函数名做 `rfind('.')`），JIT 字段 helper 按名解析占 **约 10%**。惰性包函数没有整数身份，导致 PIC 永远缓存不了跨包目标，而 z42c 的代码全在惰性包里。
2. **调用协议是解释器与 JIT 共同的天花板。** 每次调用约 11 次原子 RMW、3 次加锁、4 次 TLS、2 个 `Arc<str>` 引用计数，JIT 还要经 4 次 helper。单态虚调用 JIT ≈52 ns、解释器 ≈94 ns；fib 递归每次调用 ≈140 ns【实测】。帧管理在 z42c 里占 **约 10%**。
3. **对象模型与 GC 的内存开销是 .NET 的 2.5–3 倍，RSS 是 GC 记账堆的 4.5–6 倍。** 72 B 头（含每对象一把 Mutex）+ 单独 malloc 的负载；TLAB 不复用死槽；`used` 记账口径错误。默认的分代 + 增量模式有一个自我强化的策略环：周期内新生对象 born-black，minor 判"徒劳"，退避放大到 ×64。结果在大堆场景下**比纯 STW 更慢也更大**【实测】。
4. **已复现 5 个正确性/健壮性缺陷**：OSR 跳过 ref 形参回写（静默算错）、`long.MinValue / -1` 让 VM panic 或 abort、压缩库错误路径自死锁（永久挂起）、递归约 4000 层即静默崩溃（无异常、无任何输出）、一次自由函数调用就惰性加载全部 16 个 stdlib 包。
5. **JIT 在结构上只能单线程。** worker 线程永远走解释器（`jit_ctx` 恒为 0），z42c `--jobs N` 的并行阶段全部是解释执行【确认】。
6. **组件化的前提尚不成立。** `componentized-runtime.md` 假设"interp→jit 零依赖"，实际有 12 处；metadata ↔ gc ↔ vm_context 成环；GC trait 有 58 个方法却只有 1 个实现，还大量泄漏实现细节；VmCore 是上帝对象，挂着 socket/文件/进程表；`#[cfg(target_os)]` 在 PAL 外有 123 处、PAL 内只有 13 处。
7. **z42vm 不应改为动态链接 `native/libz42`**（见 §6）。实测动态与静态没有性能和 RSS 差别，Rust 下的代价却真实存在。真正的问题在打包：SDK 的 75–79% 是没人用的嵌入件和不可用的 `.a`；发布的 dylib 带 CI 绝对路径 install_name、没有 SONAME、缺 `z42.dll.lib`。

---

## 1. 实测数据

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
| hello + 1 次自由函数调用 | 24.4 ms | — | RSS 23 MB（不调用时 11 MB），原因见 §3.5 |

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

### 1.5 递归深度上限【复现】

| 模式 | 主线程（8 MB 栈） |
|---|---|
| 解释器 | 4000 层正常，**5000 层静默崩溃**，约 1.7–2 KB 原生栈/层 |
| JIT | 1 万层正常，2 万层崩溃 |

worker 线程只有 2 MB 栈，解释器上限约 1000 层。崩溃没有任何输出：没有 z42 异常，没有 z42 栈，连 Rust 的 "overflowed its stack" 提示也没有。

---

## 2. z42vm 二进制与发行包

### 2.1 z42vm `__text` 构成（macOS arm64，共 4.97 MB）

| 组成 | 体积 | 占比 |
|---|---|---|
| z42 VM 本体 | 1.46 MB | 29% |
| Cranelift + regalloc2 | 1.35 MB | 27%（Linux x64 上 3.82 MB / 47%，其中 `cranelift_assembler_x64` 1.88 MB） |
| rustls + ring + webpki | 0.57 MB + 约 0.2 MB 表 | 无条件依赖，移动端 staticlib 也带着 |
| regex-syntax / automata | 0.29 MB | 只因 tracing-subscriber 开了 `env-filter` |
| clap | 0.20 MB | |
| toml | 0.11 MB | |
| std | 0.53 MB | |

- `bincode`、`thiserror` 在整个 workspace 都没被使用；`serde_json` 只有 `config/render.rs` 在用；IR 上的 serde derive 只服务于测试【确认】。

### 2.2 SDK 里的运行时副本（macOS）

- 同时有 3 份完整运行时：`bin/z42vm` 7.6 MB（静态单体）、`native/libz42.dylib` 6.2 MB、`native/libz42.a` 15.3 MB。另有 `native/libz42_compression.a` 23.7 MB，这个文件**用不上**：desktop 没有静态注册 API，iOS/Android preset 已经内置 compression。
- `native/` 占 SDK 解压体积的 75%、tgz 的 72%；Linux 上是 79%。SDK 内没有任何组件消费 `native/libz42.*`。

---

## 3. 已复现 / 已确认的缺陷（P0：先修）

| # | 缺陷 | 证据 | 位置 | 修法 |
|---|---|---|---|---|
| 1 | **OSR 跳过 ref 形参回写，静默算错** | 【复现】`Wrapper→Acc(ref s, 20000)`：JIT 输出 `0`，interp 输出 `199990000`；`jit_native_from_interp: 1` | `interp/mod.rs` 的 OSR 返回 `return outcome` 早于 `run_ref_writebacks`；`exec_support.rs:170-208` | `try_osr` 在 `!frame.ref_writebacks.is_empty()` 时拒绝 OSR；补一条 `Z42_OSR_THRESHOLD=1` 的 golden |
| 2 | **`long.MinValue / -1`、`% -1`**：interp 触发 Rust panic，catch 不到；JIT 在 `extern "C"` 里 panic，进程 abort | 【复现】`panicked at src/interp/exec_value.rs:108:56: attempt to divide with overflow`；JIT 栈落在 `jit_call → panic_cannot_unwind → abort` | `exec_value.rs:108,118`；`jit/helpers/arith.rs:117,141`；`semantics.rs` 只统一了加宽规则，运算本身由两端各自传闭包 | 先定语义（C# 抛 OverflowException，或 Java 式 wrap），在 semantics 写一份 `int_div/int_rem` 三路共用；golden 补 MIN/-1；文档 `interp-jit-semantics.md:113-115` 声称"差分测试已钉住"，但测试不存在 |
| 3 | **压缩库任何错误路径都会自死锁**：损坏数据让 VM 永久挂起 | 【复现】`Deflate.Decompress(垃圾数据)` 挂住，CPU 0%，栈为 `wrap_deflate_decompress → last_error_string → parking_lot lock_slow` | `native/ext.rs:528-566` 持有 `LOADED_COMPRESSION` 锁时再调 `last_error_string()` 二次加锁（不可重入），另有 10 个同构包装器；`brotli.z42:56-58` 记录过"错误路径测试会挂"，测试被删了 | `OnceLock<LoadedCompression>`（里面全是 Copy 的 fn ptr），调用时不持锁；补回错误路径测试；顺带解决全进程压缩串行化 |
| 4 | **没有栈溢出保护**：约 4000 层递归静默崩溃 | 【复现】见 §1.5 | 解释器在 Rust 栈上递归，`exec_function_body` 是 70 KB 的单函数，栈帧很大；signal-hook-registry 安装 handler 时不带 `SA_ONSTACK`，把 Rust 的溢出提示也吞了；`std::thread::spawn` 默认 2 MB 栈 | push_frame 时检查剩余栈（入口记录栈底，低于约 128 KB 时抛 `StackOverflowException`）；线程栈改成可配置（默认 16 MB，与嵌入入口一致）；自装 `sigaction` 带 `SA_ONSTACK` 并设置 `sigaltstack`；冷 handler 标 `#[inline(never)]` 缩小热帧 |
| 5 | **惰性加载被 cctor 屏障击穿** | 【复现】hello + 1 次自由函数调用，惰性加载 16 个 stdlib 包（cli/compression/crypto/net/regex/yaml…），启动 10.2→24.4 ms，RSS 11→23 MB；`z42c --help` 加载 16 个依赖包 | `cctor.rs:568-579`：自由函数的 owner 取到的是**命名空间**名，送进 `try_lookup_type`，在 `lazy_loader/resolve.rs` 走 Fallback B，把所有未加载包装到不动点 | §4.1 的 owner 预计算；禁止把派生名送进 `try_lookup_type`；成员查找路由到属主包 |
| 6 | `jit_get_bool` 出错时返回 255，调用方不检查 | 【确认】`helpers/value.rs:188-199`、`translate/term.rs:74-79`：255 被当作 true 走真分支，挂起的异常被吞掉 | | 返回状态 + 值，或先检查 255 |
| 7 | `pending_thrown` 跨线程共享，且不是 GC root | 【确认】`vm_context/types.rs:98` 挂在 VmCore 上；根扫描器 `construct.rs:283-352` 不扫它 | | 移到 VmContext 并加入根扫描；长期改成类型化 `BuiltinError` |
| 8 | cctor `claim` 等待时既不 park 也不 poll | 【确认】`cctor.rs:382-427`，200 µs sleep 轮询，30 s 超时 | | GC 要等满 30 s，然后抛出伪造的"循环初始化"异常；等待改为 Condvar 加 `NativeParkGuard` |
| 9 | host `invoke` 在全局 `HOST` 读锁下执行用户代码 | 【确认】`host/mod.rs:401-410` | | 用户代码重入调 load/shutdown 会死锁；先 clone 出 `Arc` 再释放锁 |
| 10 | `z42_host.h` 声称结构体尾部追加字段向前兼容 | 【确认】`config.rs:160` 无条件读取新字段 | | 用旧头文件编译的调用方会被越界读（UB）；加 `struct_size` 字段或升 abi_version |
| 11 | socket 读期间把 stream 从表里摘走 | 【确认】`tcp.rs:231-247,294-309`，UDP/TLS 同构 | | 全双工写会拿到 handle_invalid；读期间 Close 无效，读完 socket 又被放回（fd 泄漏）；改成 `Arc<TcpStream>` + closed 标志 |
| 12 | 身份哈希低 3 位恒为 0 | 【确认】`corelib/object.rs:243,248` 是 `addr & 0x7fff_ffff`；`Dictionary.z42` 用 `h & mask` | | 对象作 key 时只有 1/8 的桶能作首选位置；乘法混淆一行即可修 |
| 13 | 异常表 try_end 哨兵永远对不上，两后端行为不一致 | 【确认】`func_reader.rs:109-113` 生成不存在的 `block_{len}`；interp 用 `?` 终止整个 handler 搜索，JIT 用 `continue` | | 改成 u32 下标 + 左闭右开 |
| 14 | `is_exception_subclass` 只查入口模块 | 【确认】`exception/mod.rs:234-245` | | 惰性包里的异常层级会丢 StackTrace/Message【推断】；改调 `isa_td` |
| 15 | 每对象 Mutex 被 JIT 绕过 | 【确认】JIT 在 hoist 时取出 bytes 指针即释放锁，之后裸写 | | 按 Rust 语义是数据竞争（UB）；`__array_copy` 先锁 src 再锁 dst，存在 ABBA 死锁面【推断】 |
| 16 | `isa_cache` / `subclass_memo` 以地址为 key | 【确认】 | | 可回收 ALC 被回收后不失效，地址复用可能误判【推断】 |
| 17 | GC 请求可能丢失，丢失后自动回收永久停摆 | 【确认窗口，推断概率低】 | `auto_collect.rs:196` 把 `next_collect_at` 设为 `u64::MAX`；而 `request_gc_pause` 的 CAS 失败、`pause_count>0` 两条路径都不 rearm | |
| 18 | OSR 后栈闭包 env 丢失 | 【确认代码；附录 A 的用例未能复现，可能没走到栈闭包】 | `from_interp_regs` 把 `env_arena` 置空（`jit/frame.rs:59-66`） | |
| 19 | **`obj_to_string` 吞掉 ToString 抛出的异常** | 【确认】`interp/dispatch.rs:251,272,293`：`Thrown(v) => Ok(format!("<exception: …>"))` | 字符串拼接、插值、`Console.WriteLine(obj)` | 遇到抛异常的 ToString 时静默继续；改为传播 Thrown |
| 20 | `run_returning` 把异常拍平成 anyhow 字符串 | 【确认】`interp/entry.rs:49` | `GetCustomAttributes` 的属性工厂等 | 异常类型丢失；改用保留类型的 `run_outcome` |

---

## 4. 性能：根因与改法（按收益排序）

### 4.1 根因一：身份按名字（z42c 中约 25% 以上可直接消除）

**现象**【确认 + 实测】：
- **cctor 屏障**：`CctorRegistry::pending` 只有在所有已登记类型的 cctor 都跑完后才归零。boot 时登记了 18 个 z42.core 类型（Std.Int32/Double/Char/Math/IO.Path…），一个 trivial 程序跑完它们一个都没执行，所以**门永远开着**。之后每次 Call 都要做：`rfind('.')` → `type_registry.get(owner)` → `try_lookup_type(owner)`（加锁 + 哈希 + Arc）；自由函数的 owner 是命名空间，必然 miss，还要拿 lazy_loader 读锁并查负缓存。文档（`static-ctor-init.md:64-65`、`cctor.rs:9-23` 注释）说"稳态是一次 relaxed load"，实测不成立。
- **惰性包函数没有整数身份**：VCallIC 载荷是入口模块 `functions` 下标，所以 Lazy 目标**永不进 PIC**。每次跨包虚调用都要重走：TypeDesc Arc clone、vtable 线性字符串比较、func_index miss、`try_lookup_function`（锁 + 哈希）；JIT 还要再查一次 `LazyTable`（全局 Mutex + SipHash）。z42c 的分派站点（Call 10252 / VCall 1245 / ObjNew 2116 / FieldGet 19539 / StaticGet 2070 / ConstStr 6231）**全部在惰性包里**。
- **ObjNew** 每次都哈希类型名和 ctor 名（resolver 算好的 `type_tokens` 没用于派发，`exec_object.rs:13-18` 自己承认）。
- **CallIndirect / FuncRef / 闭包**：每次 `name.to_string()` 分配 + 哈希；`LoadFn` 每次新分配一个 GC 字符串。
- **ConstStr**：每次锁 + 哈希，惰性包里要查两遍。
- **JIT helper** 每次执行都对名字做 `from_utf8`。其中 `jit_vcall` 是把 `from_utf8` 作为 release 下空函数 `assert_pic_target` 的实参，依然会被求值（`helpers/vcall.rs:52-55`）【确认 + 实测 93 样本】。
- **三套函数注册表**（`Module.func_index` 键 u32 / `LazyLoader.function_table` 键 String / JIT `LazyTable` 键 String 外加 Mutex），**两套类型注册表**，**每个调用点三套并行缓存**（`method_tokens` / `cross_module_targets` / `call_jit_ic`）。

**改法（root fix：链接一次，之后全程用整数）**：
1. VmCore 上建进程级 **append-only `FuncTable` / `TypeTable`**：分块存储，`AtomicU32 len` 用 Release 发布，读取无锁。入口模块函数占 0..n，惰性加载的函数逐个追加 FnId。TypeId 本来就是全局唯一的。
2. `Function` 上挂一个 MethodDesc 式的运行期槽：`owner_init: OnceLock<OwnerInit{None | Type(&TypeDesc)}>`、`jit: {state, code ptr, calls}`。自由函数和没有 cctor 的类型直接定为 `None`，永久免检。JIT 编译时就已知 owner：没有 cctor 的不生成屏障，有的内联成 `load gen; cmp; brif cold`。
3. 编译器侧引入 **beforefieldinit** 语义：只含字段初始化器的合成 cctor 打上类标志，这类类型的静态方法调用不设屏障。
4. PIC 载荷、`method_tokens`、ObjNew 站点、FuncRef、Closure 全部存 FnId / TypeId；三套缓存合一；IC 能缓存 Lazy 目标。
5. 解码时把 token 解析成 FnId / TypeId / StringId，名字只留在侧表供反射和诊断用（`string_id.rs` 的 Phase B，`ir.md` 里 deferred 的 slim-instruction-stringid）。

**收益**【推断】：cctor 屏障（当前代码约 10.7%，含静态读）→ <1%；跨包 VCall 100–300 ns → 约 5 ns；同时修掉 §3 第 5 条的惰性加载击穿。这也是 load context 卸载、多 VM 共享镜像、AOT（不再烘焙绝对地址）的前置条件。

### 4.2 根因二：调用协议与帧模型（两后端共同的天花板）

**现状**【确认】：每次 interp→interp 调用包括：
- `call_stack: Arc<Mutex<Vec<VmFrame>>>` 加锁 3 次：更新行号、push、pop；
- `VmFrame` 96 B，带两个 `Arc<str>` 名字（clone/drop 共 4 次 RMW）；
- `next_frame_id` 一次 `fetch_add`；
- 4 次 TLS：寄存器池取/还、`VmGuard`、`HeapGuard`；
- `resize(max_reg, Null)` 写满寄存器；
- 每次 return 都无条件调一个不内联的 `run_ref_writebacks` 去遍历空 Vec；
- 每个回边都 out-of-line 调一次 `try_osr`，返回值走 sret。

JIT 的每次调用 = 4 次 native→Rust helper（vcall/call、`jit_regs_ptr`、`jit_set_ret`、入口 hoist 的 field slot）+ 同一套帧协议。**没有 JIT→JIT 直接调用。** 这套"建帧 / push / 调用 / pop / marshal"序列在代码里有 **10 份拷贝**。

**改法**：
1. **v1（低风险）**：
   - `VmFrame{*const Function, regs, pc: Cell<u32>, base}`，名字、文件、行号在生成栈回溯时由 func+pc 懒算；
   - `call_stack` 改为 owner-only 的 `UnsafeCell`：跨线程只在 owner 已 park 时读，safepoint 握手已经提供了 happens-before；
   - 寄存器池挪进 VmContext；guard 只在引擎入口安装；
   - writeback / OSR / frame_id 都加内联快门；
   - JIT 侧：`regs_ptr` 作为帧首字段直接按偏移 load，`Ret` 直接写 `frame.ret`。
2. **v2**：每线程一块**连续 Value 栈**，供 interp 与 JIT 共用。Lua 式寄存器窗口传参（零拷贝），帧头只有 2 个字，GC 扫 `[0, top)`。三个 arena 合并为栈上 bump；`RefKind::Stack` 改用绝对下标，顺带消掉 `store_thru_ref` 把 `*const` 强转 `*mut` 的别名 UB。
3. **v3**：JIT 调用点经 FnId 的 code 槽直接 call（stub 负责惰性编译），helper 只留给慢路径。

**收益**【推断】：10_mono_vcall interp 97 → 约 50 ns；JIT 54 → 约 5–10 ns；z42c 帧相关的约 10% 基本拿回。

### 4.3 根因三：对象模型与字段访问

**现状**【确认】：
- 字段读要走：句柄 → RegionEntry（2 次 Acquire + 对象 Mutex）→ ScriptObject → `Arc<TypeDesc>` → cold Box → **clone 一次 `Arc<ObjectLayout>`**（`type_desc.rs:293-295`；不 clone 的 `_ref` 版本就在旁边第 299 行，只有 GC 在用）→ field_access → ObjStorage（另一个 malloc 块）。一次字段访问 4 次 RMW、约 10 次依赖 load。
- FieldIC 只缓存 slot，不缓存 offset。
- 对象头 72 B，含每对象一把 Mutex；负载在 GC 区之外单独 malloc；string 字段放在 refs 侧表（16 B），字节区为它留的 8 B 永不使用，所以一个 string 字段占 24 B；每个泛型对象还要冗余地逐实例拷贝 type_args。
- 数组：104 B 头 + 每个数组一个 `Arc<str>` 元素类型名；`new int[n]` 先建 16n B 的临时 Vec，再转 4n B，再拷进 GC 块；泛型 `T[]` 一律 Boxed，所以 `List<int>` 每元素 16 B（压缩存储只需 4 B）。
- JIT 的入口 hoist：只要函数里出现"从不被写的对象寄存器 + 字段"，每次激活都在入口调一次 slot helper，与循环无关。对 getter 是负优化。

**改法**：
1. **一行**：`field_access_of`、`object_region_sizes`、`trace_inline_refs` 统一改用 `composed_object_layout_ref()`。interp 字段访问、每次分配、GC 每次标记都各省 2 次 RMW。
2. FieldIC 载荷改成 `TypeId | offset:16 | kind:8`；`field_access` 挪到 TypeDesc 热区（≤64 B）。
3. JIT：原生单态站点 IC，生成序列为 tag 检查 → load `type_desc` → 比较 → load 字段；hoist 只用于循环内的不变接收者。
4. **内存模型决策**（需要过 spec）：去掉每对象 Mutex，按 CLR/JVM 做法让字宽字段 relaxed 原子读写，lock 语义交给 Monitor 侧表。
5. 实现 `object-abi.md` §3 已设计但未实施的 16 B 统一头；string 字段内联为 8 B；负载并入 GC 槽，一次分配；引用数组元素 8 B；TypeDesc 由 arena 持有，对象存裸指针。

**收益**【推断】：字段访问 15–25 ns → 1–3 ns；小对象 104 B → 32–40 B。

### 4.4 根因四：GC 策略与内存复用

**策略环（解释了"分代比 STW 慢且大"、0B 周期、193 ms 停顿、linux-arm64 回收次数从 897 塌到 88）**【确认代码链路 + 实测佐证】：
1. 增量周期打开时 `begin_alloc_black`，周期内所有新生对象都打上本周期 epoch；
2. minor 用 `keep_major` 把它们一律视为活，并照常升龄、晋升（`generational.rs:534`、`region/generation.rs:273`）；
3. minor 回收不到东西，被判为徒劳，退避 ×4，默认没有上限，最高 ×64；
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
- 分代模式下 `GC.Collect()` 实际只做 minor，却报告成 Full。
- `stats()` 和 `PauseStatsRaw` 每次调用都遍历全堆。
- 报告的停顿不含 TTSP。
- ConcurrentMarkSweep 全面劣于 STW，又与增量 major 功能重叠，应删除。

**改法**：
- **止血**（约 30 行）：只靠 keep_major 存活的条目不升龄；周期内的 minor 不判徒劳；退避加上限；pacer 目标改为一个 nursery 内跑完一个周期。
- **根治**：年轻代归 minor 管，晋升时 promote-black，年轻对象不再 allocate-black；先在 `tests/gc_incremental_model.rs` 模型 D 里穷举验证。
- TLAB 洞复用（Immix 式行/槽复用）；sweep 时就地释放 payload；续用尾巴；池超阈值后 `madvise`。
- 按真实 footprint 记账，加 `committed_bytes`；`GcKind` 分成 Minor / Slice / Major。
- STW 握手：请求时把所有线程的 `safepoint_skip` 置 1；分配点加停车点；JIT 编译、zpkg 加载、cctor 等待期间都 park；请求做成粘性的，永不写 `u64::MAX`；TTSP 单独计量。
- 无锁原子卡表；大数组按元素区间设卡；热路径用编译期选定的具体 Heap 类型，冷 API 留在 `HeapAdmin` trait。

**收益**【推断】：13 场景分代模式回到不高于 STW；RSS ÷2–4；arm64 回收次数不再塌缩。

**最先该做的验证**：linux-arm64 `--jobs 4` 下跑 `Z42_GC_PHASES=1 Z42_GC_TRACE=1`，看 trip 行是否出现 ` x64`、`open cycle` 是否接连出现、trip 行数与 AfterCollect 行数是否一致。macOS 上我已观察到 x64。

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
- `Convert`：`hard_cast_failure` 名字像失败路径，实际是每次都要过的前置判定，之后再分发一遍。int→long 在运行期是恒等变换，却要付两遍分发。
- builtin：`[Native]` 桩 = 一次完整 z42 调用 + `collect_args` 每次 malloc 一个 Vec + 一次共享计数器 `fetch_add`（7 个计数器挤在同一 cache line）。
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
- 惰性加载在 **lazy_loader 写锁内**做文件 I/O 和全量解码，期间所有线程的 lookup miss 都被阻塞。四个只读查询也拿写锁：`lookup.rs:335/388/409/486`。
- 每个 VmCore（包括每个 golden、每个 host module）都从头解码 z42.core，并急切 dlopen compression（约 0.5–0.7 ms，约 7% 指令）。
- **改法**：
  - `PackageImage`：不可变，Arc 共享，进程级缓存；`ZpkgReader` 只解析一遍；函数体按需解码；
  - per-VM 的 `LinkState`（FuncTable / TypeTable / IC 侧表 / init 状态）；
  - 块和异常表改成 u32 下标，删掉 label 与 `block_index`；
  - `FunctionRt` 首执时再构建；`ResolvedTokens` 装箱；
  - 加载移出写锁：读锁下规划 → 锁外解码 → 写锁下发布；
  - compression 改懒加载。

### 4.8 builtin / IO / 反射专项

- `write_atomic` 每次都 `F_FULLFSYNC`，却又不 fsync 父目录（在 Linux 上仍不完整），也不 park。改法：PAL 提供 `replace_atomic(path, bytes, Durability::{None, Ordered, Full})`，编译缓存用 None，它本来就按 build_id/哈希校验。z42c 构建实测可省约 6%。
- fs 全部不 park；stdout 写不 park，管道写满时会挂住 GC；`builtin_file_read/write` 在**全局** file_handles 锁下做系统调用。
- socket/进程的每次调用都在 GC 堆上分配一个判别元组；文件读和 socket 读逐字节 `set_boxed`；进程输出同时生成字符串和逐字节装箱的数组，约 16 倍膨胀。
- 反射没有任何缓存：每次 `typeof` / `GetType()` 都新建一个 `Std.Type`；`GetMethods` 每次重建整张图；dotless 查找在**写锁**下克隆全部类型名；struct 反射用 387 行 Rust 复刻编译器的 StructLayout 算法。
- builtin 错误模型无类型（`anyhow::Error` → 一律 `Std.Exception`）：`Int32.Parse` 抛不出 FormatException；`TryParse` 用异常实现，失败路径极慢。

---

## 5. 框架设计（组件化 / 目录 / 扩展 / 维护）

### 5.1 分层现状与依赖环【确认，grep 计数】

- **metadata 是跨四层的 god-module**，里面同时装着：格式读取（formats/zbc_reader/TIDX）、IR（bytecode，以及属于 interp 优化的 superinstr）、对象模型（types/*，依赖 gc 46 处）、链接（loader/merge/lazy_loader/context）、派发缓存（resolver/ic）。还有向上依赖：vm_context 9–10 处、corelib 6 处、config 1 处。resolver 不是纯函数，它会加载包、排空初始化队列。
- **interp ↔ jit 成环**：interp→jit 12 处（OSR、divert，直接读 `JitModuleCtx` 内部字段）；jit→interp 41 处。`componentized-runtime.md:61` 写的"interp→jit：零"已过时，它 §4.2 计划的 crate 拆分会因这个环直接编译失败。
- **gc ↔ metadata ↔ vm_context 成环**：gc/ 之外引用 `crate::gc::*` 的有 corelib 68、metadata 48、vm_context 25；GcRef 的标记位格式、Value 判别值、"GC 不移动"的假设都烘焙进了 JIT 机器码。
- **VmCore 是上帝对象**：约 40 个字段，包括 7 个 OS 资源表（processes/threads/files/tcp/tls/udp…，按 target cfg 门控）。corelib/native 有 72 处直接访问 `ctx.core.<field>`。
- **进程全局可变状态**（与"VmContext 是唯一规范来源"相悖）：`HOST`、IO sink、`fs_backend::ACTIVE` 与 VFS、`LOADED_COMPRESSION`、repl 注册表、`VM_CORES`、`BUILTIN_INDEX`、`str_meta` TLS。同一进程里的两个 VM 会互相串扰，例如 `__vfs_enable` 会切换所有 VM 的 fs backend。
- **PAL 名不副实**：平台 cfg 在 PAL 外 123 处、PAL 内 13 处（corelib 85、vm_context 16、gc 13）。PAL README 写的"其余模块零 cfg"不成立。

### 5.2 interp / JIT 语义双写（已产生漂移）

| 语义 | 实现份数 | 已知分歧 |
|---|---|---|
| 字段 Get/Set | interp `exec_object.rs:264-474` ↔ JIT `object_field.rs:105-309`；JIT 内部 field_set 又写了 3 遍 | JIT 缺 PinnedView 分支；错误通道一边 `bail!` 一边抛裸 Str；访问 Str 上不存在的字段，两边报错文本不同 |
| ObjNew 整条 symres 判定 | 两份，各约 240 行 | |
| Array / Static / is-as / CallIndirect / ToString / Div-Rem | 两份 | 两份同样存在 MIN/-1 panic；`jit_mk_clos` 在 OOM 时 `unreachable!` 直接 abort，interp 抛可 catch 的 OOM；Str+Str 拼接 JIT 用 `format!`，interp 用融合分配 |
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

### 5.4 死代码与遗留（可直接删，【确认】0 个生产引用）

- 依赖：`bincode`、`thiserror`；IR 上的 serde derive 和 `bytecode_serde.rs`（只服务 JSON 往返测试）。
- `metadata/project.rs`（250 行）；`formats.rs` 里 JSON 时代的 `ZbcFile`/`ZpkgFile`/`SEC_*`/`ZBC_VERSION`，以及与 `namespace_index.rs` 重复且对新格式**错误**的 `read_zbc_namespace`。
- `string_id.rs`（Phase A 落地后没有消费者）；`Function.exec_mode`（解码了但从不读取）。
- LoadFnCached / FRCS 全链路：z42 writer 从不发射；即使复活，slot 模型也会跨模块串槽。
- ConcurrentMarkSweep 模式、finalizer 机制（零注册）、`collect()` 默认 no-op、死旋钮 `Z42_GC_THROTTLE_RATIO`。
- 孤儿测试 `corelib/sync_tests.rs`（650 行，没有 `mod` 引用）；`thread/mod.rs` 空桩；z42-macros 的 `compile_error!` 桩仍从 z42-rs prelude 导出。
- `versions.rs` 461 行里 357 行是 changelog 注释，而且已经漂移（停在 1.45/0.50，常量是 1.46/0.51）。
- AOT：`app.rs` 先做急切 BFS，`vm.rs` 再报错退出，两处报错文本还互相矛盾（LLVM vs cranelift）。
- 注释体量：GC 非测试代码 42% 是注释，大量是历史和实测叙述，文件常因触到行数上限而被机械拆分，而不是按内聚性拆。

### 5.5 文档漂移（摘要）

- `componentized-runtime.md`（interp→jit 零、§7.1 方案在 Rust 下不可行）；
- `jit-design.md` 整篇过时；`object-abi.md` 的对象布局描述过时；`ir.md:532`（`Function.body`）；
- `vm-architecture.md` 关于 PIC torn-read、Builtin "否则 panic"、`vcall_ic` 与下标无关、ObjNew 用 `default_value_for` 等描述；
- `interp-jit-semantics.md:113-115`（不存在的差分测试）；`static-ctor-init.md:64-65`；
- `gc.md` / `gc-tuning.md`（退避写"翻倍"，代码是 ×4；"无 max_bytes 不自动回收"）；
- `diagnostics.md:25`（"热路径零成本"）；`runtime-settings.md:105-115`；`native-ext-loader.md:27,192-194`；`deployment-model.md:41`；
- 代码注释里的过时描述：`interp/mod.rs:1` 写 "tree-walking"，`vm_context/mod.rs` 写 "not Send/Sync"，lazy.rs 写 "24B Value"，等等。

### 5.6 建议的目标分层（自下而上、无环）

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

## 6. z42vm 要不要动态链接 `<sdk>/native/libz42`？

**结论：现在不要，继续静态链接。**

### 6.1 实测：动态化没有性能 / 内存收益

- 同一份 libz42、同一个 C 宿主：hello 动态/静态 13.2/13.2 ms；z42c 编译 526.5/531.2 ms；指令数只多 1.3%。
- 所有跑 z42 代码的进程（launcher、z42c、z42b、xtask 及其子进程）映射的都是**同一个 z42vm 文件**，`__TEXT` 已在进程间共享、dirty 为 0。动态化**不会多出任何页共享**。apphost 本身不含 VM 代码（`__text` 221 KB）。
- 唯一的收益是去掉约 6 MB 重复（gz 后约 2.8 MB）。但这份重复来自 SDK 本不该携带的嵌入件。把嵌入件移出 SDK 后，动态化反而要把 dylib 加回去，**净收益约 0.5 MB**。

### 6.2 Rust 下的真实代价

1. **全局状态分裂**：每个 cdylib 各自带一份 std、tracing-core 和 z42 的 static。exe 里装的 panic hook、tracing subscriber、`init_runtime_config` 对 dylib 内部都不生效，日志和崩溃报告会悄悄失效。所以整套 main 必须搬进 lib（`z42_vm_main(argc, argv)`），clap/regex/tracing-subscriber（约 0.9 MB）就进了每个嵌入者的 libz42，或者得编两种 libz42，那样又不去重了。
2. **mimalloc 丢失**：`#[global_allocator]` 只在 `main.rs`，薄 exe 管不到 dylib 内部的分配。实测不处理的话 z42c 慢 14–20%。要修就得在 lib 里加 feature 开关的 mimalloc，并处理 rlib 消费方。
3. **Linux TLS 变慢**：.so 里的 Rust `thread_local!` 走 general-dynamic 模型（每次 `__tls_get_addr`），PIE exe 是 local-exec，stable Rust 改不了。热路径大量用 TLS（TLAB、SATB、safepoint、帧池），估计回退 1–5%【推断】。
4. **不再是单文件**：`build sdk` 只拷 `bin/z42vm`、CI 只传 z42vm 产物、`Z42_PORTABLE_VM` 只推导 `{vm, libs}`、apphost 只检查 z42vm 是否存在、`build.rs` 的 `find_z42vm`——这些路径都会变成 `Library not loaded` 硬失败。
5. **Windows**：加载器不搜 `native\`，z42.dll 必须放进 `bin\`，或用 delay-load + `AddDllDirectory`。**Linux**：需要 rpath `$ORIGIN/../native` 和 SONAME（现在没有）。
6. **版本耦合**：`bin/` 和 `native/` 可以被分别替换，需要加 build-id 握手。签名或隔离出问题时，z42vm 整个起不来；现状下只是 `Std.Compression` 失败。

### 6.3 `componentized-runtime.md` §7.1（静态 core + `-rdynamic` + dlopen libz42_jit）在 Rust 下不可行

- LTO 后 Rust 符号都被内部化；依赖 crate 里没人引用的 `#[no_mangle]` 也会被剥掉（z42vm 里实际就没有 `z42_register_type`）。`-Zexport-executable-symbols` 是 unstable。
- Rust cdylib（libz42_jit）依赖 z42 crate 时，会**静态嵌入自己的一份 z42**：`runtime_config` 的 OnceLock、TLAB/SATB 的 thread_local、GC 注册表都会各有两份。这是正确性问题，不只是体积。
- 只有 Rust `dylib` crate-type 能真正共享，但 ABI 不稳定，要 `-C prefer-dynamic` 并随包发 libstd。
- JIT 本来就是默认模式，拆出去也省不了常规运行的内存。
- 可行形态只有三种：单体；全部 Rust 代码放进一个 cdylib + 薄 exe；叶子 C-ABI 插件（函数表）。

### 6.4 真正该做的（不改 z42vm 链接方式）

1. **SDK 瘦身**：
   - 停发不可用的 `libz42_compression.a`（desktop 没有静态注册 API，iOS/Android preset 已内置）；
   - SDK 的 `native/` 只保留 compression 动态库，嵌入件留给 runtime 包（可扩展 `z42 workload install` 装 desktop runtime）。
   - 两项合计 SDK 下载体积减少 66–69%。
2. **修好发布的动态库**：
   - mac install name 是 CI 的绝对路径 `/Users/runner/work/…`，改成 `@rpath`；
   - Linux 加 SONAME；Windows 补上漏掉的 `z42.dll.lib`；
   - 按 RID 加"C 宿主链接打包产物"的冒烟测试。
3. **嵌入路径比 z42vm 慢 1.26–1.55 倍**（分配器能解释大约 2/3）：给 staticlib/cdylib 打包构建加 lib 内 mimalloc feature；剩余差距需要 profile。影响面：self-contained、iOS、Android、wasm。
4. **其他打包问题**：
   - glibc 下限声明 2.31，实际需要 2.34（在 ubuntu-latest 上构建）；
   - `build.rs` 把测试 PoC `numz42.o` 链进了发布的 libz42.a；
   - libz42.a 导出 69 个 `jit_*` 和 37 个 `ffi_*` 全局符号，嵌入者容易撞名；
   - compression 改懒加载；
   - clap / tracing-subscriber 拆进 `crates/z42vm`，`env-filter` 换成 `Targets`（省 0.3–0.45 MB）；
   - rustls 加 feature 开关；
   - z42-compression 没做 LTO；
   - macOS 部署目标不一致；
   - Android `opt-level="z"`，打包吞掉错误（nightly Android 包里 compression 确实缺失）；
   - apphost 用 spawn + wait 而不是 exec，子进程被信号杀死时退出码变成 1；
   - PGO 机会：70 KB 的 `exec_function_body`。
5. **Tier-1 native 扩展改用函数表 ABI**：这是动态化唯一实打实的功能性理由——扩展要调全局符号 `z42_register_type`，而 z42vm 不导出它。函数表 ABI 让这个理由消失。

### 6.5 重新考虑动态化的触发条件（满足任一条）

- **T1**：Tier-1 dlopen 落地，并且坚持用全局符号 ABI（建议改函数表，就不会触发）。
- **T2**：产品要求 SDK 本体必须带动态 libz42，并且下载体积成了瓶颈。
- **T3**：出现多个不同的宿主进程常驻、共用同一个 SDK libz42 的形态（IDE/LSP 宿主等）。
- **T4**：JIT 可选成为硬需求。即使这样，也优先出"静态 interp-only"和"静态 full"两种 z42vm，而不是走 §7.1。

**若触发**：
- S0：dylib 元数据修好 + build-id 握手 + lib 内 mimalloc + 嵌入路径性能门禁；
- S1：`main.rs` 整体搬进 `z42_vm_main`，z42vm 成为约 30 行的 stub，rpath 指向 `../native`，Windows 上 dll 放 `bin\`；
- S2：改造所有"只拷 z42vm"的路径，保留 `z42vm-static` 用于自举；
- S3：Linux bench 验证 TLS 回退，超过 2% 就让 Linux 保持静态。

---

## 7. 迭代计划与进度

> 状态：⬜ 待做 · 🟡 进行中 · ✅ 已合并（附 PR 号）· ⏸ 需 User 决策。每项一个 PR，各自过完整 GREEN（纯文档 PR 走 docs 快速通道）。
> 每合并一项就更新本表；表内项全部完成（或剩余项已转入 internals）后删除本文件。

### 阶段 0（批量授权，2026-10-06）

| ID | 类型 | 内容 | 验证要点 | 状态 |
|---|---|---|---|---|
| P0-1 | fix | OSR 遇 ref 回写时拒绝 OSR（`try_osr` 检查 `frame.ref_writebacks`） | golden：附录 A.1，在较小的 `Z42_OSR_THRESHOLD` 下 interp 与 JIT 输出一致 | ✅ (#1089) |
| P0-2 | vm | 整数 MIN/-1 wrapping：`semantics.rs` 提供 `int_div` / `int_rem`，三路共用；`interp-jit-semantics.md` 语义表补行 | golden：附录 A.2（div、rem 两路，interp 与 JIT 一致） | ✅ (#1096) |
| P0-3 | fix | compression 包装器自死锁：`LOADED_COMPRESSION` 改 `OnceLock`，调用时不持锁，顺带去掉全进程串行 | 恢复错误路径测试（附录 A.3：损坏数据抛异常而非挂起） | ⬜ |
| P0-4 | vm | 栈溢出 = 致命错误（见裁决）：<br>• interp 与 JIT 的帧入口检查剩余栈<br>• 内部错误不被转成 z42 异常（builtin 错误转换的两处跳过）<br>• z42vm 打印 z42 栈并以固定非零码退出；host 返回错误码<br>• VM 创建的线程栈可配置，默认 16 MB<br>• `SA_ONSTACK` + `sigaltstack` | 附录 A.4：深递归输出致命报告和 z42 栈、退出码稳定；try/catch 不拦截；host 测试拿到错误码 | ⬜ |
| P0-5 | fix | cctor 屏障按 Function 预计算属主：<br>• 自由函数、无 cctor 的类型永久免检；静态字段站点同理<br>• JIT 不生成屏障，或内联代际检查<br>• 派生名不再送进 `try_lookup_type` | 附录 A.5：hello + 自由函数不再惰性加载额外包；cctor 系列 golden 全绿；z42c 剖面里 `ensure_*_owner_init` 消失 | ⬜ |
| P0-6 | fix | `obj_to_string` 传播 ToString 抛出的异常；`run_returning` 的调用方保留异常类型 | golden：ToString 抛出的异常能被 catch | ⬜ |
| P0-7 | fix | `jit_get_bool` 出错时不再被当成 true（返回状态码 + 值） | 测试：非 Bool 条件在 JIT 下抛异常 | ✅ (#1092) |
| P0-8 | fix | `pending_thrown` 移到 VmContext，并加入 GC 根扫描 | 测试：跨线程不串扰 | ✅ (#1095) |
| P0-9 | fix | 异常表 `try_end` 哨兵：`find_handler` 里的 `?` 改成 `continue`，与 JIT 行为一致 | golden：try 区间覆盖到函数末尾、后面还有 catch 条目 | ⬜ |
| P0-10 | fix | host：<br>• `invoke` 前先 clone 出 `Arc`，再释放全局 `HOST` 锁<br>• `z42_host.h` 配置结构加 `struct_size`，并同步 reference 的嵌入契约 | host 测试：回调里重入不死锁；旧尺寸结构不越界读 | ⬜ |
| P0-11 | fix | 身份哈希加乘法混淆（`corelib/object.rs:243,248`） | 测试：哈希低位分布 | ⬜ |
| P0-12 | perf | 快修批：<br>• `composed_object_layout_ref()` 替换 3 处<br>• `jit_vcall` 的 `from_utf8` 移进 `cfg(debug_assertions)`；字段 / isa helper 改用 `from_utf8_unchecked`<br>• `run_ref_writebacks` 加空判；`try_osr` 加内联门<br>• Convert 让数值快路先行；`collect_args` 改 SmallVec<br>• `needs_auto_collect` 先 load 再 swap<br>• lazy_loader 只读查询改用 `read()`；observer 加 `has_observers` 快门 | bench 不回退；z42c 剖面里对应项下降 | ⬜ |
| P0-13 | perf | `write_atomic` 在 Apple 上用 `F_BARRIERFSYNC`、Linux 上用 `fdatasync`，API 不变 | z42c 构建剖面里 `__fcntl` 下降 | ⬜ |
| P0-14 | perf | compression 改懒加载（首次 ext builtin 未命中时按名加载），去掉启动扫描 | hello 启动指令数下降；压缩测试全绿 | ⬜ |
| P0-15 | perf | 运行时计数器按线程分片，快照时汇总 | `--stats` 输出不变 | ⬜ |
| P0-16 | fix | GC 策略止血：<br>• 只靠 keep_major 存活的不升龄；周期内的 minor 不判徒劳；退避加上限<br>• `GcKind` 分开报告<br>• 按真实 footprint 记账，加 committed 视角<br>• GC 请求粘性化，触发时所有线程进慢路径 | 13_gc_large_heap 上分代不劣于 STW；`Z42_GC_PHASES` 不再出现 x64；GC 模型测试 | ⬜ |
| P0-17 | build | 打包：<br>• 停发不可用的 compression `.a`；SDK `native/` 去掉嵌入件<br>• install_name 改 `@rpath`、加 SONAME、补 `z42.dll.lib`<br>• glibc 下限对齐<br>• `build.rs` 不再把 PoC 链进 libz42 | `package sdk` 内容核对 + C 宿主链接冒烟 | ⬜ |
| P0-18 | refactor | 删死依赖、死代码：bincode、thiserror、IR 上的 serde、`project.rs`、formats.rs 的 JSON 类型、`string_id.rs`、LoadFnCached 链路、孤儿 `sync_tests.rs` | GREEN | ⬜ |
| P0-19 | docs | §5.5 的文档漂移修正（能随对应 PR 顺带的先顺带） | `xtask test docs` | ⬜ |

**不在授权内、需 User 决策**（⏸）：
- 删除 ConcurrentMarkSweep：会去掉 `gc-mode=concurrent` 这个用户可见取值；
- 去掉每对象 Mutex：涉及字段访问的内存模型；
- 可 catch 的栈溢出。
- `int` 等窄整数的算术溢出不回绕到本宽度（P0-2 实施时发现）：`int.MaxValue + 1`、`int.MinValue / -1` 两路都得 `2147483648`，值仍按 i64 存、运算后不截断。要不要按声明宽度回绕、在哪一层截断（编译器插 Convert，还是 VM 按类型运算），属于语言语义决策。

**阶段 0 预期**【推断】：z42c 构建快约 1.3–1.4 倍；hello 启动回到约 10 ms；SDK 下载减少约 2/3；不再有已知的崩溃或挂起路径。

### 阶段 1：身份与调用（约 1 个月，需另行确认）

1. 进程级 FuncTable / TypeTable（FnId / TypeId）；PIC 能缓存 Lazy 目标；ObjNew / CallIndirect / ConstStr 加站点缓存；三套函数注册表合一。
2. 调用协议 v1（瘦 VmFrame、owner-only call_stack、guard 只在入口）；JIT 每线程上下文，**worker 线程跑 JIT**。
3. FieldIC 存 `offset | kind`；静态字段改为稳定地址的 cell + seqlock；JIT 原生字段 IC。
4. GC：TLAB 洞复用 + sweep 时释放 payload；"年轻代归 minor 管"的根治版（先过模型 D）。
5. 抽出 `objops` 引擎无关层（先做 field / array / obj_new），统一错误通道，空引用抛 NullReferenceException。
6. Builtin ABI v2：`#[builtin]` 宏 + manifest、`BuiltinError`、blocking 标志、无 Vec 传参、桩折叠。

**阶段 1 预期**【推断】：z42c 再快 1.3–1.5 倍（字段约 10%、帧约 10%、跨包 VCall，加上并行阶段吃到 JIT）。

### 阶段 2：紧凑执行与对象模型（1–2 个月，需另行确认）

1. 解释器预链接紧凑码（u16 寄存器 / 16 B op / 偏移跳转 / 内嵌 IC 槽 / quickening / 尾调用分发）。
2. 每线程连续 VM 栈，interp 与 JIT 共用；JIT→JIT 直接调用；JIT 代码 arena；批量或后台编译；类型化 SSA 寄存器。
3. 对象模型：去掉每对象 Mutex（需先过 spec 的内存模型决策）、16 B 统一头、string 内联、单次分配、8 B 引用数组元素。
4. `PackageImage` + `LinkState`；metadata 分层；TierApi 注册槽解开 interp↔jit 环；GC 改为编译期具体类型。
5. 拆 crate：core / stdlib-os / stdlib-net（rustls 可选）/ z42vm-cli。

### 阶段 3：长期

精确 stack map + 移动式 nursery；用已 park 的 mutator 线程做并行标记；AOT 复用 JIT 翻译核（前提：改用符号化常量）；按 §6.5 的触发条件再评估动态链接 / 可选 JIT。

---

## 附录 A：复现程序

编译与运行：`z42c --emit-zbc <x>.z42 <x>.zbc`，再执行 `z42vm [--mode jit|interp] <x>.zbc <Namespace>.Main [-- 参数]`（裸 `.zbc` 必须给入口名）。

### A.1 OSR 跳过 ref 回写（P0-1）

期望 `199990000`。现状：`--mode jit` 输出 `0`（`--stats` 显示 `jit_native_from_interp: 1`）。

```z42
namespace Repro.OsrRef2;

using Std.IO;

void Acc(ref long t, long n) {
    long i = 0L;
    while (i < n) {
        t = t + i;
        i = i + 1L;
    }
}

long Wrapper(long n) {
    long s = 0L;
    Acc(ref s, n);
    return s;
}

void Main() {
    long r = Wrapper(20000L);
    Console.WriteLine(r.ToString());
}
```

### A.2 `long.MinValue / -1`（P0-2）

按裁决，期望 `-- div` 输出 `-9223372036854775808`，`-- rem` 输出 `0`。现状：interp 报 `panicked at src/interp/exec_value.rs:108:56: attempt to divide with overflow`；JIT 下进程 abort。

```z42
namespace Repro.DivMin;

using Std;
using Std.IO;

long Div(long a, long b) { return a / b; }
long Rem(long a, long b) { return a % b; }

void Main() {
    string[] args = Environment.GetCommandLineArgs();
    long m = long.MinValue;
    long d = -1L;
    try {
        if (args.Length > 0 && args[0] == "rem") {
            Console.WriteLine(Rem(m, d).ToString());
        } else {
            Console.WriteLine(Div(m, d).ToString());
        }
    } catch (Exception e) {
        Console.WriteLine("caught: " + e.Message);
    }
}
```

### A.3 压缩库错误路径死锁（P0-3）

期望输出 `caught: …`。现状：永久挂起，CPU 0%，栈停在 `wrap_deflate_decompress → last_error_string → parking_lot lock_slow`。

```z42
namespace Repro.ZDead;

using Std;
using Std.IO;
using Std.Compression;

void Main() {
    byte[] junk = new byte[16];
    int i = 0;
    while (i < 16) { junk[i] = (byte)(i * 37 + 11); i = i + 1; }
    try {
        byte[] r = Deflate.Decompress(junk);
        Console.WriteLine("decompressed len=" + r.Length.ToString());
    } catch (Exception e) {
        Console.WriteLine("caught: " + e.Message);
    }
}
```

### A.4 深递归（P0-4）

现状：解释器 `-- 4000` 正常、`-- 5000` 静默崩溃（主线程 8 MB 栈，无任何输出）；JIT `-- 10000` 正常、`-- 20000` 崩溃。按裁决，期望：溢出时打印致命报告和 z42 调用栈，以固定非零码退出。

```z42
namespace Repro.DeepRec2;

using Std;
using Std.IO;

long Down(long n) {
    if (n == 0L) { return 0L; }
    return 1L + Down(n - 1L);
}

void Main() {
    string[] args = Environment.GetCommandLineArgs();
    long d = long.Parse(args[0]);
    Console.WriteLine(Down(d).ToString());
}
```

### A.5 自由函数击穿惰性加载（P0-5）

用 `Z42_LOG=z42::metadata::lazy_loader=debug` 运行。现状：`lazy-loaded zpkg` 出现 16 次（cli、compression、crypto、diagnostics、encoding、io、json、net、numerics、random、regex、text、threading、toml、uri、yaml），启动 10.2 → 24.4 ms，RSS 11 → 23 MB。期望：不出现与程序无关的包。

```z42
namespace Demo;

using Std.IO;

int Greet(int x) { return x + 1; }

void Main() {
    Console.WriteLine(Greet(41).ToString());
}
```

## 附录 B：测量方法

- **微基准**：先 `z42c --emit-zbc src/bench/scenarios/<n>.z42 <n>.zbc --opt-all`，再 `z42vm --mode jit|interp <n>.zbc <Namespace>.Main`，取 5 次中位数。
- **真实负载剖面**：
  1. 在 `src/compiler` 下执行 `z42c build --workspace --release --output-dir <tmp> -q`，事先把 stdlib 的 zpkg 拷进 `<tmp>`；
  2. 运行期间用 macOS 的 `sample <z42vm pid> 18 -file out.sample` 采样；
  3. 按函数名统计包含时间（对每条调用链只计最外层匹配）。
  - `Z42_PORTABLE_VM=<z42vm>` 可以让同一个 z42c 跑在指定的 VM 上，便于新旧对比。
- **GC**：`Z42_GC_TRACE=1`（每次回收一行）、`Z42_GC_PHASES=1`（trip 行带退避倍数 `xN`），`--set gc-mode=stw|concurrent|generational`。
- **惰性加载**：`Z42_LOG=z42::metadata::lazy_loader=debug`，统计 `lazy-loaded zpkg` 行数。
- **启动**：`/usr/bin/time -l`（RSS），用 Python `subprocess` 循环计时取中位数。
