# zbc 字节码格式

> **页型**: 参考页 ｜ **状态**: ✅ 已实现（v1.27）｜ **代码**: `src/compiler/z42c.ir/src/BinaryFormat/`（`ZbcFormat.z42` / `ZbcWriter.z42` / `ZbcInstr.z42`）
> **相关**: [源代码编译流程](../../../internals/src/compiler/source-compile.md) · [zpkg 包格式](zpkg.md) ｜ **对齐**: 2026-07-19

## 概述

`.zbc` 是 z42c 为单个模块产出的平台无关字节码：寄存器式指令流 + 元数据 section。它是编译流程写出阶段的产物，也是虚拟机的输入。当前版本 **1.27**（`major=1, minor=27`）。

本页是 wire format 参考——文件如何逐字节编码；指令的执行语义见运行时部分。

## 约定

- **字节序**：所有多字节整数一律**小端（little-endian）**。
- **原语**：

| 记法 | 编码 |
|------|------|
| `u8` / `u16` / `u32` | 定宽无符号整数（LE） |
| `i64` | 定宽有符号整数（LE） |
| `varint` | 无符号 LEB128（每字节低 7 位 + 续位；最多 5 字节） |
| `str` | `u16 字节长` + UTF-8 字节 |
| `utf8` | 裸 UTF-8 字节，无长度前缀（用于 magic、section tag） |
| `pool idx` | `u32`，指向 STRS 字符串池（0-based） |

- **寄存器**：`u16` 索引，每函数前 `param_count` 个为入参。无目标寄存器时写 `0xFFFF`。
- `u32` 字段无值时约定为 `0xFFFFFFFF`（如无基类、catch-all 类型）。

## 文件布局

### 文件头（16 字节）

```
偏移  字段            宽度   值
0     magic           3 B    ASCII "ZBC"
3     (补零)          u8     0x00
4     version_major   u16    1
6     version_minor   u16    27
8     flags           u16    见下
10    section_count   u16
12    reserved        u32    0
```

**flags**：`bit1 (0x02) = HasDebug`（含 DBUG section）。zbc 仅使用此位。

### Section 目录

紧跟文件头，每条 12 字节；数据区紧跟目录，首段偏移 `= 16 + section_count × 12`。

```
tag      4 B    ASCII section 标签
offset   u32    从文件头起的绝对字节偏移
size     u32    该段字节数
```

### Section 顺序

固定写出 8 段：`NSPC` → `STRS` → `TYPE` → `SIGS` → `IMPT` → `EXPT` → `FUNC` → `REGT`；其后按需追加 `DBUG`（任一函数有行号/局部变量名）、`TIDX`（模块含测试）。

## Sections

### NSPC — 模块名

`str`（模块全限定名）。

### STRS — 字符串池（segment-dict）

按 ASCII `.` 把每个池串切成段、去重存段字典，串本身表示为段索引序列，reader 以 `.` 拼回。消除全限定名的公共前缀重复。

```
u32           seg_count
seg_count ×   { varint seg_len; utf8 seg_bytes }        段字典（first-seen 去重）
u32           str_count
str_count ×   { varint seg_n; seg_n × varint seg_idx }  每串 = 段索引序列
```

池索引按 intern 顺序稳定；其余 section 一律用 `pool idx` 引用这里。

### TYPE — 类型描述符

`u32 class_count`，随后每类：

```
name            pool idx
base            pool idx（无基类 = 0xFFFFFFFF）
field_count     u16
每字段 ×        { name pool idx; type_tag u8; type_name pool idx;
                  attr_count u16; attr×{type_name u32, factory u32}; visibility u8 }
tp_count        u8（泛型形参数）
每 tp ×         { tp_name pool idx; constraint_flags u8;
                  [tp_ref pool idx 当 flags bit3]; iface_count u8; iface × pool idx }
attr_count      u16（类级 attribute）
attr ×          { type_name u32; factory u32 }
flags           u8（类形状，见下）
static_field_count  u16
静态字段 ×      （布局同实例字段）
interface_count u16
interface ×     pool idx
[enum 块]       仅 flags bit5 置位：member_count u16 + 每成员{name u32, value i64}
```

**类形状 flags（u8）**：`bit0` abstract、`bit1` sealed、`bit2` struct、`bit3` record、`bit4` interface、`bit5` enum、`bit6` delegate。

`visibility`：`0` public / `1` private / `2` protected。

### SIGS — 函数签名

`u32 fn_count`，随后每函数：

```
name          pool idx
param_count   u16
ret_tag       u8（type tag）
ret_name      pool idx
exec_mode     u8（0 Interp / 1 Jit / 2 Aot）
is_static     u8
visibility    u8
method_flags  u8（bit0 virtual / bit1 abstract）
min_arg       u16（必填逻辑参数数）
params_from   u8（变长参数起始逻辑下标；0xFF = 无）
每参 ×        { param_type pool idx; param_name pool idx; default_kind u8; [default payload] }
tp_count      u8（z42c 恒 0）
attr_count    u16
attr ×        { type_name u32; factory u32 }
每参 ×        { u16 attr_count; attr×{type_name u32, factory u32} }   参数级 attribute
```

`default_kind` 载荷：`2` = i64(8B)、`3` = f64 bits 以 i64 存(8B)、`4` = bool(u8)、`5` = str(pool idx)；其余无载荷。

### IMPT — 导入符号

`u32 n` + `n × pool idx`。为本模块调用到的外部函数名，按 Ordinal 排序去重。

### EXPT — 导出函数

`u32 fn_count` + 每项 `{ name pool idx; kind u8 }`（kind 恒 `0`）。

### FUNC — 函数体

`u32 fn_count`，随后每函数：

```
reg_count     u16
block_count   u16
instr_len     u32（指令字节区总长）
exc_count     u16
block_offsets u32 × block_count（各块在指令区内偏移）
异常表 ×      { try_start u16; try_end u16; catch_label u16;
                catch_type u32(pool idx, catch-all = 0xFFFFFFFF); catch_reg u16 }
instr_bytes   instr_len 字节（各块指令 + 终结符）
```

