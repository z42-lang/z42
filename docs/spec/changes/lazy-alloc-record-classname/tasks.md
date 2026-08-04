# Tasks: 对象分配去掉每次的类名 String 克隆

**变更说明：** `alloc_object` 每次 `new` 都 `type_desc.name.clone()`（类名 String 堆分配+memcpy）传给
`record_alloc` 的 `AllocKind::Object{class}`，但该 `class` 仅在**装了 alloc sampler 时**（罕见的采样场景）
被消费。改 `record_alloc` 收 `impl FnOnce()->AllocKind` 闭包，仅采样时构造 `AllocKind`（含类名 clone）；
热路径不装 sampler 时永不构造。对象保留一个便宜的 `Arc<TypeDesc>`（原子 refcount，非 String alloc）供闭包懒取名。
**原因：** 对象密集代码每次 `new` 一次多余的类名 String 分配；实测 OO 循环 interp −7.5% / jit −10%。
**文档影响：** 无（GC 内部，行为不变；sampler 采样内容不变，2 个 sampler 单测通过）。

- [x] 1.1 `gc/arc_heap.rs::record_alloc`：`kind: AllocKind` → `kind_fn: impl FnOnce()->AllocKind`，仅采样分支调用
- [x] 1.2 array 调用点：`|| AllocKind::Array{..}`
- [x] 1.3 object 调用点：捕获 `Arc<TypeDesc>`，`|| AllocKind::Object{class: td.name.clone()}`；删热路径 eager clone
- [x] 1.4 正确性：cargo test 897/0（含 2 sampler 单测，懒取名内容不变）；snapshot 用 ErasedKind 不受影响
- [x] 1.5 性能：OO 循环 interp 6200→5740ms(−7.5%) / jit 3521→3171ms(−10%)
- [x] 1.6 e2e-direct 205/208（interp+jit，=baseline 同款 3 例直跑器局限，零回退）→ PR
