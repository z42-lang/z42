# Design: VM 全惰性加载

## Architecture — 惰性层级

```
zpkg   ── 首次需要其中符号 → 只读 section 目录 + FIDX/TIDX 索引(不解 body)
 ├ 函数 ── 首次被调用 → FIDX 定位 offset → parse 这一个 Function(OnceLock 缓存)
 ├ 类型 ── 首次使用 → TIDX 定位 → 建这一个 TypeDesc + vtable(OnceLock + 增量继承修复)
 └ 编译 ── (jit) 首次调用 → cranelift 编译  ✅ 已做(jit/lazy.rs)

单一收口(已存在,不移动调用点):
  interp: exec_call → module.func_index 命中 / 否则 ctx.try_lookup_function
  jit:    jit_call → resolve_fn_by_id / 否则 ← 统一到 try_lookup_function 并编译
  类型:   ctx.try_lookup_type
```

**核心洞察(agent 证实)**:`try_lookup_function` / `try_lookup_type` 是**唯一的"未找到→加载"收口**——
惰性化只需**改这两个内部做什么**(从"加载整个 zpkg"→"按索引 parse 单个"),调用点全不动。

## Decisions

### Decision 1: 两阶段(阶段 1 无格式改动先落地)
- **阶段 1 — 统一加载路径(无格式 bump)**:删 `main.rs` 的 `is_eager` BFS 合并;jit 与 interp 都走
  `try_lookup_function`(zpkg 粒度惰性)。关键补丁:`resolve_fn_by_id`(frame.rs)未命中 `module.functions`
  时,**从 lazy loader 的 `function_table` 取函数并 `compile_one` 编译**(而非退 `cross_zpkg_via_interp`
  跑解释器)——否则 stdlib 在 jit 下整体退化 interp(agent 点明的隐藏依赖)。+ static-init 改扫命名空间、
  ConstStr 溢出走 `try_lookup_string`。**收益有限**(z42.core 仍整个 zpkg 加载),但**统一了架构、
  删了残留 eager 合并**,是阶段 2 的骨架。可独立验证不回归。
- **阶段 2 — 函数/类型粒度(格式 bump)**:加 FIDX/TIDX 索引,加载器"存原始字节 + 首次用到才 parse"。
  **真收益**(z42.core 内部也只 parse 碰到的)。

### Decision 2: 格式索引 FIDX / TIDX（阶段 2）
zbc·zpkg 现最细可寻址 = section(MODS 内到 module)。加两段:
- **FIDX**:`func_name → (FUNC 段内 offset, len, SIGS 序号)`,让单个函数字节可随机取。
- **TIDX**:`type_name → (TYPE 段内 offset, len)`,让单个类型描述可随机取。
- writer(ZbcWriter.z42)写 FUNC/TYPE 时顺带记 offset 表;reader 拆 `read_func`/`read_type` 为
  `read_one_func(offset)` / `read_one_type(offset)`。zbc·zpkg minor bump(strict-pin 两端同步,见
  version-bumping.md)。

### Decision 3: Module 惰性槽（阶段 2）
`Module` 由"全 parse 的 `Vec<Function>` / 全建的 type_registry"改为:
- 保留**原始 FUNC/TYPE section 字节** + FIDX/TIDX 索引。
- `functions: Vec<OnceLock<Function>>`(按 FIDX 序预分配,首次 `resolve_fn_by_id`/`try_lookup_function`
  时 parse 填槽)。
- `type_registry: 按需 build`——`OnceLock<Arc<TypeDesc>>` 槽,首次 `try_lookup_type` 建。
- **沿用惰性 JIT 已验证的 `OnceLock` 模式**(fn_entries_by_id 同款:预分配、按需填、线程安全)。

