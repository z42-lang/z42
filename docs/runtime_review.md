# Runtime VM 架构评审报告

> 评审日期：2026-07-05
> 范围：`src/runtime/`（约 5.2 万行 Rust），从代码结构、实现框架、数据驱动、易扩展维护四个角度。
> 用途：作为后续 refactor change 的规划底稿；每项推进时按 workflow 开独立 change，完成后在本文档「跟踪表」勾销。

---

## 总体判断

**分层架构是健康的**：interp / jit / gc / metadata / corelib / native / host / pal 模块边界清晰；错误处理全线 `anyhow::Result` 合规（runtime-rust.md 达标）；jit helpers 有集中注册表；观测体系（observer / counters）有雏形。

存在三类**系统性问题**：

1. **六个核心文件严重超出项目自身 500 行硬限制**（code-organization.md 规定「必须拆分，不得推迟」）；
2. **两处直接违反 common-pitfalls.md §1「资源加载顺序必须显式排序」强制规则**（已亲自到源码核实属实）；
3. **解释器与 JIT 的语义实现重复三份、多张关键映射表未数据驱动**——对目标是 byte-identical 自举的项目，这是一致性和可扩展性的最大隐患。

---

## 一、高优先级

### H1. 违反强制规则：资源加载顺序未排序（2 处实锤）

与 2026-05-17 `fix-depindex-nondeterministic-order` 引发跨 OS CI 差异的是同款模式：

| 位置 | 问题 | 修复 |
|------|------|------|
| `src/runtime/src/native/ext.rs:128`（`load_via_dlopen`） | `read_dir` 结果直接迭代加载 native 库；下游 `ExtBuiltinTable::register()`（ext.rs:56-70）是 first-wins 语义——两个 .so 提供同名 builtin 时谁生效取决于文件系统枚举顺序 | collect 到 `Vec<PathBuf>` 后 `sort()` 再迭代 |
| `src/runtime/src/metadata/lazy_loader.rs:242-260`（`candidates_for_namespace` / `remaining_declared`） | 直接迭代 `HashMap`（std HashMap 带随机种子，**同一二进制每次跑顺序都不同**），返回的 zpkg 候选列表顺序非确定 | 返回前 `sort()` |

另有一处弱风险：`loader.rs:611-622` `build_type_registry` 构建的 `HashMap<String, Arc<TypeDesc>>` 若被下游按迭代序消费也是非确定源，排查时顺带确认。

### H2. 六个文件超 500 行硬限制

| 文件 | 行数 | 超限 | 建议拆法 |
|------|------|------|---------|
| `src/runtime/src/gc/arc_heap.rs` | 2299 | 4.6× | 按职责拆：core（结构/mode）/ alloc（分配+OOM）/ collect（mark/sweep/finalizer）/ generational(minor/major/promotion) / roots（pin/frame/scanner）/ observe（barrier observer/sampler/stats），各 <400 行 |
| `src/runtime/src/metadata/zbc_reader.rs` | 1759 | 3.5× | 拆 `format_tables.rs`（65 个 OP_* + SEC_* 常量表）+ `section_readers.rs`（各 `read_*`）+ ~200 行入口协调器（头部检查 + section 分发） |
| `src/runtime/src/jit/translate.rs` | 1692 | 3.4× | 镜像 interp 的组织（exec_instr.rs 19 行分发 + 8 个 exec_* 子模块是正面样板）：拆 translate_value / call / array / object / control，主文件只留 dispatch 路由 |
| `src/runtime/src/vm_context.rs` | 1273 | 2.5× | 结合 M3（ResourceRegistry）一起做 |
| `src/runtime/src/corelib/reflection.rs` | 1224 | 2.4× | 拆 type_builder（类型对象构造）/ member_reflection（fields/methods/properties）/ attribute_reflection（自定义属性），reflection.rs re-export 保持对外接口 |
| `src/runtime/src/metadata/loader.rs` | 1033 | 2.1× | 与 M2（NamespaceIndex 提取）合并做 |