### REGT — 每寄存器类型

`u32 fn_count`，每函数 `u32 reg_count` +（reg_count>0 时）`reg_count × u8`（各寄存器 IrType）。供 JIT 直接按类型选指令。

### DBUG — 调试信息（可选）

`u32 fn_count`，每函数：`u16 line_count` + 每行 `{ blk u16; instr u16; line u32; file u32(0xFFFFFFFF=无); col u32 }`；`u16 var_count` + 每变量 `{ name pool idx; reg u16 }`。

### TIDX — 测试索引（可选）

```
magic         utf8 "TIDX"
version       u8 = 3
entry_count   u32
每条目 ×      { method_id u32; kind u8; flags u16;
                skip_reason u32; skip_platform u32; skip_feature u32; expected_throw u32;
                test_case_count u32; arg_repr u32 × test_case_count; timeout_ms i32 }
```

`kind`：1 Test / 2 Benchmark / 3 Setup / 4 Teardown。字符串字段为 1-based pool idx（`0` = 无）。

## 指令编码

每条指令与终结符以 4 字节头开始，后跟按操作码定义的操作数：

```
op        u8     操作码
type_tag  u8     结果/操作数类型标签；控制流指令为 Unknown(0x00)
dst       u16    目标寄存器；无目标 = 0xFFFF
...              额外操作数（u8 / u16 / u32 / i64）
```

调用类指令的实参列表编码为 `u8 arg_count + u16 × arg_count`（各寄存器号）。

## 操作码表

头之后的字节列在"操作数"。

| 值 | 名 | 操作数 |
|----|----|--------|
| 0x00 | ConstI | `u32`(i32) 或 `i64`(i64)，按 type_tag |
| 0x01 | ConstF | `i64`（IEEE754 bits） |
| 0x02 | ConstBool | `u8` |
| 0x03 | ConstStr | `u32` pool idx |
| 0x04 | ConstNull | — |
| 0x05 | Copy | `u16 src` |
| 0x08 | ConstChar | `u32` |
| 0x10–0x14 | Add / Sub / Mul / Div / Rem | `u16 a, u16 b` |
| 0x15 | Neg | `u16 src` |
| 0x18 | Not | `u16 src` |
| 0x19–0x1B | BitAnd / BitOr / BitXor | `u16 a, u16 b` |
| 0x1C | BitNot | `u16 src` |
| 0x1D–0x1E | Shl / Shr | `u16 a, u16 b` |
| 0x1F | ToStr | `u16 src` |
| 0x30–0x35 | Eq / Ne / Lt / Le / Gt / Ge | `u16 a, u16 b`（结果 bool） |
| 0x40 | Br | `u16 target_block` |
| 0x41 | BrCond | `u16 true_block, u16 false_block`（头 dst = cond 寄存器） |
| 0x42 | Ret | — |
| 0x43 | RetVal | —（头 dst = 返回值寄存器） |
| 0x44 | Throw | —（头 dst = 异常寄存器） |
| 0x50 | Call | `u32 method_token` + args |
| 0x51 | Builtin | `u32 name_idx` + args |
| 0x52 | VCall | `u32 method_idx, u16 obj` + args |
| 0x53 | CallNative | `u32 module, u32 type, u32 symbol` + args |
| 0x55 | LoadFn | `u32 method_token` |
| 0x56 | CallIndirect | `u16 callee` + args |
| 0x57 | MkClos | `u32 method_token, u8 stack_alloc` + args（捕获） |
| 0x60 | FieldGet | `u16 obj, u32 field_idx` |
| 0x61 | FieldSet | `u16 obj, u32 field_idx, u16 val` |
| 0x62 | StaticGet | `u32 field_idx` |
| 0x63 | StaticSet | `u32 field_idx, u16 val` |
| 0x70 | ObjNew | `u32 class_token, u32 ctor_token` + args + `u8 type_arg_count, u32 × type_arg` |
| 0x71 | IsInstance | `u16 obj, u32 class_token` |
| 0x72 | AsCast | `u16 obj, u32 class_token` |
| 0x73 | Typeof | `u32 type_name, u8 type_arg_count, u32 × type_arg` |
| 0x80 | ArrayNew | `u16 size, u8 elem_tag, u32 elem_name` |
| 0x81 | ArrayNewLit | args（元素）+ `u32 elem_name` |
| 0x82 | ArrayGet | `u16 arr, u16 idx` |
| 0x83 | ArraySet | `u16 arr, u16 idx, u16 val` |
| 0x84 | ArrayLen | `u16 arr` |
| 0x85 | StrConcat | `u16 a, u16 b` |
| 0xA0 | LoadLocalAddr | `u16 slot`（ref 参数取址） |
| 0xA1 | LoadElemAddr | `dst`, `arr:u16`, `idx:u16` — `ref arr[i]` 取址（1.43 起由 z42c 发射；VM 侧 2026-05-05 即可解码执行）|
| 0xA2 | LoadFieldAddr | `dst`, `obj:u16`, `field:u32`(STRS) — `ref obj.f` 取址（同上）|
| 0xB0 | DefaultOf | `u8 param_index`（`default(T)`） |
| 0xB1 | Convert | `u16 src`（数值转换） |

## 类型标签

指令头 `type_tag`（u8）：

```
0x00 Unknown(兼 void)   0x0A F32
0x01 Bool               0x0B F64
0x02 I8   0x03 I16      0x0C Char
0x04 I32  0x05 I64      0x0D Str
0x06 U8   0x07 U16      0x20 Object
0x08 U32  0x09 U64      0x21 Array
```

标签只占 1 字节、不内联类信息；对象的类名、数组的元素类型以独立操作数字段承载（如 `ObjNew` 的 class_token、`ArrayNew` 的 elem_name）。

## Token 编码

部分指令的符号字段以 `u32` token 编码，按范围区分本地与跨包引用：

```
本地（本模块）    [0, 0x7FFFFFFE]        = module.Functions / Classes 的插入序下标
IMPORT_BASE       0x80000000
跨包（导入）      IMPORT_BASE | pool_idx = 0x80000000 与 STRS 池索引按位或
0xFFFFFFFF        保留为未解析哨兵（正常产物不出现）
```