### Decision 4: 类型按需物化 vs 继承闭包（阶段 2 核心风险）
`build_type_registry` 现 topo-sort 全类 + 算 flattened 字段布局/vtable,**假设全类集在场**。按需建单
类型需要:建 T 时**递归先建其 base 链**(base 可能在别的 zpkg → 经 `try_lookup_type` 触发),再算 T 的
布局。`try_fixup_inheritance`(loader.rs)已是"惰性加载后修继承"的雏形,复用/推广为"按需建单类型的
base-first 物化"。**风险标注**:若某些布局计算确需全类集(如密封优化/whole-program vtable),按需化过
复杂 → 阶段 2 可先只做**函数**按需(类型仍 zpkg 粒度惰性),类型按需拆后续 change。

### Decision 5: 自举安全（格式 bump 分阶段引入）
FIDX/TIDX 是格式变更 → 按 bootstrap-seed.md「support 先行、晚一 nightly 再 use」:
- **先一个 nightly**:reader(z42vm)支持读 FIDX/TIDX(存在则用,不存在则回落全 parse);writer 暂不 emit。
- **晚一个 nightly**:writer(z42c)开始 emit FIDX/TIDX,产物用新格式。
- 保证上一版 z42c/z42vm 永远能编/读当前源产物。`xtask test bootstrap` 守边界。

### Decision 6: 正确性门禁 —— 行为逐字节不变
惰性只改"何时加载/物化",**不改任何执行结果**。全部现有 golden(interp + jit)、stdlib [Test]、
cross-zpkg、自举不动点(gen1==gen2 byte-identical)必须全绿且输出不变。这是最强回归保证。

## Implementation Notes

- **阶段 1 关键改动**:`resolve_fn_by_id`(frame.rs)miss 分支:先 `module.func_index` →(新)
  `vm_ctx.try_lookup_function(name)` 取 `Arc<Function>` → 若可翻译,**在共享 JITModule 里 compile_one
  这个 lazy 函数**、缓存 FnEntry → 走 native;仅真不可翻译才 `cross_zpkg_via_interp`。需把 lazy-loader
  函数纳入 JIT 的可编译源(现 `compile_one` 只从 `self.module.functions[idx]` 取,需扩展为可编译一个
  外部 `&Function`)。
- **阶段 1 static-init**:jit `run()` 改为扫已加载 + 惰性命名空间的 `.__static_init__`(镜像 interp
  `init_static_fields`),或对入口依赖的 ns force-load 其 static-init。
- **阶段 2 loader**:`load_artifact` 不再 `read_func`/`build_type_registry` 全量;改建"索引 + 原始字节 +
  OnceLock 槽"。`read_zbc`/`read_mods_section` 拆分。
- **阶段 2 兼容回落**:reader 见无 FIDX/TIDX 的旧格式 → 回落当前全 parse 路径(格式 bump 期共存;
  strict-pin 最终收敛后可删回落,按 philosophy 不长期留兼容——回落仅存在于 bump 分阶段窗口)。

## Testing Strategy

- **回归(最强)**:`xtask test`(完整 GREEN gate:e2e interp+jit / cross-zpkg / stdlib / compiler
  自举不动点 / vscode)全绿且输出逐字节不变。
- **单元(`lazy_load_tests.rs`)**:
  - 阶段 1:jit 删 eager 合并后,一个只调 z42.core 的程序不加载其它 stdlib zpkg;lazy-loaded stdlib
    函数走 **native**(非 interp fallback);static-init 正确执行。
  - 阶段 2:一个碰 K 个函数/类型的程序,只 parse 这 K 个(观测 parse 计数 / OnceLock 填充数);
    未碰的函数/类型永不 parse/物化。跨 zpkg 继承按需 base-first 物化正确。
  - 格式:FIDX/TIDX 往返 + 无索引旧格式回落。
- **格式 bump**:version-bumping.md checklist(两端 minor、fixture 重生、changelog、自举)。
- **性能佐证(非门禁)**:`Z42_LOAD_PROFILE`(新增,可选)打印"parse 了 N 个函数 / M 个类型";CI
  test-host interp e2e ~26m→回落、test-vm-jit shard ~29m→回落(阶段 2 后)。以 CI 为权威。