拆分是独立 refactor change、单独 commit（code-organization.md 执行方式）。

### H3. 解释器 / JIT 语义三重实现——自举一致性最大隐患

同一算术/比较/位运算语义现有**三份实现**：

1. interp：`src/runtime/src/interp/exec_value.rs`（基于 Value 类型 match）
2. JIT helper：`src/runtime/src/jit/helpers/arith.rs`（再做一遍类型判断）
3. JIT 内联：`translate.rs` 中 `reg_types` 确认 I64 后**绕过 helper** 的 Cranelift 原语路径（约 translate.rs:614-800）

改一处语义（wrapping 策略、除零行为、新数值类型）需记得同步三处，漏改即 interp 与 JIT 行为分歧——对「编译器全自举 byte-identical」目标是致命的。

**建议**：建 `semantics.rs` 单一真相来源；interp 与 JIT helper 都调它；内联路径只负责寄存器访问，计算规则用 semantics 的 Cranelift 等价物并以注释锚定对应函数。

**配套**：JIT 不支持指令清单在两处手工维护（`jit_unsupported_reason`，translate.rs:132-148 与 match 内 `bail!` 路径，约 :1008-1035），新增 IR 指令无静态防护保证两处同步——用一张 const 表（opcode 名 → 原因）驱动两个检查点。runtime-rust.md 已规定 interp match 不许 `_` 兜底，JIT 侧应有同等纪律。

### H4. Write barrier 在 release 构建下无防护

`src/runtime/src/gc/arc_heap.rs` 的 `write_barrier_field`（约 :1631-1648）靠调用方遵守「仅在 `new.is_heap_ref()` 时调用」的口头契约，唯一防线是 concurrent 模式下的 `debug_assert!`——release 构建被编译掉，违约时静默 no-op。并发标记模式下漏 barrier 是内存安全级 bug。

**修复成本极低**：函数开头无条件 `if !new.is_heap_ref() { return; }`。

---

## 二、中优先级（数据驱动与扩展性债）

### M1. loader / lazy_loader 职责重叠

两模块各自维护「namespace → zpkg 候选」查找（`loader.rs:431-470` 两个 `find_namespace_in_*_dirs` vs `lazy_loader.rs` `candidates_for_namespace`）和两套类型表（`build_type_registry` vs `seed_types_for_lookup`）。

**建议**：提取 `metadata/namespace_index.rs`（`NamespaceIndex`：namespace → candidates 的统一查询 + rebuild），loader 与 lazy_loader 共享；resolver（token 解析 + inline cache）职责独立，保持不动。

### M2. VmContext 呈 god object 趋势

`vm_context.rs:133-320` 的 VmCore 有 30+ 字段，其中 **10 个是完全相同的 `Mutex<HashMap<u64, T>>` + `AtomicU64` 计数器模式**（processes / threads / mutexes / channels / file_handles / tcp_sockets / tcp_listeners / tls_sockets / udp_sockets / rwlocks）。

**建议**：提取 `ResourceRegistry<T>` 泛型（内嵌锁 + 表 + id 生成），新增资源类型从「改 VmContext 三处」变成「加一个字段」。与 H2 的 vm_context.rs 拆分同一个 change 做。

### M3. 应表驱动却硬编码的四张映射表

