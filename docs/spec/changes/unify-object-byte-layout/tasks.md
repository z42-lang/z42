# Tasks: 统一 struct/class 内存布局 + 引用压 8B

> 状态：🟡 进行中 | 创建：2026-08-12
> 终点 = C# 完全等价（8B 引用 / 非移动 GC / 路 A）；内部分 PR，每 PR 独立 GREEN。

## 进度概览
- [ ] PR-1: 布局元数据（编译器发对象全字段布局表 + reader，行为不变，格式 bump）
- [ ] PR-2: runtime 切字节存储（删 slots → bytes；FieldGet/Set/IC/反射/JIT byte-offset；GC 位图；引用暂 16B）
- [ ] PR-3: 引用 8B 标记指针（GcRef 16→8B 路 A）
- [ ] PR-4: 字符串 8B 细指针（Arc<str>→StrHeader）
- [ ] PR-5: Value 16B + JIT 收尾 + 文档/roadmap 收口

## PR-1: 布局元数据（行为不变）
- [x] 1.1 `StructLayout.z42`：`_computeObjectLayout`（对象全字段：基元/引用/内联 struct 统一 offset + 引用位图带 kind）；复用 `_kindOf`/`_alignOf`/`_alignUp` + 新增 `_objSizeOf`(8B 引用)/`_objLayoutOfStruct`(8B struct 展平)
- [x] 1.2 `ClassDescBuilder.z42`：发对象完整布局表（class/record，暂与 slots 并存）+ `IrModule.z42` IrClassDesc 新字段（ObjectSize/ObjFieldOffsets/Sizes/Kinds/Count/ObjRefOffsets/Kinds/Count）
- [x] 1.3 `ZbcFormat.z42` zbc `Minor` → **34** + changelog；`ZpkgWriter.z42` zpkg `Minor` → **39**；zbc_reader.rs 常量 + changelog；zbc_reader_tests 版本 pin 更新（**rebase 到 #186 后：#186 占了 1.33/0.38 类可见性字节，本 change 提到 1.34/0.39——两者 TYPE 段位置不重叠：可见性字节紧随 class_flags，对象块在 inline 块之后**）
- [x] 1.4 `ZbcWriter.z42` / `ZbcReader.z42` 写/读对象布局块（+ 顺带补 ZbcReader 的 inline 块读取以与 writer 对称）
- [x] 1.5 `zbc_reader.rs`（读对象块→ObjectLayoutDesc）/ `loader.rs`（threadTypeDescCold.object_layout）/ `bytecode.rs`（ObjectLayoutDesc + ClassDesc.object_layout）/ `types.rs`（TypeDescCold.object_layout）——暂不消费
- [x] 1.6 `CompilerFingerprint++` —— **跳过**（格式 Minor bump 已令 `.meta` 旧条目失效，指纹冗余，version-bumping.md 明确不必动）
- [~] 1.7 `StructLayout` 对象布局单测（4 例，layout_tests.z42）✅ + doc-sync（zbc.md/zpkg.md changelog + version-bumping 表）✅ | **fixture 重生（zbc/zpkg）+ golden hex 待 CI**（macOS 两代自举墙，见备注）
- [~] 1.8 GREEN —— 本地可验部分：cargo build ✓ / cargo --lib 919 pass（16 个格式-预期失败=待 0.39 fixtures）/ z42.ir 编译 ✓ / semantics 语法 ✓ | **full self-host + fixtures 以 CI 为准**（格式 bump 冷路径本地不可验，bootstrap-seed.md）。rebase 到 #186 后格式为 zbc 1.34/zpkg 0.39

