# zbc 字节码格式

> **页型**: 参考页 ｜ **状态**: ✅ 已实现（v1.27）｜ **代码**: `src/compiler/z42c.ir/src/BinaryFormat/`（`ZbcFormat.z42` / `ZbcWriter.z42` / `ZbcInstr.z42`）
> **相关**: [源代码编译流程](source-compile.md) · [zpkg 包格式](zpkg-format.md) ｜ **对齐**: 2026-07-19

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