| 表 | 现状 | 建议 |
|----|------|------|
| **opcode / section 常量** | 65 个 `OP_*` 在 zbc_reader.rs:121-198、SEC_* 在 formats.rs:34-43、Instruction 枚举在 bytecode.rs、writer 在 z42c 侧——version-bumping.md 的 5 处同步清单本质是在为这个分散买单 | 集中 `OpcodeInfo` / `SectionInfo` const 表（可与 H2 的 zbc_reader 拆分同 change）；**建议在下一次 zbc format bump 前完成**，直接受益 |
| **BUILTINS（442 项，corelib/mod.rs:74-442）↔ stdlib .z42 声明** | 无一致性校验，签名不匹配到运行期才报 "unknown builtin" | VM 启动期 eager validation，或 z42c 编 zpkg 时比对 `[Native]` 声明与 BUILTINS 签名 |
| **GC 调参魔数** | 90% near-limit、10% throttle（arc_heap.rs:1398-1400）、75% pressure（:1421-1433）、晋升阈值 2（region.rs:115）直接写在条件里 | 收进 `RuntimeConfig::GcTuning`（near_limit_ratio / pressure_ratio / throttle_ratio / promotion_threshold / soft_ref_threshold），调优实验不改代码；参数文档落 book |
| **native marshal 类型映射** | `dispatch.rs:94-130` `parse_type()` 与 `marshal.rs` 的转换是两处需同步的 match | 合并为一张类型注册表（name / SigType / ffi_type），可自动生成 C header 与文档 |

### M4. zbc 版本特性检查未集中

reader 各 section 里「1.XX 起有此字段」的逻辑靠注释约定（zbc_reader.rs:336-420、:503-600），无集中判定点。strict-pin 政策下当前问题不大，但每次 minor bump 的改动面比必要的大。

**建议**：`metadata/zbc_compat.rs` 提供 `ZbcVersion::verify()` + `has_feature(Feature)`，section reader 接收版本参数按 feature 分支。

### M5. 解释器内部样板收敛

| 模式 | 位置 | 建议 |
|------|------|------|
| OOM 异常构造重复 3 次（禁严格模式→构造 Std.OutOfMemoryException→恢复→返回） | exec_call.rs:192-202、:248-258；exec_object.rs:70-81 | 提取 `make_oom_exception(ctx, module, msg)` 辅助 |
| resolved 缓存加载模式重复 11 处（`resolved.filter(...).and_then(...)`） | exec_instr.rs:135-137、:157-159、:202-204 等 | 宏或泛型辅助 `load_cached_token` |
| 「主模块查不到→查 lazy loader」函数解析重复 | exec_call.rs:94-105、:209-216；exec_vcall.rs、dispatch.rs | 提取 `resolve_function(ctx, module, name)` 统一入口（未来加缓存也在此） |
| TypeTag 常数（T_BOOL..T_ARRAY）孤立镜像编译器侧 | exec_value.rs:244-262 | 加编译期一致性测试，或移到 `metadata::tokens` 单一定义处 re-export |

同时收敛新增指令时的遗漏面。

### M6. Safepoint 与 GC 协议不清晰

`maybe_auto_collect`（arc_heap.rs:1391-1414）的「设 flag 走 defer / inline collect」决策隐含在「flag 是否 wire-up」里；`external_needs_collect` 注释为 "optional wire-up"，谁检查、何时检查无文档。

**建议**：在 `MagrGC` trait（heap.rs）文档化 Safepoint 协议（注册 / defer / fallback 三态），为多线程执行铺路。属文档+小改，可与 H4 同 change。

---

## 三、低优先级（记录在案，等触发条件）

