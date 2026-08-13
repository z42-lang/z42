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
- [ ] 2.1 `types.rs`：`ScriptObject` 删 `slots`，`bytes`/`struct_refs` 覆盖全字段；`inline_region_sizes` 扩全字段；`trace_children` 改位图扫
- [ ] 2.2 `arc_heap.rs`：`alloc_object` 尺寸；`scan_object_refs`/mark/sweep 位图；字节统计
- [ ] 2.3 `exec_object.rs`：FieldGet/Set/`FieldIC` → byte-offset+kind
- [ ] 2.4 `exec_struct.rs`：对象基址访问推广全字段
- [ ] 2.5 `reflection.rs`：Get/SetValue byte-offset
- [ ] 2.6 `ExprEmitter.z42`：`obj.x` 直接字段 codegen 改 byte-offset（两条路合一，删死 slot）
- [ ] 2.7 `jit/*`：字段访问 byte-offset（引用暂 16B，Value STRIDE 仍 24）
- [ ] 2.8 golden：基元/引用/内联 struct/继承/跨包/反射 + `--mode jit`
- [ ] 2.9 GREEN + 自举 byte-identical

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
