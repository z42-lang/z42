# Fix: JIT 对象字段 helper 未处理 StackObject → OSR 下 stack-alloc 对象崩

> 状态：IMPL（bug fix，纯 runtime，无格式 bump、自举字节不动）。
> 关系：[`fix-jit-osr-stackarray`](../../archive/) (#204) 的**对象侧对偶**——那次修数组三 helper 并把本 fix
> 明确列为 follow-up（"JIT 的对象字段 helper 同样只处理 `Value::Object`、不处理 `Value::StackObject`…
> 作独立 follow-up：先构造能触发的 stack-object-in-OSR-loop 用例，再镜像 interp 的 StackObject 字段逻辑"）。
> 本 change 补上这条对偶。

## 症状

`Z42_OSR_THRESHOLD=1`（强制每循环走 OSR）下，跑一个**局部对象被逃逸分析栈分配**的 `--release` 程序，
JIT 段访问该对象字段抛：

```
Error: uncaught exception: FieldSet: expected object, got StackObject { idx: 0, frame_id: 18 }
```

默认阈值（10000）下自然 OSR 极少触发，故平时门禁绿、**latent**。但 z42c / xtask 自身是 `--release`
的大型 z42 程序（逃逸分析开、栈分配对象如 `InlineState`/`InlineCtx`），长循环在 jit 模式下自然 OSR 后
即可能命中——即 #204 里 `Z42_OSR_THRESHOLD=1` 全 e2e 崩在 golden regen 的同一类交互（那次是数组）。

## 根因

逃逸分析栈上分配（`escape-analysis-stack-alloc`）把不逃逸的局部对象分配进 **interp 的 per-context
stack arena**，值为 `Value::StackObject { idx, frame_id }` 句柄。**interp 的 `exec_object` 的
`field_get`/`field_set` 都处理 StackObject**（经 `ctx.stack_arena` 解析、复用同一 FieldIC），但
**JIT 的对应 helper（`jit/helpers/object.rs` 的 `jit_field_get`/`jit_field_set`）只处理
`Value::Object`、把 StackObject 落到 `other =>` 报错**。

平时不出问题：非 OSR 的 JIT 函数从头在 JIT 跑，其 `ObjNew` **忽略 stack_alloc、一律走堆分配**
（`translate.rs:1397` "JIT ignores stack_alloc in v1"）→ 不产 StackObject。**但 OSR 是 interp→JIT
中途切换**：interp 段先创建了 StackObject（在 `frame.regs` 里），回边 OSR 进 JIT 后，JIT 代码访问该
对象字段 → 命中未处理 StackObject 的 helper → 崩。

（JIT 的**原生字节内联**字段快路径不受影响：其 hoist helper `jit_obj_field_slot` 对非
`Value::Object` 返回 `off=-1`，把每次访问路由到冷 helper——所以修 helper 即全覆盖，与数组侧对称。）

### 为何 #204 当时未能复现（关键）

#204 说"手工构造 `new P(...)` 局部对象在循环里字段访问也未复现（ctor 调用使对象逃逸→堆）"。真正原因
**不是 ctor 逃逸**，而是：`z42c build <toml>` **默认 debug**（`optSet = Opt.ProfileDefault(false) =
Opt.None`）→ 逃逸分析 pass 根本不跑 → 对象一律堆分配。必须 **`build … --release`**（`Opt.All` 含
`Opt.StackAlloc`）才让逃逸分析栈分配对象，OSR 下方能命中。另一必要条件：对象须**在循环外创建**、循环内
只字段访问（循环内 `new` 会被 JIT 每迭代堆分配覆盖 → 无 StackObject）。二者齐备即稳定复现。

## 修复

给 `jit_field_get`/`jit_field_set` 各加一条 `Value::StackObject` 臂，**镜像 interp `exec_object`**：
经 `vm_ctx_ref(ctx).stack_arena.lock().with_obj`/`with_obj_mut(idx, frame_id, …)` 解析读写，
**复用 per-site `FieldIC`**（栈对象与堆对象携带同一 `type_desc.id`、`field_index` 按类型，故
`(TypeId→slot)` 缓存一致）；set **不发 GC write barrier**（栈槽非堆槽，栈对象的堆-ref 字段由 arena
root scan 保活），与 interp 一致。stale-handle / 越界经 `with_obj` 的 `Result` → `set_exception` 抛出。

回落安全：非 Object/StackObject receiver（Str.Length/ByteLength、Array.Length/Count）保持原臂；其余
仍走 `other =>` 抛错，逐字等价改前。

## 验证

- **直接 before/after**（`scratch_bench/soloop2.z42`，`--release`，循环外建 `P`、循环内 `p.x=i` +
  读 `p.x/p.y`）：
  - 改前：`Z42_OSR_THRESHOLD=1 --mode jit` → `FieldSet: expected object, got StackObject` 崩；
    `--mode interp` = `201300000`（`SO2.P` 栈分配 100 次）。
  - 改后：interp == jit(default) == jit(OSR=1) == `201300000`。
- **多字段宽度**（`scratch_bench/sotypes.z42`）：sbyte/short/int/long/byte/ushort/uint/double/bool/char
  全宽字段 get+set 于 OSR'd 热循环，三模式逐一致 `2743288800`。
- **真实压测**：用修好的 vm 让 **z42c 在 `--mode jit Z42_OSR_THRESHOLD=1` 下 `--release` 编译
  z42.collections（5 文件）** 成功（z42c 自身栈分配对象+数组在强制 OSR 下正确）——对应 #204 里崩掉的
  golden-regen（z42c 自执行）同类路径。
- **回归**：`cargo test --lib` 989/0；`xtask test all`（e2e interp+jit + stdlib + compiler 自举
  5/5 gen1==gen2 逐字节 + vscode）。纯新增 StackObject 臂、不改 Object 路径 → 默认路径零影响。

## 相关

- 逃逸分析栈分配：`escape-analysis-stack-alloc`（interp arena；对象+数组）。
- OSR：`add-osr-loop-tiering`（interp→JIT，`from_interp_regs` 拷 `frame.regs`）。
- 对偶：`fix-jit-osr-stackarray`（#204，数组三 helper）。
- **已知残留同类**：`jit_obj_field_slot` 的原生内联字段快路径已对 StackObject 回落 helper（安全）；
  内联 struct 叶（StructFieldGetPrim/SetPrim）在 OSR 下的栈 struct 句柄未单独验证，若未来复现另立 follow-up。