| # | 事项 | 触发条件 |
|---|------|---------|
| L1 | corelib builtin 参数解包样板宏化（`#[builtin(...)]` 或声明宏） | builtin 数量再上一个量级时 |
| L2 | PAL 层悬空：corelib/fs 直接调 `std::fs` 未走 PAL——要么 fs 收进 PAL，要么明确 PAL 只管 signal/system | 下次跨平台需求（新 RID / wasm fs）时裁决 |
| L3 | `src/runtime/src/corelib/` 缺目录 README（code-organization.md 要求），新增 builtin 无 checklist | 随任一 corelib change 顺带补 |
| L4 | 观测体系 Phase 2 占位未跟踪：counters.rs:53-62（jit_* / native_calls / exceptions_*）与 observer.rs:56-87 各 4-5 个占位 | 在 roadmap Deferred 索引挂号，JIT 推进时 wire-up |
| L5 | 并发标记 mark_queue（arc_heap.rs:242）无背压上限，mutator 写速 > mark 速时队列无界增长 | concurrent GC 生产化前置项 |
| L6 | 卡表粒度 = CHUNK_SIZE（region.rs:47，256）未与年轻代协调，可能粗粒度误标 | 分代 GC 性能调优期（Phase 2+），先落设计文档 |
| L7 | `jit/frame.rs:142-154` FnEntry 裸指针 `*const u8` 无法承载 AOT 的代码定位；建议演进为 `CodeLocation` enum（Jit ptr / Aot 引用） | AOT 基础设施动工时 |
| L8 | JIT helper 新增需改 5 处（定义 / HelperIds / register_symbols / declare_imports / imp!），无 checklist 文档；helper 是否需 `check!`（异常检查）无标注 | 短期在 helpers/mod.rs 头部补 checklist 注释；长期 proc-macro 生成 |
| L9 | `exec_value.rs`（371 行）已过 300 行软限制 | 下次动它时顺势拆（arith / logic / convert） |
| L10 | config.rs 三处同步（struct 字段 / from_getter / KNOWN_KNOBS 表）可宏化 `define_knobs!` | 锦上添花，随任一 config 改动顺带评估 |
| L11 | GC observer 与 stats 混在 RcHeapInner（arc_heap.rs:145-182），可拆 ObserverRegistry / StatsCollector | 随 H2 的 arc_heap 拆分顺带评估 |
| L12 | host / corelib 的 stdout sink 双层 thread-local 耦合（io.rs:20-33、:74-80），embedding 场景复杂化时难理解 | REPL / IDE 集成需求出现时抽象 IoRouter |

**已确认合规项**（无需行动）：错误处理全线 `anyhow::Result`、host FFI 边界 `Z42HostStatus` 不混用内部错误；jit helpers 集中注册表设计正确；interp 的 exec_* 拆分是全 runtime 的组织样板。

---

## 四、建议落地顺序与跟踪表

按 workflow：每项开独立 refactor change（拆分不与功能混提交），受 parallel-development.md 子系统互斥锁约束（下表已标子系统）。

| 顺序 | change（建议名） | 覆盖 | 子系统 | 状态 |
|------|-----------------|------|--------|------|
| 1 | `fix-runtime-load-order-determinism` | H1（+ H4 write barrier 防护，同为小而硬） | runtime | ☐ |
| 2 | `refactor-arc-heap-modularization` | H2(arc_heap) + L11 顺带评估 | runtime | ☐ |
| 3 | `refactor-zbc-reader-split` | H2(zbc_reader) + M3(opcode 表) + M4(zbc_compat) —— **建议在下一次 format bump 前完成** | runtime | ☐ |
| 4 | `refactor-jit-translate-split` | H2(translate) + H3(semantics 统一 + unsupported 表) —— 反正要大动 JIT 文件，一次收敛三重实现 | runtime | ☐ |
| 5 | `refactor-vm-context-resource-registry` | H2(vm_context) + M2 | runtime | ☐ |
| 6 | `refactor-metadata-namespace-index` | M1 + H2(loader) | runtime | ☐ |
| 7 | `refactor-reflection-split` | H2(reflection) + L3(README) | runtime | ☐ |
| 8 | `refactor-interp-boilerplate` | M5 | runtime | ☐ |
| 9 | `add-gc-tuning-config` | M3(GC 调参) + M6(safepoint 协议文档) | runtime | ☐ |
| 10 | `add-builtin-signature-validation` | M3(BUILTINS 校验) + M3(marshal 表) | runtime + stdlib | ☐ |

L 系列不排期，按各自触发条件挂靠对应 change 或 roadmap Deferred 索引。

> 注意：以上全部为 runtime 子系统——按互斥锁一次只能一个 in-flight，顺序执行即可；#10 跨 runtime+stdlib 需两锁全空闲。