**tokenize 的字段**：`Call.Func`、`ObjNew.{ClassName,CtorName}`、`IsInstance.ClassName`、`AsCast.ClassName`、`LoadFn.Func`、`MkClos.Func`。其余符号字段（`VCall.Method`、`Field*/Static*.Field`、`Builtin.Name`、`CallNative.*`、`Typeof.TypeName`、`Array*.ElemName`）直接用 `pool idx`。

## 版本

**Strict-pin**：reader 仅接受 `major` 与 `minor` 与 writer 完全一致；不为旧 minor 提供兼容。每次 minor bump 后所有既存 `.zbc` 必须重新生成。

触发 **minor** bump：新增 opcode、新增 section、已有 section 字段/语义变化、flag 位语义变化。触发 **major** bump（迄今未发生）：改 magic、改 16B 头字段、改 section 目录条目格式、重划 token 编码空间。

bump 的同步 checklist 见开发基础设施部分的 version-bumping 规范。

## Minor changelog

| minor | 日期 | 触发 spec | 引入内容 |
|:-----:|------|----------|---------|
| 1.0 | 2026-05-09 | [tokenize-ir-and-zbc-bump](../../../spec/archive/2026-05-09-tokenize-ir-and-zbc-bump) | 重设结构骨架（替换 pre-1.0 sequential format）；IR 字段 tokenized via TokenAllocator (local index OR `IMPORT_BASE + STRS idx` for cross-zpkg) |
| 1.1 | 2026-05-10 | [span-column-propagate](../../../spec/archive/2026-05-10-span-column-propagate) | Line table entry 加 `u32 Column`（除 Line 外）|
| 1.2 | 2026-05-10 | [split-debug-symbols](../../../spec/archive/2026-05-11-split-debug-symbols) Phase 1 | `ZbcFlags.SymOnly` + `BLID` section（16B BLAKE3-128 build_id，always last）|
| 1.3 | 2026-05-10 | split-debug-symbols Phase 4 | `SIGS` 加 per-parameter type names（u32 strIdx × ParamCount），stack-trace signature decoration |
| 1.4 | 2026-05-11 | [add-generic-func-constraint](../../../spec/archive/2026-05-11-add-generic-func-constraint) | Constraint bundle flag 0x40 + per-param/return type-name strings (Z42FuncType signature) |
| 1.5 | 2026-05-13 | [fix-numeric-cast-lowering](../../../spec/archive/2026-05-13-fix-numeric-cast-lowering) | 新 opcode `Convert` (0xB1) 表达显式数值类型转换（替换之前 cast 为 IR no-op 的语义） |
| 1.6 | 2026-05-19 | [fix-array-default-init](../../../spec/archive/2026-05-19-fix-array-default-init) | `ArrayNew` opcode 在 `size` 之后追加 1 byte element type tag（`TypeTags::*`），驱动数组元素的 per-type 默认值（int→0 / bool→false / char→'\0' / ref→null） |
| 1.7 | 2026-05-27 | [align-zbc-reader-writer-asymmetry](../../../spec/archive/2026-05-27-align-zbc-reader-writer-asymmetry) | SIGS / TYPE 在 u8 TypeTag 之后追加 u32 type_str_idx（ret_type / field type）。Reader 优先 string 作权威类型名；tag 留作 hint。修 Read→Write byte parity；启用 ReadWriteRoundTrip CI 防线 |
| 1.8 | 2026-05-27 | [jit-type-specialization](../../../spec/archive/2026-05-28-jit-type-specialization/) P0 step 0.3/0.4 | 新 `REGT` section（`u32 fn_count` + per-fn `u32 reg_count + u8[] IrType`），承载每函数的 per-register `IrType` byte 数组。Reader 把每条解到 `Function.reg_types: Box<[IrType]>`，JIT translator 后续据此跳过 `jit_add` / `jit_eq` / `jit_and` helper、直接 emit Cranelift `iadd` / `icmp` / `band`。zpkg 0.9 同步在 packed module 加 length-prefixed `RegtData`。Pre-1.8 zbc 不可读 |
| 1.9 | 2026-05-30 | [add-test-timeout-attribute](../../../spec/archive/2026-05-30-add-test-timeout-attribute/) | TIDX section bumped to v=3：每条 `TestEntry` 在 `TestCase[]` 之后追加 `timeout_ms: i32`（`0` = 无 override，runner 用默认 300 s；正值 = `[Timeout(milliseconds: N)]` 显式 cap）。Compile-time 由 E0917 保证 `0 < N ≤ i32::MaxValue`，runtime 防御性地把负值降级回 0。zpkg 0.10 同步联动。Pre-1.9 zbc 不可读 |
| 1.10 | 2026-06-09 | [add-attribute-reflection](../../../spec/archive/2026-06-09-add-attribute-reflection/) | TYPE section 每个 class 在 type-param block 之后追加 `attr_count: u16` + `attr_count ×` (`type_name_str_idx: u32`, `factory_func_str_idx: u32`)，承载用户自定义 attribute 的 (类型名, 工厂函数名) 引用。运行期 `Type.GetCustomAttributes()` 调工厂函数构造活实例 + 缓存（C3）。Count 恒写（0=无 attribute）保证 per-class 布局统一。zpkg 0.12 同步联动。Pre-1.10 zbc 不可读 |
| 1.11 | 2026-06-09 | [add-attribute-reflection-methods](../../../spec/archive/2026-06-09-add-attribute-reflection-methods/) | SIGS section 每个 function 在 type-param block 之后追加同形的 `attr_count: u16` + (type-name, factory-func) str-idx 对，承载方法/函数级用户 attribute。运行期 `MethodInfo.GetCustomAttributes()` 据此构造活实例（C3b）。Count 恒写。ZpkgWriter 的 global SIGS 同步加同样字段。zpkg 0.13 同步联动。Pre-1.11 zbc 不可读 |
| 1.12 | 2026-06-10 | [add-reflection-type-flags](../../../spec/archive/2026-06-10-add-reflection-type-flags/) | TYPE section 每个 class 在 attr block 之后追加 `flags: u8`（bit0 abstract / bit1 sealed / bit2 struct / bit3 record），承载类修饰符。运行期载入 `TypeDesc.class_flags`，背书 `Type.IsAbstract` / `Type.IsSealed`（struct/record 位已写进 wire，将来 `IsValueType`/`IsRecord` 纯 stdlib、不再 bump 格式）。zpkg 0.14 同步联动。Pre-1.12 zbc 不可读 |
| 1.13 | 2026-06-10 | [add-reflection-static-fields](../../../spec/archive/2026-06-10-add-reflection-static-fields/) | TYPE section 每个 class 在 flags 字节之后追加静态字段块：`static_field_count: u16` + 每条 (`name: u32`, `type_tag: u8`, `type_str: u32`)，与实例字段块同形。运行期载入 `TypeDescCold.static_fields`，`Type.GetFields()` 在实例字段后追加静态字段（`FieldInfo.IsStatic = true`）。仅声明类自身静态字段（继承静态延后）。zpkg 0.15 同步联动。Pre-1.13 zbc 不可读 |
| 1.14 | 2026-06-10 | [add-field-attribute-reflection](../../../spec/archive/2026-06-10-add-field-attribute-reflection/) | TYPE section 每个字段记录（实例块 + 静态块）在 `type_str: u32` 之后追加 `attr_count: u16` + 每条 (`type_name: u32`, `factory: u32`)，承载字段级用户 attribute 引用，与 class/method attr 同形。运行期索引进 `TypeDescCold.field_attributes`，`FieldInfo.GetCustomAttributes()` 调工厂构造活实例。zpkg 0.16 同步联动。Pre-1.14 zbc 不可读 |
| 1.15 | 2026-06-10 | [add-parameter-attribute-reflection](../../../spec/archive/2026-06-10-add-parameter-attribute-reflection/) | SIGS section 每个函数记录在方法级 `attr_count` 块之后追加**每参数 attr-ref 块**——对 `param_count` 个参数（含实例方法的隐式 `this` 槽，恒空）各写 `attr_count: u16` + (`type_name: u32`, `factory: u32`) 对偶。运行期载入 `FunctionCold.param_attributes`（SIGS 对齐），`ParameterInfo.GetCustomAttributes()` 按源参数位置（= wire 索引 − this 偏移）取并调工厂。zpkg 0.17 同步联动。Pre-1.15 zbc 不可读 |
| 1.16 | 2026-06-12 | [add-reflection-array-element-type](../../../spec/archive/2026-06-14-add-reflection-array-element-type/) | `ArrayNew` opcode 在 element type tag 之后、`ArrayNewLit` 在 elem args 之后各追加 `element_type: u32`（STRS idx，元素类型 FQ 名，如 `int` / `geometry.Point` / `int[]`）。运行期数组值改由 `ArrayObj { element_type, elems }` 承载（不再类型擦除），`Value::Array(GcRef<ArrayObj>)`。背书 `Type.IsArray` / `Type.GetElementType()` 与非擦除的 `arr.GetType()`。zpkg 0.18 同步联动。Pre-1.16 zbc 不可读 |
| 1.17 | 2026-06-14 | [add-reflection-get-interfaces](../../../spec/archive/2026-06-14-add-reflection-get-interfaces/) | TYPE section 每个 class 在静态字段块之后追加**接口块**：`interface_count: u16` + `interface_name_idx[]: u32`（类直接声明的接口名，bare）。运行期载入 `TypeDescCold.interfaces`，`Type.GetInterfaces()`（`__type_interfaces` builtin）沿 base 链聚合本类 + 继承接口（按名 dedup）。Count 恒写（0=无接口）。传递接口实现（interface-extends-interface）延后。zpkg 0.19 同步联动。Pre-1.17 zbc 不可读 |
| 1.18 | 2026-06-16 | [add-reflection-generic-type-definition](../../../spec/archive/2026-06-16-add-reflection-generic-type-definition/) | 新 `Typeof` opcode（`0x73`）：`dst` + `TypeName: u32`（STRS idx，定义 FQ 名）+ `type_arg_count: u8` + `type_arg_idx[]: u32`（实例化 arg FQ 名，镜像 `ObjNew` type_args 编码）。所有 `typeof(...)` 统一 emit 它，移除 `__typeof` builtin。背书 `Type.IsGenericTypeDefinition` / `GetGenericTypeDefinition()` + 修 `typeof(Box<int>).GetGenericArguments()`。zpkg 0.20 同步联动。Pre-1.18 zbc 不可读 |
| 1.19 | 2026-06-16 | [add-reflection-interface-class-predicates](../../../spec/archive/2026-06-16-add-reflection-interface-class-predicates/) | **interface 现在 emit 一条最小 TYPE 条目**（identity + flags，无 base/字段/方法表），故 `typeof(IFoo)` 解析到真句柄。`class_flags` 字节扩 **bit4 = interface**（bit0-3 不变；bit5 预留 enum）。背书 `Type.IsInterface` + 把接口排除出 `Type.IsClass`。无新字段，仅 flags 语义扩展 + TYPE section 多接口条目。zpkg 0.21 同步联动。Pre-1.19 zbc 不可读 |
| 1.20 | 2026-06-16 | [add-reflection-assignable-from](../../../spec/archive/2026-06-16-add-reflection-assignable-from/) | TYPE section 每类的**接口块改存 FQ 名**（`Demo.IShape`，此前 bare `IShape`）。结构不变（`u16 count + str idx[]`），仅字段语义 bare→FQ。`GetInterfaces()` 据此解析到**真接口句柄**；`x is IShape`/`as`/`Type.IsAssignableFrom` 按 FQ 名做 robust 接口身份比较。zpkg 0.22 同步联动。Pre-1.20 zbc 不可读 |
| 1.21 | 2026-07-09 | [reencode-strs-segment-dict](../../../spec/archive/2026-07-09-reencode-strs-segment-dict/) | **STRS 段重编码为 segment-dict**：`seg_count + 段字典(varint len + utf8)×seg_count + str_count + (varint seg_n + varint seg_idx×seg_n)×str_count`。按 `.` 切分去重 namespace 段，串=段索引序列（reader `join('.')` 无损还原）。消除旧编码的可推导 offset 冗余 + FQ 名前缀重复（z42.core STRS −44%）。**串索引不变** → 其它段零改动。zpkg 0.25 同步联动。Pre-1.21 zbc 不可读 |
| 1.22 | 2026-07-09 | [add-enum-type-metadata](../../../spec/archive/2026-07-09-add-enum-type-metadata/)（unify-type-metadata P1-a） | **enum 作 TYPE 段类型实体**：`class_flags` 新增 **bit5=enum**；置位时类记录尾部追加 enum 成员块 `member_count:u16 + (name_idx:u32, value:i64)×n`（gated → 非 enum 类记录字节不变）。背书 `Type.IsEnum` + `Std.Enum.GetNames/GetValues/GetName` + `typeof(EnumType)`。unify-type-metadata 首砖（enum 成员值有了 TYPE 的家；TSIG enum 块 P1 仍并存，P3 删）。zpkg 0.26 同步联动。Pre-1.22 zbc 不可读 |
| 1.23 | 2026-07-10 | [add-member-visibility](../../../spec/archive/2026-07-10-add-member-visibility/)（unify-type-metadata P1-b） | **成员可见性入 TYPE/SIGS**：TYPE 段每字段块（实例 + 静态）在 attrs 之后追加 `visibility:u8`；SIGS 段每函数在 `is_static` 之后追加 `visibility:u8`（0=public / 1=private / 2=protected；non-gated → 每成员固定 +1 byte）。背书 `FieldInfo.IsPublic/IsPrivate` + `MethodInfo.IsPublic/IsPrivate`（reader 把 SIGS visibility 灌进 `Function.visibility`、TYPE visibility 灌进 `FieldSlot`/`FieldDesc`）。unify-type-metadata 第二砖（TSIG 可见性字段有了 TYPE/SIGS 的家；P3 删 TSIG）。zpkg 0.27 同步联动。Pre-1.23 zbc 不可读 |
| 1.24 | 2026-07-10 | [add-method-modifiers](../../../spec/archive/2026-07-10-add-method-modifiers/)（unify-type-metadata P1-c） | **方法修饰符入 SIGS**：SIGS 段每函数在 `visibility` 之后追加 `method_flags:u8`（bit0=virtual / bit1=abstract；`static` 仍由既有 `is_static` 字节表达，不重复；non-gated → 每函数固定 +1 byte）。背书 `MethodInfo.IsVirtual`（**权威化**——从 vtable-presence 启发式改读 flag，virtual/override/abstract 皆置 bit0，镜像 C#）+ 新增 `IsAbstract`（bit1）。reader 把 SIGS method_flags 灌进 `Function.method_flags`（与 `is_static`/`visibility` 同源同路径）。unify-type-metadata 第三砖。zpkg 0.28 同步联动。Pre-1.24 zbc 不可读 |
| 1.25 | 2026-07-10 | [add-param-metadata](../../../spec/archive/2026-07-10-add-param-metadata/)（unify-type-metadata P1-d） | **参数元数据入 SIGS**：每函数在 `method_flags` 后追加 `min_arg:u16`（必填逻辑参数个数）+ `params_from:u8`（varargs 逻辑 index，0xFF=无）；每参数在 `param_type` 后追加 `name_str_idx:u32`（源名，this 槽="this"）+ `default_kind:u8 + payload`（0=无/1=null/2=i64 8B/3=f64bits 8B/4=bool 1B/5=str u32 idx；字面量折叠，非字面量默认值 kind=0）。背书 `ParameterInfo.IsOptional/IsParams/DefaultValue` + `Name` **权威化**（SIGS 优先，DBUG 回退）。unify-type-metadata 第四砖。zpkg 0.29 同步联动。Pre-1.25 zbc 不可读 |
| 1.26 | 2026-07-11 | [add-delegate-metadata](../../../spec/archive/2026-07-11-add-delegate-metadata/)（unify-type-metadata P1-e ②） | **delegate-as-class**：`class_flags` 新增 **bit6=delegate**（无额外 payload——沿 1.19 interface「flags 语义扩展 + 新增条目」先例 bump）。每 `delegate` 声明（含泛型）emit 一条 TYPE 条目（FQ 名 + bit6 + TypeParams——泛型 tps 存 TYPE，Invoke 按名引用）+ 合成 `<FQ>.Invoke` 死体桩进 SIGS/FUNC（实例/virtual/参数源拼写+名+P1-d 元数据；真实调用走 CallIndirect，桩永不被调）。背书 `Type.IsDelegate` + Invoke 签名反射；P3 删 TSIG 的 delegate 表前置。zpkg 0.30 同步联动。Pre-1.26 zbc 不可读 |
| 1.27 | 2026-07-14 | [stabilize-dispatch-keys](../../../spec/changes/stabilize-dispatch-keys)（方案A） | **无 wire-layout 变化**：派发键从「兄弟集相关」改为「方法自身签名纯函数」（`regName` 一律全签名 mangle，协议豁免名 `ToString`/`Equals`/… 保持裸名）→ `CallInstr`/`VCall` 操作数字符串 + SIGS/导出方法名 **全局重键**（内容变、布局不变）。VM vtable 槽键随之保留 `$` 后缀（`derive_simple_method_name` 不再截断）→ VCall 与 vtable 一致 + 重载虚方法各占独立槽。bump 仅为触发 ci-bootstrap 两代自举整树重键（耦合 zpkg 0.32）。Pre-1.27 zbc 不可读 |
| 1.28 | 2026-07-18 | [fix-crosspkg-interface-impl](../../../spec/changes/fix-crosspkg-interface-impl) | **接口 TYPE 条目尾部加方法签名块**（`class_flags` bit4=interface gated：`mcount:u16 + (name_idx:u32, ret_idx:u32, pcount:u8, ptype_idx:u32×pcount)×n`，同 enum 块模式）——跨包接口实现（`class X : IFace@其他包`）须能从依赖 zpkg 恢复接口方法名/签名（抽象接口方法无 body 不入 SIGS、EXPT 已删 → TYPE 是唯一载体；`TsigReconcile._rebuildInterface` 消费）。VM 侧 parse-and-discard（接口派发走 vtable）。耦合 zpkg 0.33。Pre-1.28 zbc 不可读 |
| 1.29 | 2026-08-05 | [add-escape-analysis-stack-alloc](../../../spec/archive/2026-08-05-add-escape-analysis-stack-alloc/) | **`ObjNew`/`ArrayNew`/`ArrayNewLit` 编码尾部各加 1 个 `u8` 栈分配标志**（`1`=帧 arena 栈分配 / `0`=堆；镜像 `MkClos.stack_alloc` 先例）。由编译期逃逸分析 pass 置位；interp 据此在 per-context 栈 arena 分配、绕过 GC（JIT 忽略 flag、照常堆分配）。耦合 zpkg 0.34。Pre-1.29 zbc 不可读 |
| 1.30 | 2026-08-07 | [impl-sealed-semantics](../../../spec/changes/impl-sealed-semantics) | **SIGS `method_flags:u8` 新增 bit2=sealed**（`METHOD_FLAG_SEALED`；`sealed override` / 简写 `sealed` 方法置位，且必连带 bit0=virtual）。**字节布局不变**——bit2 先前保留为 0；strict-pin 下仍 bump 以防「同版本号、不同 bit2 语义」的静默分歧（跨 nightly 种子链）。背书 `MethodInfo.IsSealed` + 编译期 sealed-receiver 去虚化（follow-up）。耦合 zpkg 0.35。Pre-1.30 zbc 不可读 |
| 1.31 | 2026-08-09 | [add-struct-value-semantics](../../../spec/archive/2026-08-09-add-struct-value-semantics/) | **TYPE section 尾部新增值 struct 布局块**（Flags bit2=struct gated）：`size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n` = 带种类引用位图，供运行时 GC 定位 blob 内引用叶子 + StructCopy 逐叶子 clone（字段 offset/size 由 codegen 烘焙进访问指令、不经此块）。同时 **z42c 开始 emit** blob 值指令 `StructAlloc/Copy/FieldGetPrim/SetPrim`（opcode 0xC0–0xC3，A-support 已加编解码/执行）。耦合 zpkg 0.36。Pre-1.31 zbc 不可读 |
| 1.32 | 2026-08-11 | [add-struct-heap-inline](../../../spec/archive/2026-08-11-add-struct-heap-inline/) | **TYPE section 新增合成内联 struct 布局块**（`CLASS_FLAG_HAS_INLINE_STRUCT` bit7=0x80 gated，紧随 struct 块）：`size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n` = class 内联 struct 字段的**对象相对**字节区大小 + 引用位图，供 `ScriptObject` alloc（`struct_bytes`/`struct_refs` 侧表）+ 内联字段访问定位引用叶子。同 struct 块 shape（reader 复用 `StructLayoutDesc`）。背书 `class C { Point pt; }` 字节内联（D1-a：基元字节内联 + 引用叶子侧表）。耦合 zpkg 0.37。Pre-1.32 zbc 不可读 |
| 1.33 | 2026-08-13 | [enforce-class-access](../../../spec/archive/2026-08-13-enforce-class-access/) | **TYPE section 新增类声明可见性字节**（紧随 `class_flags` u8）：`visibility:u8`（0=public/1=private/2=protected/3=internal），**每条 TYPE 记录 +1 字节**（无 flag gate，恒存在）。跨包 `internal` 类引用强制的元数据载体——`TsigReconcile` 从此字节还原 `ExportedClassZ.Visibility`，`ImportedSymbolLoader` 填 `Z42ClassType.Visibility`，`AccessChecker.CheckTypeRef` 据此对 imported internal 类的跨包引用 emit E0404。VM 侧 **read-and-discard**（暂无类可见性反射面）。耦合 zpkg 0.38。Pre-1.33 zbc 不可读 |
| 1.34 | 2026-08-14 | [unify-object-byte-layout](../../../spec/archive/2026-08-15-unify-object-byte-layout/)（PR-1） | **TYPE section 新增对象全字段布局块**（普通引用类=`class_flags` 不含 struct/interface/enum/delegate=116 的**派生谓词** gated，随 inline 块之后——class flags U8 满位无空闲，故用派生谓词而非新标志位，writer/reader 同谓词锁步）：`ObjectSize:u32 + field_count:u16 + (off:u32, size:u32, kind:u8)×n + ref_count:u16 + (ref_off:u32, ref_kind:u8)×m` = 每直接字段对象相对 offset/size/kind（供 FieldGet/反射）+ 扁平 8B 引用位图（含内联 struct 内部叶子，供 GC）。**引用叶子按 8B 裸指针宽度**（C# 等价终点）。**PR-1 休眠**：`TypeDescCold.object_layout` 携带但 runtime 不消费（仍走 slots）；PR-2 切字节存储时替换 slots。OWN 字段 offset 从 0，继承 base-shift 由消费方组合。耦合 zpkg 0.39。Pre-1.34 zbc 不可读 |
| 1.35 | 2026-08-14 | [unify-object-byte-layout](../../../spec/archive/2026-08-15-unify-object-byte-layout/)（PR-3 chunk 2a） | **对象块直接字段 `field_kinds` 细化**：粗粒度 `GcRef`（=2，原含 object/array/interface/delegate/func）细分为 `GcRefArray`（=4，数组 `T[]`→运行时 `Value::Array`）与 `GcRefClosure`（=5，delegate/func/未解析→运行时 `Value::Closure`/`FuncRef`，**非 GcRef**）；object/interface 仍 `GcRef`（=2）。编译期 `StructLayout._refineDirectRefKind` 判定（**保守**：只把确定 object/array 标可内联，其余落 closure 侧表默认，false-negative 只次优不 UB）。供 chunk 2b 把 object/array 引用安全内联为 8B 裸指针、closure/string 留侧表。**仅对象块直接字段 `field_kinds` 字节变化**——`ObjectSize`/字段 offset/size/**引用位图 `ref_kinds`（仍粗粒度）**/struct 块全字节不变。**runtime 休眠**：`compose_object_layout` 把 4/5 映射回粗粒度 GcRef 侧表路径（行为不变）；chunk 2b 消费。耦合 zpkg 0.40。Pre-1.35 zbc 不可读 |
| 1.36 | 2026-08-21 | [add-generic-methods](../../../spec/changes/add-generic-methods) | **方法级泛型 type_args**（M1）。4 个新 opcode：`MethodTypeArg`(0xB2，物化方法级形参为具体 `Std.Type`，读 `frame.method_type_args[param_index]`)、`MethodDefault`(0xB3，方法级 `default(T)` 零值，镜像 `DefaultOf` 但读 frame 槽)、`CallGeneric`(0xB4)、`VCallGeneric`(0xB5)。`CallGeneric`/`VCallGeneric` = 常规 `Call`/`VCall` 编码在 method-token/obj 之后、args 之前插一段 `method_type_args`（`count:u16 + (str idx:u32)×count`）。**非泛型 `Call`/`VCall`（0x50/0x52）编码不变 → 全仓自举 byte-identical**；仅携类型实参的调用才发 Generic 变体。耦合 zpkg 0.41。Pre-1.36 zbc 不可读 |
| 1.37 | 2026-09-02 | [fix-generic-array-value-zero-init](../../../spec/archive/2026-09-02-fix-generic-array-value-zero-init/) | **泛型数组值类型零初始化**（方案 C）。`ArrayNew` 编码在 escape-analysis stack-alloc u8 之后追加 `type_param_kind:u8`（0=none/1=method 级/2=class 级）+ `(type_param_index+1):u16`（偏置，-1/none→0）。元素为泛型形参时携带其类型参数引用，VM 运行期查 `frame.method_type_args[idx]`/接收者 `type_args[idx]` 解析出具体类型，把值类型 T 的数组槽初始化为该类型的零值（0/false/'\0'/零布局）而非 `Null`，并把 `ArrayObj.element_type` 设为具体类型（顺带修正泛型数组反射元素类型）。**非泛型 `ArrayNew`（kind=0/index=-1，尾字节 `00 00 00`）语义不变、方法体不含泛型数组则 byte-identical**。`ArrayNewLit`（`new T[]{…}`）不变（槽全被字面量写满，无 null-tail bug）。耦合 zpkg 0.42。Pre-1.37 zbc 不可读 |
| 1.38 | 2026-09-03 | [stabilize-instance-dispatch-keys](../../../spec/archive) | **实例/静态虚方法派发键**改为 primary(裸名) / 非-primary(全签名 mangle)。**wire 布局不变，仅键字符串内容变** → 加重载从此是纯增量、不再扰动既有键。bump 触发 ci-bootstrap 版本差 gate → 两代自举重键。耦合 zpkg 0.43。（**本行系 2026-09-10 unify-type-identity-fqn 补录** —— 该次 bump 漏写 changelog，与 version-bumping.md 里记的「1.37→1.38 漏 fixture」同源）|
| 1.39 | 2026-09-13 | [encode-ctorless-objnew](../../../spec/archive/2026-09-13-encode-ctorless-objnew) | **`ObjNew` 尾部追加 `ctor_known:u8`**（在 escape-analysis 的 `stack_alloc` u8 之后）。**正向**标记：编译器在**整包装配之后**（`CtorKnownFixup`，那是本包全部 `IrModule` 第一次同时在手的时刻）确认该 ctor 名出现在「本包全部已发射函数 ∪ `DependencyIndex`」里，才置 1。运行期据此把「构造器本该在、却全路径解析不到」（依赖包版本 skew ⇒ 抛 `MissingSymbolException`）与「这个类本来就没有构造器」（照常零初始化）分开 —— 此前只能靠 `argc > 0` 近似，零实参站点是公开缺口。**位的缺席是保守态**：未置位时行为与 1.38 逐字一致，绝不会误判成静默跳过真构造器。耦合 zpkg 0.44 |
| 1.40 | 2026-09-14 | [fix-call-arity-skew](../../../spec/archive/2026-09-14-fix-call-arity-skew) | **SIGS `method_flags` 新增 bit3 = `METHOD_FLAG_SRET`**：函数返回 blob 值 struct ⇒ 物理签名末尾有隐藏返回槽（sret），它**不计入** `param_count`（为了不污染反射/跨包签名）。此前这个事实从没进过元数据，运行时拿到的 `param_count` 少报了一，无法精确判定「调用的实参数与被调方签名是否对得上」。现在由 `FunctionEmitter` 在 `RetIsStruct` 时置位，运行时判据 = `phys == param_count + sret`（`params` 变长无上界），在首次绑定点拒绝 primary 裸键下的错签名派发。**wire 布局不变，仅字段语义变**——旧产物该位恒 0，会被新判据误判，故必须 bump。耦合 zpkg 0.45 |
| 1.41 | 2026-09-15 | [fix-imported-iface-static-fidelity](../../../spec/archive/2026-09-16-fix-imported-iface-static-fidelity/) | **接口方法块新增 `is_static:u8`**（TYPE 段 `CLASS_FLAG_INTERFACE` bit4 gated 的接口方法块，每方法在 `pcount` 之后、`ptype`s 之前）。镜像 SIGS 早有的 `is_static` 专用字节。此前接口方法块只携 `name/ret/pcount/ptypes`、**零 flags 字节** ⇒ `static abstract` 接口成员的静态位一路丢成 false（`TsigReconcile._rebuildInterface` 硬编码 `isStatic=false`），迫使 `InheritanceResolver` 用 `if (it.IsImported) return;` 临时守卫跳过导入接口的 static 满足性校验（#636）。承载真值后：`TsigReconcile` 用 wire 值构造 `ExportedMethodZ`（`isVirtual=!isStatic`、`isAbstract=true` 派生）、导入侧 `mz.IsStatic` 恢复真值、删守卫 ⇒ 导入接口获完整 static/可见性/返回满足性校验。VM 侧 **read-and-consume**（保游标对齐；vtable 派发不需要它，`IfaceMethodSig.is_static` 供未来 `MethodInfo.IsStatic` 反射）。耦合 zpkg 0.46。Pre-1.41 zbc 不可读 |
| 1.42 | 2026-09-16 | [assoc-type-crosspkg](../../../spec/archive/2026-09-16-assoc-type-crosspkg/) | **跨包关联类型**（associated types across zpkg）：两处 wire 新增，承载此前一个字节没进 wire 的三份数据。① **约束 bundle 新增 bit7 = `has_assoc_binding`**（`assoc_count:u8 + (name_idx:u32, type_idx:u32)×n`，在 iface 列表之后）——承载 `where T:IEnum<Item=int>` 的绑定要求。② **每条 TYPE 记录尾部新增 always-present 统一 assoc 块**（`assoc_count:u16 + (name_idx:u32, type_idx:u32)×n`）——接口写 `(Item, "")` = 声明的关联类型名单、类写 `(Item, int)` = 类侧绑定；消费端按 `class_flags & CLASS_FLAG_INTERFACE` 路由。此前接口关联类型名单（`Z42InterfaceType.AssocTypeNames`）+ 类侧绑定（`Z42ClassType.AssocBinding`）+ 约束绑定（`ConstraintBundle.AssocBinding`）**全无 wire 承载**，跨包 100% 丢失，迫使 `ConstraintChecker`/`InheritanceResolver` 三处 `IsImported` 守卫保守跳过。承载后：`TsigReconcile`/`ImportedSymbolLoader` 恢复三份数据、删三守卫 ⇒ 跨包关联类型获与同包一致的完整校验（正确绑定放行、错/缺绑定 E0453）。关联类型是**纯编译期**概念，VM 侧两处新载荷均 **read-and-consume**（`validate_type_arg_constraint` 无关联类型分支）。耦合 zpkg 0.47。Pre-1.42 zbc 不可读 |
| 1.43 | 2026-09-17 | [fix-ref-lvalue-addressing](../../../spec/changes/fix-silent-semantic-gaps/) | **z42c 开始发射 `LoadElemAddr`(0xA1) / `LoadFieldAddr`(0xA2)** —— `ref arr[i]` / `ref obj.f` 的取址。⚠️ 注意这是 **A-use 而非 A-support**：VM 侧这两条的解码（`zbc_reader/opcodes.rs`）与执行（`interp/exec_address.rs`）**自 2026-05-05 `cb61cc072` 就完整存在**，四个多月来一直是纯死代码（零测试、零生产者）。此前编译器对**任何** ref 实参一律发 `LoadLocalAddr`(0xA0)，而对数组元素 / 对象字段，`Emit(inner)` 是一次**读取**、返回新分配的**临时寄存器** ⇒ 取的是临时槽的地址 ⇒ 出口 copy-out 写回临时槽、调用方看不到（**静默丢写**：`Inc(ref arr[0])` 打印 10 而非 11）。按本页既有纪律「A-support 不 bump、**bump 在 A-use**」（同 1.31 blob 值指令）。wire 布局：`LoadElemAddr` = `dst + arr:u16 + idx:u16`（同二元算术形），`LoadFieldAddr` = `dst + obj:u16 + field:u32`（同 `FieldGet` 形）——两者**布局早由 VM 解码器钉死**，本次只是补上生产方。耦合 zpkg 0.48 |

