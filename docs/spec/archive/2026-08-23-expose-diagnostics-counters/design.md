# Design: 运行时计数暴露给 z42（Std.Diagnostics.RuntimeStats）

> 本 change 是「脚本性能分析」程序 P1c。统一 DRAFT（P1b/P1c/P2）已 User 批准；本文件是 P1c 分支。

## Architecture

```
z42 script
  └─ Std.Diagnostics.RuntimeStats.Counters()        (z42.diagnostics/src/RuntimeStats.z42)
        │  [Native("__diag_counters")] extern
        ▼
  corelib::diagnostics::builtin_diag_counters   (src/runtime/src/corelib/diagnostics.rs)
        │  ctx.counters().snapshot()  +  ctx.heap().stats()
        │  → alloc_named(STD_RUNTIME_COUNTERS, [11 fields])
        ▼
  Value(Object: Std.Diagnostics.RuntimeCounters) (z42.diagnostics/src/RuntimeCounters.z42)
```

同源保证：`builtin_diag_counters` 投影的 counter 来自 `RuntimeCounters::snapshot`、堆派生字段来自
`HeapStats`——与 `app.rs` 组装 `ProfileSnapshot` 的**同两个来源**，故脚本内读值与 `--print-stats-on-exit`
JSON 语义一致（除采样时刻不同带来的自然 skew）。

## Decisions

### Decision D3: z42 端全景字段落点 —— 新 RuntimeCounters，HeapStats 不动
**问题：** P1a 已把 `minor/major/reclaimed` surface 到 profile JSON，但 z42 侧 `Std.HeapStats`（gc.rs:418
位置投影）仍是 7 字段。脚本要拿全景（counter + 堆派生）该放哪？
**选项：**
- A — 扩 `Std.HeapStats` 补 3 分代字段：要同步改 3 处（HeapStats.z42 property + `heap_stats_type_desc`
  names + `builtin_gc_stats` 投影数组），且 HeapStats 语义是「堆」快照、塞 counter 不合适。
- B — 新 `Std.Diagnostics.RuntimeCounters`（全 11 字段 = 7 counter + allocations + 3 分代），
  `Std.HeapStats` 保持 7 字段不动。
**决定：** 选 **B**。全景（运行时 counter + 堆派生）归 `Std.Diagnostics.RuntimeStats`——语义正确（Diagnostics
是「运行时可观测面」），且不动 GC.GetStats 的精简契约、不碰 gc.rs 三处同步。HeapStats 仍是纯堆快照。

### Decision D4: allocations CI gate —— 先 informational
**问题：** `allocations` 确定性、可做硬 gate，但绝对阈值 / 相对% 未定。
**选项：** A — 立即硬 gate（绝对 golden）；B — 先 informational 观察几轮再定阈值。
**决定：** 选 **B**（记忆已定）。CI 打印 `allocations` vs baseline，**不 fail**；观察 3–5 轮跨 OS
（linux/mac）× GC-mode（stw/generational/concurrent）稳定性后，后续 change 再转硬 gate。理由：分配次数
虽确定性，但不同 GC-mode 触发的内部临时对象可能不同；先摸清基线波动面再上阈值，避免误报挡 PR。

### Decision D5: 投影方式 —— 按名投影（alloc_named），非位置投影
**问题：** `builtin_gc_stats`（gc.rs:418）用**位置**数组投影（脆，加字段要同步改数组顺序）；
`corelib/diagnostics.rs:30 alloc_named` 是**按 `field_index` 名**投影。
**决定：** 用 `alloc_named`（按名）。新对象字段多（11 个）、易演进，按名投影抗字段重排、少一处顺序耦合。
与现有 `Std.Diagnostics.Retainer`/`RootRef` 的构造范式一致。

### Decision D6: 不动格式
新 builtin 仅 append `BUILTINS`（运行时表，非二进制格式）；`[Native]` 是运行时解析字符串。
**无 zbc/zpkg 格式 bump** → 不触发自举两代墙。本地可完整 GREEN（fresh VM 有 `__diag_counters`；
stdlib 由 fresh z42c 编译；`xtask test stdlib` 在 fresh VM 下跑 → 全链闭合）。

### Decision D7: 累加器类名 `RuntimeStats`，**不叫 `Runtime`**（避 z42.core prelude 冲突）
**问题：** 最初把静态访问器类命名为 `Std.Diagnostics.Runtime`（`Runtime.Counters()`）。但 z42.core
（prelude，隐式导入所有模块）已有 `namespace Std.Runtime; public static class Runtime`。同简名 `Runtime`
在 **inline 链式调用**（`Runtime.Counters().Allocations`）语境下被解析到错误的 `Std.Runtime.Runtime`
→ 返回 Null → getter 报 `VCall: expected object, got Null`。诡异处：`RuntimeCounters c = Runtime.Counters();`
（先存 typed local）绑定正确、可用——只有 inline 链式误绑。
**根因：** 命名冲突（自引入），**非编译器 bug**：实测把 Rust builtin 分配方式改成与 `GC.GetStats` 逐字节
相同（手建 UNRESOLVED TypeDesc）仍 Null；改名后 inline 在 interp + jit 立即恢复（a0=28,a1=230,cmp=true）。
**决定：** 静态访问器类命名 **`RuntimeStats`**（`Std.Diagnostics.RuntimeStats.Counters()`），与 `Std.Runtime`
无简名碰撞。返回类型仍 `RuntimeCounters`。**教训：新 stdlib 简名必须先 grep 全 `src/libraries/` 确认不与
prelude（尤 z42.core）简名冲突**——冲突不一定报编译错，可能表现为 inline 语境静默误绑 Null。

