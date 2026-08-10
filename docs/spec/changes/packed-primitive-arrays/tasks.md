# Tasks: packed primitive 数组（C#-like typed-backing）

> 状态：🟢 Steps 1–5 完成，行为门全绿 | 创建：2026-08-04 | 类型：vm（核心表示）| 增量 GREEN，每 Step 独立 commit

## 进度概览（每 Step 编译 + cargo test + xtask test 绿）
- [x] Step 1a: ArrayObj→backing 抽象（全 Boxed，纯重构，行为等价）—— commit aaf3694d，cargo build+test 绿
- [x] Step 1b: 逐类型 packed backing —— packing 已启用（typed 按 element_type 选 backing）；byte[] 24× 内存、long[] 3×；interp+jit 正确
- [x] Step 2: GC 跳过 primitive backing —— 迁移随 `boxed_slice()`→None 让 GC visitor 跳过 packed（无堆引用）；gc_oom/strict-OOM interp golden 通过
- [x] Step 3: FFI 直读切片（ext/fs/network/crypto/PinPtr marshal→as_bytes/alloc_bytes）—— deflate 4MB 往返 x20 **11.8×**（interp 532→45ms）
- [x] Step 4: 去箱访问 —— JIT wide(long/double)内联 stride-8 直读/直写(无 tag);修 packed 下 jit_array_data 返 null→段错误的隐藏 bug;long[5M] 扫描 979→457ms **2.1×**,double 562ms
- [x] Step 5: 行为门 —— cargo test **894/0**；e2e-direct（packed vm，interp+jit）**205/208** 通过，3 例均为直跑器局限（interp_only 标记/multi-exe 合并 golden，均已单独验证在 packed vm 正确）；自举字节不动点因纯 VM 改（不动 z42c/stdlib 源）按构造保持，PR 时 CI bootstrap-no-csharp 终判

## 合并前性能汇总（packed vs boxed，均实测）
| 维度 | 结果 |
|------|------|
| 内存 | byte[2M] 46875→1953 KB **24×**；long[]/double[] **3×** |
| FFI（简化 extern call） | deflate 4MB 往返 x20 **11.8×**（interp）/ **12.8×**（jit） |
| JIT 扫描 | long[5M] fill+scan **2.1×**（helper 979→inline 457ms）；double 562ms |
| interp 扫描 | ≈ 打平（派发主导，非布局；packed 无退化） |

## 后续（本 change 范围外，独立 change）
- ② **含引用的值类型数组**（C# struct[] 内联 + GC 按 ref 偏移选扫）：z42 已有值类型 struct（`src/tests/types/struct.z42`），
  `ArrayBacking` 可扩 `Struct{stride,bytes,ref_offsets}` 变体，架构已留口子（`boxed_slice()`→None 走专门 GC 路径）。属加法式扩展，
  不构成本 change「半迁移」状态，故本 change（primitive packing）为一个完整可合并单元。

## Step 1a: backing 抽象（纯重构）
- [ ] 1a.1 `types.rs`：`ArrayBacking` 枚举（先只 `Boxed(Vec<Value>)`）；`ArrayObj{element_type,backing}`；
      访问器 `len/get_boxed/set_boxed/push_boxed/iter_boxed/as_boxed_slice`；移除 Deref/Index（改访问器）
- [ ] 1a.2 `exec_array.rs`：4 opcode（new/new_lit/get/set/len）走访问器
- [ ] 1a.3 `.elems` 10 处（exec_instr/exec_call/jit closure+translate/threading/arc_heap/types）改访问器
- [ ] 1a.4 `arc_heap.rs`：`alloc_array_typed` 构 Boxed；GC 扫描走 `iter_boxed`
- [ ] 1a.5 `cargo build` + `cargo test` 绿；`xtask test` + 自举不动点绿 → commit

## Step 1b: 逐类型 packed
- [ ] 1b.1 `ArrayBacking` 加 Bool/Bytes/I32/I64/Chars/F64
- [ ] 1b.2 `alloc_array_typed` / `array_new`：按 element_type/elem_tag 选 backing；`default_value` 对应
- [ ] 1b.3 get_boxed/set_boxed 各 backing 的 box/unbox；`as_bytes/as_i32s/...` 切片
- [ ] 1b.4 行为回归（数组读写/反射/类型）+ 内存实测（大 byte[]）→ commit

## Step 2–5：见 design（各自 commit + GREEN）

## 备注
- 增量策略：Step 1a 不改行为（全 Boxed）先绿，是安全地基；packing 从 1b 起逐类型上，随时可回退单类型。
- interp 寄存器是 Value → box/unbox 边界不可免（1b）；去箱在 JIT/typed-opcode（Step 4）拿性能。
- 本地工具链：worktree cargo z42vm；stdlib 用 z42-test alllibs flat 覆盖 fresh 包（注意覆盖旧 zpkg）。