> **如何 bump minor**：见 [`version-bumping.md` §"Bumping `.zbc` minor version"](../../../agent/rules/version-bumping.md#bumping-zbc-minor-versionfreeze-zbc-v1-2026-05-14)。简而言之 — 写 `ZbcWriter.VersionMinor++` + 同步 `zbc_reader.rs` 常量 + 本表加一行 + `xtask build test` regen（原地重生 6 个 zbc-format fixture）+ commit。Invariant CI 校验三方常量一致。

### Token 编码（v1.0+）

Tokenizable IR 字段（`Call.func` / `LoadFn.func` / `LoadFnCached.func` /
`MkClos.fn_name` / `ObjNew.{class_name, ctor_name}` / `IsInstance.class_name` /
`AsCast.class_name`）以 u32 token 写入，编码语义：

```
intra-module:    [0,             0x7FFF_FFFE]   token = local index
                                                  (Functions / Classes 索引)
IMPORT_BASE:     0x8000_0000
cross-zpkg:      [0x8000_0000,   0xFFFF_FFFE]   token - IMPORT_BASE = STRS idx
UNRESOLVED:      0xFFFF_FFFF                    占位 / 错误状态
```

Decoder（C# `ZbcReader.IdMap` / Rust `metadata::zbc_reader::IdMap`）按 token
范围分发：本地 token → `local_funcs[token]` 或 `local_classes[token]`；
import token → `pool[token - IMPORT_BASE]`；UNRESOLVED → `<unresolved>` 诊断。

Cross-zpkg 引用直接复用 STRS 池，**不引入新 IMPT entry 格式**；IMPT 区段保持
v0.x 的 namespace 提取语义（用于 lazy zpkg 路由）。

**不 tokenize** 的字段：`BuiltinInstr.Name`（closed set runtime 解析）、
`VCallInstr.Method`（receiver-type-dependent，IC 路径）、
`FieldGet/Set.FieldName` / `LoadFieldAddr.FieldName`（同上）、
`StaticGet/Set.Field`（runtime lazy 全局编号）、
`CallNative*.{Module,TypeName,Symbol}`（native interop separate concern）。
这些字段继续以 STRS 池 idx 编码。

---

> 本表自 `docs/internals/src/formats/zbc.md` 迁入（批 2）。
> **每次格式 bump 必须在此加一行** —— 见 [version-bumping.md](../../../agent/rules/version-bumping.md) 第 3 步。
