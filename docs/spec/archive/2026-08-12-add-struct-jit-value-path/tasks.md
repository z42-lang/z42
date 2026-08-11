# Tasks: JIT struct 值路径（P5-A helper 桥接）

> 状态：🟢 已完成 | 创建：2026-08-12 | 完成：2026-08-12

## 进度概览
- [x] 阶段 1: frame_id 地基（纯惰性 + OSR 继承）
- [x] 阶段 2: struct helper（4 个）+ 注册
- [x] 阶段 3: translate 接线（bail→call）+ array_get/as_cast + **array_new StructBytes backing（实施期补）**
- [x] 阶段 4: 测试（5 单元 + JIT golden struct_jit.z42）
- [ ] 阶段 5: 验证 GREEN + 文档同步

## 阶段 1: frame_id 地基
- [x] 1.1 `interp/exec_struct.rs`：`decode_prim`/`encode_prim`/`prim_width`/`is_ref_tag`/`resolve_layout` 由 `pub(super)`→`pub(crate)`（零逻辑改动）
- [x] 1.2 `jit/frame.rs`：`JitFrame` 加 `frame_id: u32`，4 个构造函数初始化为 0（默认哨兵）
- [x] 1.3 纯惰性分配：`struct_ops::frame_id_of` 在分配型 helper 里 `if frame_id==0 { =next_frame_id() }`——覆盖入口+所有嵌套帧，零 per-site churn（取代 eager 设各帧创建点）
- [x] 1.4 OSR 交接帧（`interp/mod.rs` `from_interp_regs` 调用点）**继承** interp 帧 frame_id（先于惰性，deref 一致性）

## 阶段 2: struct helper + 注册
- [x] 2.1 `jit/helpers/struct_ops.rs`（NEW）：`jit_struct_alloc`（arena alloc + 写 StructRef 到 regs[dst]）
- [x] 2.2 `jit_struct_copy`（arena copy_into，值独立）
- [x] 2.3 `jit_struct_field_get_prim`（base 三臂：arena StructRef / 堆 Object / StructRefHeap，镜像 interp）
- [x] 2.4 `jit_struct_field_set_prim`（三臂 + 堆 Object 引用叶子写屏障）
- [x] 2.5 helper 异常约定（失败 `set_exception` + 返回非 0，translate 侧跳异常）
- [x] 2.6 `jit/helpers/mod.rs`：`mod struct_ops;` + 导出
- [x] 2.7 `jit/helpers/registry.rs`：4 个 helper 的 `reg!` symbol + `decl!` FuncId 签名 + `HelperIds` 字段

## 阶段 3: translate 接线 + array_get/as_cast
- [x] 3.1 `jit/translate.rs`：import 4 个 helper FuncRef（`imp!(helper_ids.struct_*)`）
- [x] 3.2 `translate.rs`：4 条 struct 指令的 bail（:1504-1509）换成对应 helper call + 参数封送（type_name 作 ptr/len）
- [x] 3.3 `jit/helpers/array.rs`：`jit_array_get` 加 StructBytes→StructRefHeap 特判（镜像 interp array_get）
- [x] 3.4 `jit/helpers/object.rs`：`jit_as_cast` BoxedStruct 精确匹配→拆箱 arena StructRef（用 frame_id）+ StructRefHeap 拷出（foreach）+ 删旧「JIT 无 frame_id」注释
- [x] 3.5 **（实施期补，D6）** `jit_array_new`/`jit_array_new_lit` 对 value-struct 元素造 StructBytes backing（复用 interp `try_struct_backed`/`pack_struct_elem`，提 `pub(crate)`）；`new_lit` 返回 `()→u8` + translate `check!`——否则 JIT 下 `new Point[]` 造 Null 数组、`arr[i]` 无 StructRefHeap

## 阶段 4: 测试
- [x] 4.1 `jit/helpers/struct_ops_tests.rs`（NEW）：5 组单测（alloc+frame_id / 字段读写 round-trip / copy 值独立 / 非 struct base 抛异常 / 悬垂 frame_id 抓）；堆 Object/StructRefHeap/引用叶子/拆箱走 golden
- [x] 4.2 `src/tests/types/struct_jit.z42`（NEW）：JIT golden 综合用例（本地+嵌套+string+struct[]+装箱拆箱）
- [x] 4.3 确认既有 `struct*.z42` golden 在 `--mode jit` 全过（现在真走 JIT struct 路径而非 bail→interp）

## 阶段 5: 验证 GREEN + 文档
- [x] 5.1 `cargo build --release`（z42vm）无错 + `cargo test --lib` 全绿（882+21 passed，含 5 新单测）
- [x] 5.2 `xtask test`（不传 Z42_HOME）全 stage 绿 + self-host **5/5 byte-identical**（z42c 零改动）
- [x] 5.3 `xtask test e2e --mode jit`（JIT 专腿）全量 golden 绿 + 8 struct golden `--mode jit` 全过 + `struct_jit.z42` 双模式 EXIT=0 输出一致
- [x] 5.4 spec scenarios 逐条覆盖确认（golden struct_jit.z42 覆盖全部 8 场景）
- [x] 5.5 `docs/book/src/runtime/struct-value-semantics.md` 加「JIT 值路径」节 + 页头对齐 + 更新 as_cast/Deferred 陈述
- [x] 5.6 `docs/roadmap.md` Deferred 索引更新（P5-A 已落、P5-B 原生内联记 Deferred）
- [x] 5.7 JIT `README.md` helpers 表加 `struct_ops.rs`
- [x] 5.8 **数据对比**（min-of-3，20M 迭代，Bench 被调 20000×→函数级 JIT）：

  | workload | jit-BEFORE（bail→interp） | jit-AFTER（P5-A） | interp-ref | P5 加速 |
  |----------|---------------------------|-------------------|-----------|---------|
  | struct-op 密集（每迭代 alloc+copy+3 字段 op） | 7.32s | 6.67s | 7.45s | **1.10x** |
  | 算术密集 + 少量 struct 字段 | 1.72s | 1.31s | 1.71s | **1.31x** |

  - **sanity 实测**：before-VM 对 struct 函数 `jit≈interp`（s_heavy jit=7.26 ≈ interp=7.25）——**证实 jit-before 确实 bail→interp**。
  - **结论**：P5-A 消除「含 struct 函数整体 bail」；加速随非-struct 工作占比升（1.10x→1.31x），上限≈本 VM JIT 标量循环天花板（纯 long 循环实测 1.29x）——helper 桥接把非-struct 部分的 JIT 收益**足额**带给 struct 代码。struct op 本身仍 interp 速度（P5-B 原生内联的空间）。
  - 说明：本 VM interp 高度优化、JIT 增益本就温和（与既有 perf 记录一致）；double 转换循环因 `jit_convert` per-iter helper 而 helper-bound（故基准用 long 免转换混淆）。

## 状态：🟢 已完成 | 完成：2026-08-12

## 备注
- 格式中立（无 zbc/zpkg bump、无新指令）→ 无 fixture 重生、无两代自举、warm 环境全程本地验证。
- 主门 = CI `test-vm-jit(linux-x64)`（vm-jit-consistency）；本地 `xtask test e2e --mode jit` 已对齐通过。
- Scope 实施期扩展（已同步 proposal）：`jit_array_new`/`new_lit` StructBytes backing + interp `exec_array`/`exec_struct` 可见性 + `interp/mod.rs` 模块可见性 + `exec_object.rs` frame_id 调用点。