## Implementation Notes

- **Rust builtin**（`corelib/diagnostics.rs`）：
  ```rust
  pub fn builtin_diag_counters(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
      let c = ctx.counters().snapshot();
      let h = ctx.heap().stats();
      alloc_named(ctx, STD_RUNTIME_COUNTERS, &[
          ("__prop_BuiltinCalls",        Value::I64(c.builtin_calls as i64)),
          ("__prop_NativeCalls",         Value::I64(c.native_calls as i64)),
          ("__prop_JitMethodsCompiled",  Value::I64(c.jit_methods_compiled as i64)),
          ("__prop_JitCompileUsTotal",   Value::I64(c.jit_compile_us_total as i64)),
          ("__prop_JitNativeFromInterp", Value::I64(c.jit_native_from_interp as i64)),
          ("__prop_ExceptionsThrown",    Value::I64(c.exceptions_thrown as i64)),
          ("__prop_ExceptionsCaught",    Value::I64(c.exceptions_caught as i64)),
          ("__prop_Allocations",         Value::I64(h.allocations as i64)),
          ("__prop_MinorCollections",    Value::I64(h.minor_collections as i64)),
          ("__prop_MajorCollections",    Value::I64(h.major_collections as i64)),
          ("__prop_ReclaimedBytes",      Value::I64(h.reclaimed_bytes as i64)),
      ])
  }
  ```
  - `STD_RUNTIME_COUNTERS` = 一个**字符串常量** `const STD_RUNTIME_COUNTERS: &str =
    "Std.Diagnostics.RuntimeCounters";`（与现有 `STD_RETAINER`/`STD_ROOTREF` 同型——它们是 FQN 字符串，
    非数字 well-known ID）；`alloc_named` 靠 `ctx.try_lookup_type(type_name)` 查**已加载的** z42.diagnostics
    类型，无需注册数字 ID。
  - `__prop_` 前缀：z42 auto-property `public long X { get; }` desugar 成私有 backing field `__prop_X`
    + `get_X()`；投影须写 backing field 名（与 HeapStats.z42 范式一致）。
  - `alloc_named` 按 `field_index` 名投影，字段顺序无所谓，但**名字必须与 z42 class 的 backing field 精确对应**。
- **z42 类型**（`RuntimeCounters.z42`）：11 个 `public long X { get; }` auto-property，不暴露 ctor
  （只由 `__diag_counters` emit），顶注释说明字段来源（counter vs 堆派生）。**单文件 <200 行类硬限**（11 属性 OK）。
- **z42 API**（`RuntimeStats.z42`）：`public static class RuntimeStats` + `[Native("__diag_counters")] public static
  extern RuntimeCounters Counters();`。命名 `Counters()`（PascalCase 名词，快照访问器，符合惯例）。
- **CI gate**：`bench-pr.yml` 加一步——跑固定脚本（分配确定的小 workload）取 `--print-stats-on-exit
  --stats-format json` 的 `allocations`，与存在 bench-baselines 分支的 baseline 比，`echo` 差异，
  **不设 exit≠0**。baseline 采集脚本复用现有 bench 场景（如 `07_string_heavy` / `08_dict_heavy`）或新增
  一个分配确定的 `bench/scenarios/` gate 场景（实施时判定现有场景 allocations 是否确定 + 有代表性）。

## Testing Strategy

- **单元测试**（`diagnostics_tests.rs`）：构造 VmContext，跑几次 builtin / 分配对象，调
  `builtin_diag_counters`，断言返回对象字段数 == 11 且 `Allocations` / `BuiltinCalls` 反映实际活动。
- **Golden test**（`z42.diagnostics/tests/runtime-counters/`）：z42 脚本调 `RuntimeStats.Counters()`，打印
  **确定性字段**（如「分配 N 个对象后 Allocations 增量 == N」的布尔断言，规避非确定绝对值），
  `expected_output.txt` 用数值/布尔（不嵌入易漂移的绝对计数）。
- **VM 验证**：`xtask test`（完整 GREEN gate）；`xtask test stdlib`（z42.diagnostics 的 [Test]）。
- **一致性对账**：单测断言 `RuntimeStats.Counters().Allocations` 与同 ctx `heap().stats().allocations` 相等。

## Deferred

- **确定性硬 gate**：informational 观察后转硬阈值（绝对/相对%）——后续独立 change。
- **事件订阅 / span**（diagnostics.md §7 `subscribe`/`span`）：不在脚本性能分析程序范围。
