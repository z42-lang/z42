# Tasks: 融合字符串拼接分配（fuse-str-concat-alloc）

> 状态：🟢 已完成 | 创建：2026-08-17 | 完成：2026-08-17 | 类型：perf（最小化模式）

**变更说明：** 给 GC 堆加 `alloc_str_concat2(a, b)`——按 `a.len()+b.len()` 一次性分配
`BlockType::Str` 块并直接拷入两段，取代 `alloc_str(&format!("{a}{b}"))` 的**两次堆分配**
（中间 `String` + GC 块）。`StrConcat` IR op（字符串 `+` / `Std.String.Concat`）与 interp
`Add` 的字符串臂改走它；JIT 侧 `jit_str_concat` helper 同步。

**原因：** re-profile（2026-08-17，string/alloc-heavy）显示分配是本 workload 主成本
（var_region alloc 5.5% + memmove/memset 18% + mimalloc 11.4%），而 `str_concat` 每次都先
`format!` 出中间 `String` 再拷进 GC 块 = 每次拼接两次分配。融合后省一次堆分配 + 一次拷贝。
（本 change 取代已弃的 A1「current_heap TLS」——实测 `_tlv_get_addr` 非 current_heap 主导，
见 memory [[gc-post-unify-optimization-backlog]]。）产出**字节相同**的字符串，无外部行为变化、
无格式 bump、自举字节不动。

**文档影响：** `docs/book/` GC/var_region 机制页补 `alloc_str_concat2` 融合分配说明（对齐日期刷新）。

## 进度概览
- [x] 1. heap.rs：MagrGC trait 加 `alloc_str_concat2` 默认实现
- [x] 2. arc_heap.rs：ArcMagrGC 覆写 + 私有 `alloc_str_concat2_in_region`
- [x] 3. exec_value.rs：`str_concat` / `add` 字符串臂改走 `ctx.heap().alloc_str_concat2`
- [x] 4. exec_instr.rs：`Add` / `StrConcat` 分发点补传 ctx
- [x] 5. jit/helpers/value.rs：`jit_str_concat` 改走 `alloc_str_concat2`
- [x] 6. cargo build z42vm + microbench 复测（interp==jit==1021406250 字节相同；hyperfine 1.30× 更快，mimalloc 12%→5.9% 减半）；顺带删净死代码 `ops::str_val`（唯一 caller 已改）
- [ ] 7. GREEN（cargo --lib + xtask test all + 自举 5/5 逐字节 + vm-jit-consistency）
- [x] 8a. 文档同步：docs/design/runtime/gc.md 变长块堆节加「融合拼接分配 alloc_str_concat2」段
- [x] 7. GREEN 全绿
- [x] 8b. 归档 + PR

## 验证报告
- **cargo --lib**：917 + 21 = 938 passed, 0 failed, 2 ignored ✅
- **xtask test all --skip zzz**：✅ GREEN — all stages passed (C#-free)
  - e2e（interp）+ cross-zpkg + stdlib [Test] + z42c [Test] 24 + **self-host 5/5 gen1==gen2 逐字节** + vscode-syntax
  - self-host 字节相同 = z42c 自编译（重度 StrConcat）产物零漂移 → 融合分配字节正确
- **正确性**：allocbench interp==jit==1021406250（字节相同）
- **性能**：hyperfine 融合 VM **1.30×** 更快（±0.01）；profile mimalloc 压力 **12.0%→5.9%**（减半）
  - ⚠️ 1.30× 是 O(n²) 增长拼接放大（每次拼接省 2 次 O(n) 拷贝 + 1 次分配）；定长拼接得「省 1 次分配」收益，较小但仍正
- **无格式 bump**（纯 runtime，字符串字节不变）

## 备注
- 无格式 bump（纯 runtime；字符串字节不变）。
- interp `Add` 字符串臂是防御性回退（字符串 `+` 编译期恒 lower 成 `StrConcat`）；一并融合保持一致。