## PR-1 关键实现决策（本次落地，记录供 PR-2 复核）
- **引用叶子按 8B**（非 16B）：User 裁决。`_objSizeOf` 引用返 8（`_alignOf` 现已对引用返 8，故对齐无需变）。内联 struct 字段用 8B 版 `_objLayoutOfStruct` 展平（与 16B 版 `_compute` 分开，后者 runtime 仍消费）。
- **派生谓词 gate（不扩 flags）**：class flags U8 满位（bit0-7 全占），扩 U16 涟漪大 → 对象块用**派生谓词** `(class_flags & 116)==0`（116=struct4|interface16|enum32|delegate64）gated，writer(ZbcWriter/ClassDescBuilder)与 reader(ZbcReader/zbc_reader.rs) 同谓词锁步。普通引用类（class/record）发，struct/interface/enum/delegate 不发。
- **对象块格式**：`ObjectSize:u32 + field_count:u16 + (off:u32,size:u32,kind:u8)×n + ref_count:u16 + (ref_off:u32,ref_kind:u8)×m`。per-field 供 FieldGet/反射，扁平引用位图（含内联 struct 内部叶子）供 GC。
- **OWN 字段 offset 从 0**：继承 base-shift 留 PR-2 消费时组合（镜像 fields=base++own）。
- **休眠**：`TypeDescCold.object_layout` 携带 `Arc<bytecode::ObjectLayoutDesc>`，runtime 不消费（仍 slots）；PR-2 转可消费形式替换 slots。

## PR-1 备注：格式 bump 本地验证墙 + CI 收尾
- 本地 macOS 两代自举墙（实证 2 次）：`xtask build compiler` 第一步 cargo 重建 z42vm 为新版，读不了旧 seed → 无法本地跑 full self-host + 无法本地重生新格式 fixtures。established 路径（add-crosspkg-internal-class 复用经验）：推 PR → CI ci-bootstrap 两代自举验 self-host + 产 `current-sdk-<os>` artifact（0.39 z42c+stdlib）→ 下载配本地 cargo 0.39 z42vm → 换 `.z42` 成 0.39 seed → 重生 zbc/zpkg fixtures + golden hex → 本地 cargo test 全绿 → commit fixtures。
- **待 CI 收尾项**：① fixture 重生（`src/tests/zbc-format/*.zbc` ×6 + `src/tests/zpkg-format/*.zpkg` ×4）；② golden hex（`z42c.semantics/tests/zbc/zbc_tests.z42` 的 empty.zbc hex，会因 header minor 变）；③ full `xtask test` GREEN + `xtask test bootstrap` 越界检查（无新语法，预期过）。

## PR-2: runtime 切字节存储
> **方案已定（2026-08-14，见 design.md D8-D11）**：Option B 运行时组合。`ScriptObject` = `bytes`（全
> 基元叶子 composed offset + 引用字段 8B 洞）+ `refs`（全引用叶子，composed 位图序），删
> `slots`/`struct_bytes`/`struct_refs`。直接字段仍发字段名（零编译器改动），运行时 name→slot→composed
> offset。内联 struct 叶子编译器烘焙 composed offset（+base-shift）。**继承边界统一 8B 对齐**（两处组合
> 算法逐字节一致）。引用暂 16B（8B 留 PR-3）。**无格式 re-bump**（zbc object_layout 仍 own-only）。
> **爆炸半径**：151 处 `.slots` + 40 处 `.struct_bytes/.struct_refs`（非测试，~15 文件）+ 编译器/JIT/反射/static + 数十测试。**all-or-nothing，须一次落地才绿**。
- [ ] 2.0 运行时 composed 对象布局（先做，可 additive 测）：新增运行时 `ObjectLayout`（total_size + per-slot offset/size/kind + composed 引用位图）on `TypeDescCold`；loader 组合 `base.composed ++ own`（own 从 zbc `object_layout`；own 区起始 = `align_up(base_size, 8)`，镜像 `merge_with_base`）；cargo 单测继承组合 offset。**此步先落 + 单测 de-risk byte-identical 分歧**
- [ ] 2.1 `types.rs`：`ScriptObject` 删 `slots`/`struct_bytes`/`struct_refs` → `bytes`+`refs`；`object_regions()`（复用 is_struct 走 struct_layout、否则 composed）；`trace_children` Object/BoxedStruct/RefKind::Field 臂改 `for r in &obj.refs`
- [ ] 2.2 `arc_heap.rs`：`alloc_object`（删 slots 参数，从 composed 布局 size bytes+refs）/`alloc_boxed_prim`（struct_bytes→bytes）；`scan_object_refs`/mark/sweep 扫 `refs`；write_barrier
- [ ] 2.3 `exec_object.rs`：FieldGet/Set/`FieldIC` 全接收者臂（StackObject/Object）→ name→slot→composed offset → `decode_prim(bytes)` / `ref_index`→`refs[ri]`；obj.new caller 删 slots 构建
- [ ] 2.4 `exec_struct.rs`：`struct_field_get_val`/`set_val` 的 `Value::Object` 臂 `struct_bytes`→`bytes`、`struct_refs`→`refs`、`inline_layout()`→composed 位图；`unbox_struct`/`copy_array_elem_out` 同步
- [ ] 2.5 `reflection.rs` + 其余 ~15 文件的 `.slots`/`.struct_bytes`/`.struct_refs` 直读点（repl/gc/snapshot/stack_alloc/jit frame/assemblyloadcontext/exception 等）迁移 byte-offset+ref
- [ ] 2.6 `ExprEmitter.z42` + `StructLayout.z42`：内联 struct 叶子根 offset 从 `_inlineCache`（struct_bytes 相对）改 `_objectCache` composed（+base-shift 组合，与 loader 8B 对齐一致）；直接字段 codegen 不变（仍发名）
- [ ] 2.7 `jit/*`：`jit_field_get/set` + 方案 B 内联 STRIDE=24 硬编码 → byte-offset 基址（引用暂 16B，Value STRIDE 仍 24）
- [ ] 2.8 追加 static 字节化（见 PR-2-static 节）
- [ ] 2.9 golden：基元/引用/内联 struct/继承/跨包/反射 + `--mode jit`；更新 ~十余 `arc_heap_tests` 等测试
- [ ] 2.10 GREEN + 自举 byte-identical（分歧则查两处组合算法）

