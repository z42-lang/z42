# Tasks: packed primitive 数组（C#-like typed-backing）

> 状态：🟡 进行中 | 创建：2026-08-04 | 类型：vm（核心表示）| 增量 GREEN，每 Step 独立 commit

## 进度概览（每 Step 编译 + cargo test + xtask test 绿）
- [x] Step 1a: ArrayObj→backing 抽象（全 Boxed，纯重构，行为等价）—— commit aaf3694d，cargo build+test 绿
- [ ] Step 1b: 逐类型 packed backing（byte/char/int/long/double/bool）
- [ ] Step 2: GC 跳过 primitive backing + write-barrier 仅 Boxed
- [ ] Step 3: FFI 直读切片（ext/fs/network/crypto marshal → as_bytes）
- [ ] Step 4: 去箱访问（JIT 直接 buf[i]；interp typed 快 opcode）——性能超 1.35×
- [ ] Step 5: 收尾（.elems 残余 + 反射 + 文档 + bench before/after）

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
