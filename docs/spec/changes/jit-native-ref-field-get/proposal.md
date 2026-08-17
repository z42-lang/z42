# Proposal: JIT 对象引用字段 GET 原生内联（T1-B）

> 状态：IMPL（vm 类，纯 runtime JIT codegen，无格式 bump、自举字节不动）。
> P5-B（对象**原语**字段原生字节内联）的**引用侧对偶**——把对象/数组**引用**字段的 FieldGet 从
> `jit_field_get` helper 提升为原生 8B tagged 指针 load。单逻辑单元、单 commit、单 PR。

## Why

P5-B 后原语字段走原生，但**引用**字段（类实例 / 数组类型）的 FieldGet 仍 100% 走 `jit_field_get`
helper——同一层 native→Rust 调用 + `RefCell` borrow + `FieldIC` 桥。引用字段密集的热循环（对象图遍历、
成员集合 / 子对象反复读）因此 JIT≈interp。

实测「非可提升的引用字段读+写热循环」（`this.cur`/`this.alt` 每迭代读+写）JIT 仅 **1.05×** interp——
正是 P5-B 前原语场景 1.09× 的翻版：字段访问 helper 桥把 JIT 压到几乎白 JIT。

前置地基 = 统一对象堆（`unify-gc-heap`）：引用现在是**单机器字的非拥有 `GcRef` tagged 指针**（无 Arc /
无 Drop），故「原生 store 一个引用 `Value`」得以成立。布局统一前引用 payload 需 Arc 处理，
`reg_access.rs` 旧注释「堆 tag 从不原生 store」在统一堆后过时。

## What Changes

1. **`metadata/types.rs` `ScriptObject::inline_ref_field(name)`**（`inline_prim_field` 的引用对偶）：
   字段是**字节内联**的 object/array 引用（`ref_slot==-1` 且 `tag∈{TAG_OBJECT,TAG_ARRAY}`）→ 返回
   `(bytes.as_ptr(), offset, is_array)`；否则 `None`（原语 / 侧表引用 closure·func·**string** /
   struct 根）。

2. **`jit/helpers/object.rs` `jit_obj_ref_field_slot`**（`jit_obj_field_slot` 的引用对偶）：非抛异常解析
   器，写回 `(out_bytes_ptr, out_off, out_tag)`，`out_tag` = 非空 load 要打的 `Value` 判别 tag
   （`7`=`Object` / `6`=`Array`，因 `IrType::Ref` 不区分二者故运行期一次解析后 hoist）；非 fast-path 写
   `out_off=-1`。

3. **`jit/translate.rs`**：
   - 入口块新增 `hoisted_ref_fields` hoist（`hoisted_fields` 的引用对偶）：对每个 FieldGet 的 dst 是
     `IrType::Ref` 且 receiver 从不被重赋值的 `(obj, field)`，一次解析 `(bytes_ptr, off, tag)`。
   - FieldGet 臂新增第三分支（`prim` → `ref` → `helper`）：`brif off<0` → helper 兜底；否则 native——
     `raw = load i64` at `bytes_ptr+off`；`raw==0` → `Value::Null`（仅 tag）；否则 `store_tagged(tag, raw)`
     把 8B tagged 指针原样作 payload。**只读、无 write barrier**。

## 正确性关键

- **逐字节等价**：`GcRef` 恰 8B；`to_tagged_bits()`（`write_inline_ref` 写进 `bytes` 的值）硬件上逐位等于
  其内存表示 = 寄存器 `Value::Object` 的 payload（provenance exposure 运行期 no-op）。故「原生拷 8B + 打
  tag」= helper 路径「`from_tagged_bits`→`Value::Object`→写寄存器」同结果；`raw==0` 分支复刻
  `read_inline_ref` 的 `0→Null`。
- **Drop 安全**：gate = `reg_types[dst]==IrType::Ref` ⇒ dst 旧值 ∈ Drop-free 集合
  `{Object,Array,Null,StackObject,StackArray}`（Box 变体 Ref/PinnedView 的产生指令本就 bail JIT；
  string 是 `IrType::Str` 被排除）⇒ native `store_tagged` 覆写无需 drop。
- **GC 安全**：load 与 store 间无 safepoint；非移动 GC + 对象持活 ⇒ `bytes_ptr` 整帧有效。
- **回落等价**：`off=-1`（非 Object receiver 含 OSR `StackObject` / 侧表引用 / struct 根 /
  Str.Length / null-throw）→ `jit_field_get`，与改前逐字等价。原语 XOR 引用，故与 `hoisted_fields` 永不重叠。

## Out of Scope

- **引用字段 SET**：写要 GC write barrier，仍走 helper（本 change 只做 GET）。
- **string 字段**：`IrType::Str` / 侧表 GcRef，一律 helper（Str→GcRef 边界更微妙）。
- **重赋值 receiver 的引用字段**（链表遍历 `cur=cur.next`）：不适配 hoist 模型，需 per-access 内联缓存
  （T2-D 方向），另立。

## 效果 / GREEN

- reffield 热循环 JIT **1.48s→0.90s（JIT 自身 1.64×）**、jit-vs-interp **1.05×→1.71×**。
- 正确性：object/array/null/循环内改写混合场景 interp==jit==jit-OSR 逐字节（校验和 297000000）。
- GREEN：cargo --lib 927+21/0 + `xtask test all`（e2e interp+vm-jit-consistency + stdlib + 自举
  5/5 gen1==gen2 逐字节 + vscode）。无格式 bump。