## PR-2 追加: static 存储字节化（修 REPL struct-in-static 悬垂，design.md D11）
- [ ] S.1 `vm_context.rs` `VmCore.static_fields: Vec<Value>` → 每类静态存储块「offset 字节内联」（static struct 字段内联字节；引用字段存句柄一槽一引用）
- [ ] S.2 `ExprEmitter.z42` `StaticSet`/`StaticGet`（`:639`/`:134`）：static struct 字段发字节拷贝（非裸 StructRef 句柄）；逃到 static 时拷字节
- [ ] S.3 e2e `src/tests/types/struct_static_field.z42`：`class Holder { static Point P; }` 存/取/就地改/拷出独立副本
- [ ] S.4 REPL 回归：`struct B{...}; B b=new(); b` + carry-forward 不崩（需 `xtask build toolchain` + 手动 REPL 验，见 [[green-gate-skips-scripting-interactive]]）

## PR-3: 引用 8B 标记指针（路 A）
- [ ] 3.1 `refs.rs`：`GcRef` 16→8B，窄 generation 进高 16 位 + mask deref；alloc 写 generation
- [ ] 3.2 对象布局引用 offset 16→8B；GC 按 8B 读；写屏障
- [ ] 3.3 ABA 窄 generation 评估 + Miri/ASAN；平台（ARM MTE/PAC）验证
- [ ] 3.4 GREEN

## PR-4: 字符串 8B 细指针
- [ ] 4.1 `StrHeader{len,[u8]}` 细指针表示；`Value::Str` payload 8B
- [ ] 4.2 所有 `Arc<str>` 用点迁移（corelib string/convert、marshal、interning）
- [ ] 4.3 `Length` benchmark（string-heavy 自编译）
- [ ] 4.4 GREEN

## PR-5: Value 16B + 收尾
- [ ] 5.1 `Value` payload 收窄 → enum 16B；value_layout 断言
- [ ] 5.2 `jit/translate.rs` STRIDE 24→16 / PAYLOAD；数组元素 16B
- [ ] 5.3 `z42-abi` 断言 + marshal 复核
- [ ] 5.4 `object-abi.md` §3/§2.1/§5 修订；`zbc.md`/`zpkg.md` changelog；`struct-value-semantics.md`；`roadmap.md` 385/386
- [ ] 5.5 目录 README 同步（触发矩阵）
- [ ] 5.6 GREEN + dist（若涉及）

## 备注
- Open Question（design）：窄 generation 位宽抗 ABA？字符串 Length deref 代价？bootstrap 格式 bump 时序？
- 复用点见 design.md Implementation Notes，勿重造 P3b 机制。
