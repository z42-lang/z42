# Design: 通用堆保留诊断（Heap Retention Diagnostics）

> 上位设计：[load-context.md](../../../design/runtime/load-context.md) §5「保留根诊断」。
> 本 change 把它**泛化**为通用 `Std.Diagnostics.Heap`（任意对象），落 L1+L2；L3 完整链条延后。

## Architecture

```
  Std.Diagnostics.Heap.DirectReferrers(target)  ──[__heap_direct_referrers]──┐
  Std.Diagnostics.Heap.RetainingRoots(target)   ──[__heap_retaining_roots]───┤
                                                                             ▼
  ArcMagrGC::retention_query(target_ptr, mode):
    1. force_collect()                        // 存活即可达，无浮动垃圾
    2. 建反向图（一次堆扫描）:
       region_object.iterate_alive + region_array.iterate_alive:
         对每个存活对象 O，trace_children(O) → 每条堆-ref child:
             rev[child_ptr].push(Parent{ ptr:O, type, kind })
       分类根 scanner（categorized）→ 每个根 Value R：
             roots_of[R.ptr].push(RootInfo{ kind })
    3a. L1 DirectReferrers：rev[target_ptr] + 直接指向 target 的根 → RetainerInfo[]
    3b. L2 RetainingRoots：从 target 反向 BFS（沿 rev）收集祖先集；
        任一祖先/target 被根直接指向 → 收集该根 kind（去重）→ RootInfo[]
    → 结果回 builtin → 建 z42 Retainer[] / RootRef[]
```

## Decisions

### D1: 按需堆扫描（第 2 层路线），不建常驻保留边注册表
**问题：** load-context.md §5 有「第 1 层框架边（常驻可枚举）」+「第 2 层堆路径（按需 GC walk）」两条。
**决定：** 本 change 只走**第 2 层**——按需一次堆扫描建反向图。理由：通用对象诊断没有"框架注册槽"
可枚举（那是 context 专属边）；通用查询天然是堆反向可达。常驻注册表留给 context 专项优化（后续）。

### D2: 触发 full GC 换准确（User 确认）
`retention_query` 先 `force_collect()`。之后 `iterate_alive` 只见**可达**对象（垃圾已 sweep）→
反向图无浮动垃圾误报。诊断非热路径，一次额外 GC 可接受。

### D3: 反向图一次堆扫描构建，L1/L2 复用
`trace_children` 是正向；反向图 = 遍历所有存活对象取每条正向边的反。`region_object` +
`region_array` 都支持 `iterate_alive`。对象身份用 data ptr（`GcRef::as_ptr`）。L1 = `rev[target]`
一跳；L2 = 沿 `rev` 反向 BFS 到根。同一次扫描服务两者（builtin 按 mode 取需要的部分）。

### D4: 分类根 scanner（新增，类别级）—— L2 报根的前提
**问题（正确性校正）：** 现有 `external_root_scanner` 只吐匿名 `Value`，无来源标签 → L2 报不出根类别。
**决定：** 新增**分类根 scanner** hook（`CategorizedRootScanner = Box<dyn Fn(&mut dyn FnMut(&Value, RootKind))>`），
`VmCore` 接线时按类别枚举：`static_fields → StaticField`、每线程 `call_stack` 帧 regs/env → `StackFrame`、
`func_ref_slots → FuncRefSlot`、pinned roots → `Pinned`。**只在诊断查询时调用**，不参与 mark 热路径
（mark 仍用既有匿名 scanner）。类别级即可（具体字段名/局部名 = L3+ 的 root-source 精确标签，延后）。

### D5: 结果类型 + 对象标识
- `RetainerInfo { type_name: String, kind: RetainerKind (Object/Array/Root), label: String, id: usize }`。
- `RootInfo { kind: RootKind (StaticField/StackFrame/FuncRefSlot/Pinned), label: String }`。
- 去重：L2 根按 `kind` 去重（类别级，同类只报一次 + 计数可选）。L1 引用者按 ptr 去重。
- z42 侧 `Std.Diagnostics.Retainer` / `RootRef` 由 builtin 用注册表-object 范式（仿 GC HeapStats）构建。

## Implementation Notes

- **`retention_query` 位置**：`ArcMagrGC`（需访问 `region_object`/`region_array`/roots）。经 MagrGC trait
  暴露 `retention_direct_referrers(target) -> Vec<RetainerInfo>` / `retention_roots(target) -> Vec<RootInfo>`
  （或单个 `retention_query(target, mode)`）。builtin 走 `ctx.heap().…`。
- **target 取 ptr**：builtin 收到 `Value::Object(gc)` / `Value::Array(gc)` → 取 `GcRef::as_ptr` as usize。
  非堆对象（primitive/null）→ 空结果。
- **trace_children 反向**：复用 `Value::trace_children`；对每个存活对象的 Value 包装调用（object 需包成
  `Value::Object` 以复用；或直接遍历 slots/elems 的 heap-ref）。
- **force_collect**：复用既有 `collect`/`ForceCollect` 入口（STW）。查询在 collect 后、同一 STW 或紧邻
  安全点做（避免中途分配扰动）。反向扫描本身在 region 锁下，一致快照。
- **zbc/格式**：无 bump。
- **文件行数**：`retention.rs` 控 300 内；`diagnostics.rs` 同。

## Testing Strategy

- **Rust 单测**（`gc/retention_tests.rs`）：合成堆（object 引 object、array 引 object）→ 反向图正确；
  L1 直接引用者命中；L2 反向 BFS 到根（喂合成分类根）；浮动垃圾（未加根的对象）经 collect 后不报。
- **e2e golden**（`src/tests/reflection/heap_retention/`）：z42 构造 `Holder.field → target`、数组元素、
  static 字段链 → `DirectReferrers` / `RetainingRoots` 断言类型/根类别。
- **零回归**：完整 `xtask test`（含自举 gen1==gen2）。
