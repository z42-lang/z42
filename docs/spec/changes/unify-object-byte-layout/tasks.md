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
- [ ] 1.1 `StructLayout.z42`：`_computeObjectLayout`（对象全字段：基元/引用/内联 struct 统一 offset + 引用位图带 kind）；扩展或复用 `_computeInlineLayout`
- [ ] 1.2 `ClassDescBuilder.z42`：发对象完整布局表（暂与 slots 并存，不改 runtime 行为）
- [ ] 1.3 `ZbcFormat.z42` zbc `Minor++` + changelog；`ZpkgWriter.z42` zpkg `Minor++`
- [ ] 1.4 `ZbcWriter.z42` / `ZbcReader.z42` 写/读对象布局表
- [ ] 1.5 `zbc_reader.rs` / `loader.rs` / `bytecode.rs`：读入 `TypeDescCold`（暂不消费）
- [ ] 1.6 `CacheStore.z42` `CompilerFingerprint++`
- [ ] 1.7 格式 fixture 重生（zbc/zpkg）+ golden hex 单测；`StructLayout` 对象布局单测
- [ ] 1.8 GREEN（含 bootstrap 越界检查）

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
