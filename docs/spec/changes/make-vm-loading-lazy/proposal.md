# Proposal: VM 全惰性加载 —— zpkg / 函数 / 类型都"用到才加载/物化"

## Why

z42vm 每次启动一个程序,都**eager 地把整套 stdlib 闭包解析进内存**:所有函数字节码
(`read_func` 全解)、所有类型(`build_type_registry` 全建),哪怕这个程序只碰几十个。JIT 更是
`is_eager` BFS 合并整个 dep 闭包(main.rs)。惰性逐函数 JIT 已让**编译**按需,但**加载**仍全量——
一个小 golden 用例每进程仍要 parse 整套 z42.core。

后果:每个 z42vm 启动的固定成本 ∝ 整套 stdlib,而非"程序实际触及"。CI e2e golden 每例 fork 一进程、
每进程全量加载 stdlib(interp ~28s/例、jit ~32s/例)——两条关键路径(test-host / test-vm-jit)都卡这里。
但收益远不止 CI:**每个 CLI / 脚本 / REPL 启动**都在为没用到的 stdlib 付费。

**惰性 JIT 已经拆掉"必须预编译全部"这个前提——那么"必须预加载全部"也不再必要。** 终局是:
**zpkg 按需打开、函数按需 parse、类型按需物化、代码按需编译**——每一层"只为用到的付费"。这与惰性 JIT
一脉相承,是战略性运行时改进(尤其惠及 REPL capstone 的快启动 + 增量加载)。

## What Changes

把加载/物化的粒度从"整套闭包 / 整个 zpkg"降到"单个函数 / 单个类型 / 首次使用":

- **zpkg 按需**:打开一个 zpkg 时只读它的 section 目录 + **函数/类型索引**(不解 body);删 JIT 的
  eager BFS 合并,jit 与 interp 统一走单一惰性收口 `try_lookup_function` / `try_lookup_type`。
- **函数按需 parse**:`Function` 的字节码在**首次被调用**时才从索引定位、parse 这一个(现在 `read_func`
  一次全解;改为存原始 FUNC 字节 + 首次用到经 `OnceLock` 建 `Function`)。
- **类型按需物化**:`TypeDesc`/vtable/字段布局在**首次使用**该类型时才建一个(现 `build_type_registry`
  一次全建;改为按需建 + 逐个继承修复)。
- **格式**:zpkg/zbc 加**函数偏移索引 `FIDX`**(name → FUNC 段 offset/len + SIGS 序号)+**类型偏移
  索引 `TIDX`**,使单个函数/类型可随机寻址(现最细只到 section / MODS 内 module)。→ zbc·zpkg
  格式 minor bump。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | 新增 `FIDX`/`TIDX` section 定义 + version minor++ |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 写 FUNC/TYPE 时记录每函数/类型 offset,emit `FIDX`/`TIDX` |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | 读 `FIDX`/`TIDX`；`read_func`/`read_type` 拆出"按 offset 解单个"；strict-pin minor 同步 |
| `src/runtime/src/metadata/loader.rs` | MODIFY | 加载改"存原始 section 字节 + 索引",不 parse-all；`build_type_registry` 拆出按需建单类型 + 继承修复 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `Module` 存原始 FUNC/TYPE 字节 + 索引 + `OnceLock<Function>` / `OnceLock<TypeDesc>` 惰性槽 |
| `src/runtime/src/metadata/lazy_loader.rs` | MODIFY | `resolve_function`/`resolve_type` 由"加载整个 zpkg"改为"按索引 parse 单个函数/类型" |
| `src/runtime/src/vm_context.rs` | MODIFY | `try_lookup_function`/`try_lookup_type` 收口不变;支撑惰性槽访问 |
| `src/runtime/src/main.rs` | MODIFY | 删 `is_eager` BFS 合并;jit 走 lazy loader |
| `src/runtime/src/jit/frame.rs` | MODIFY | `resolve_fn_by_id` 统一:未命中 `module.functions` 时经 lazy loader 取函数并**编译**(不退 interp) |
| `src/runtime/src/jit/mod.rs` | MODIFY | static-init 发现改扫命名空间/force-load（不再只扫 merged `module.functions`）；ConstStr 溢出走 `try_lookup_string` |
| `docs/design/runtime/zbc.md` / `zpkg.md` | MODIFY | FIDX/TIDX 段格式 + minor changelog |
| `docs/book/src/runtime/lazy-loading.md` | NEW | 全惰性加载机制页（层级/索引/按需 parse/类型物化/jit 统一，配伪代码+mermaid） |
| `src/tests/zbc-format/*` / `zpkg-format/*` | MODIFY | 格式 bump fixture 重生（version-bumping.md checklist） |
| `src/runtime/src/metadata/lazy_load_tests.rs` | NEW | 单测:只 parse 碰到的函数/类型、跨 zpkg 惰性解析、jit 编译 lazy-loaded 函数 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 runtime（+ ir/compiler 格式）锁 |

**只读引用**：`interp/exec_call.rs`/`exec_vcall.rs`/`exec_object.rs`（调用/类型解析收口）、
`jit/lazy.rs`（惰性编译现状）。

## Out of Scope

- **批处理 runner**（前一版方案）——全惰性把每进程成本降到"只碰到的"后,fork-per-case 本身就快,
  批处理非必需;若全惰性后仍不够再议。
- **AOT**（延后不变;AOT 需要 eager 全编,格式索引对它无害但不在本 change 消费）。
- **z42b（stdlib [Test] 运行器）的同类优化**——本 change 聚焦 VM 加载层;z42b 自然受益于更快的 VM 启动。
- **持久 daemon / 跨进程共享编译产物**——明确不做（机器码进程私有）。

## Open Questions

- [ ] 阶段划分:阶段 1（统一 jit/interp 加载路径,zpkg 粒度,无格式改动）先落地验证不回归,阶段 2
  （FIDX/TIDX 格式 + 函数/类型按需 parse）随后?→ design Decision 1
- [ ] 格式 bump 的分阶段引入纪律（bootstrap-seed.md：support 先行、晚一 nightly 再 use）如何套用于
  FIDX/TIDX?→ design Decision（自举安全）
- [ ] 类型按需物化与 `build_type_registry` 的 topo-sort/继承闭包（现假设全类集在场）如何拆成"按需建单
  类型 + 增量继承修复"?→ design Decision（阶段 2 核心技术风险）
