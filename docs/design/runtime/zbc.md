# zbc 二进制格式规范

> ⚠️ **已迁移（2026-07-19）**：权威格式参考已移至 book —— [`docs/book/src/compiler/zbc-format.md`](../../book/src/compiler/zbc-format.md)（与当前 z42c 源码逐字节对齐）。本页保留 minor changelog 等历史，不再更新格式细节。

## 设计目标

1. **二进制 + 文本双形态**：`.zbc` 二进制与 `.zasm` 文本一一对应，可互转，不丢信息
2. **解释器直接执行**：指令流即执行流，无须二次转换，解释器 fetch-decode-dispatch 即可跑
3. **JIT 直接翻译**：每条指令携带类型信息，JIT 无须重新分析类型即可生成带类型的 CLIF
4. **SSA Block 参数**（非 phi 节点）：分支携带实参，解释器传参、JIT 建立 CLIF block 参数均自然映射

---

## 核心设计决策

### 用 Block 参数替代 Phi 节点

传统 SSA phi 节点在解释器中处理笨拙（需要"在块入口提前求值"）。
zbc 采用 **Cranelift / MLIR 风格的 Block 参数**：

```
; 旧风格（phi 节点）
loop:
  %i = phi [entry: %i0] [loop: %i_next]

; zbc 风格（block 参数）
loop(%i: i32):
  ...
  br loop(%i_next)          ; 传递下一迭代的值
```

- **解释器**：`br block(args...)` 时把 args 写入目标 block 的参数寄存器，然后跳转
- **JIT**：Cranelift block 天然支持 block 参数，1:1 映射，无需任何转换

### 寄存器索引（u16）

每个函数最多 65 535 个虚拟寄存器，覆盖所有实际场景。
参数寄存器 = 前 N 个（`%0..%N-1`），其余为计算寄存器。

### 指令携带类型标签

每条指令 header 含 `type: u8`，JIT 直接按类型选择 CLIF 操作，无需类型传播 pass。

---

## 文本格式（`.zasm`）

文本格式是二进制的可读投影，可由 `z42c --dump-asm` 生成，也可被 `z42c --assemble` 读回二进制。

```asm
.module "Demo.Greet"  version 0.1.0

.strings
  s0  "Hello, "
  s1  "world"

; --- 函数定义 ---
.func @greet  exported
  .params  (str)          ; %0 = name: str
  .returns str
  .regs    3

  .block entry
    %1:str = const.str  s0
    %2:str = call @str.concat  %1, %0
    ret  %2

.func @main  exported
  .params  ()
  .returns void
  .regs    3

  .block entry
    %0:str = const.str  s1
    %1:str = call @greet  %0
    call.void @io.println  %1
    ret
```

**文本语法规则**：
- 寄存器：`%N`（N 为十进制索引）
- 类型标注：`%dst:type = opcode ...`（dst 无类型时写 `-`）
- 字符串引用：`s<idx>`（引用 `.strings` 池）
- 函数引用：`@fully.qualified.name`
- 块标签：`label:` 加 block 参数 `label(%p0:type, %p1:type):`
- 跳转带参：`br label(%a, %b)`

---

## 类型标签（TypeTag, u8）

```
0x00  void
0x01  bool
0x02  i8     0x03  i16     0x04  i32     0x05  i64
0x06  u8     0x07  u16     0x08  u32     0x09  u64
0x0A  f32    0x0B  f64
0x0C  char
0x0D  str                      ; GC 管理的 UTF-8 字符串
0x10  object  (+ u16 class_idx) ; GC 管理的类实例
0x11  array   (+ u8 elem_type)  ; GC 管理的数组
0x12  struct  (+ u16 type_idx)
0x13  tuple   (+ u8 arity)
0x14  enum    (+ u16 type_idx)
0x1F  ptr     (+ u8 elem_type)  ; raw pointer（unsafe）
```

扩展类型（`0x10`—`0x1F`）在指令流中以 **类型描述词** 形式内联：首字节为标签，后续字节为参数。

---

## 操作码表（Opcode, u8）

### 常量（0x00–0x0F）

| 操作码 | 助记符     | 操作数                       | 说明 |
|--------|------------|------------------------------|------|
| 0x00   | const.i    | dst:u16, value:i64           | 整数常量（按 type 截断） |
| 0x01   | const.f    | dst:u16, value:f64           | 浮点常量 |
| 0x02   | const.bool | dst:u16, value:u8            | true=1 / false=0 |
| 0x03   | const.str  | dst:u16, str_idx:u32         | 字符串池引用 |
| 0x04   | const.null | dst:u16                      | null（与 type 一起使用） |

### 算术与逻辑（0x10–0x2F）

| 操作码 | 助记符   | 操作数            |
|--------|----------|-------------------|
| 0x10   | add      | dst, a, b         |
| 0x11   | sub      | dst, a, b         |
| 0x12   | mul      | dst, a, b         |
| 0x13   | div      | dst, a, b         |
| 0x14   | rem      | dst, a, b         |
| 0x15   | neg      | dst, a            |
| 0x16   | and      | dst, a, b         |
| 0x17   | or       | dst, a, b         |
| 0x18   | not      | dst, a            |
| 0x19   | bit_and  | dst, a, b         |
| 0x1A   | bit_or   | dst, a, b         |
| 0x1B   | bit_xor  | dst, a, b         |
| 0x1C   | bit_not  | dst, a            |
| 0x1D   | shl      | dst, a, b         |
| 0x1E   | shr      | dst, a, b         |

### 比较（0x30–0x3F，结果类型始终 bool）

| 操作码 | 助记符 | 操作数      |
|--------|--------|-------------|
| 0x30   | eq     | dst, a, b   |
| 0x31   | ne     | dst, a, b   |
| 0x32   | lt     | dst, a, b   |
| 0x33   | le     | dst, a, b   |
| 0x34   | gt     | dst, a, b   |
| 0x35   | ge     | dst, a, b   |

### 控制流（0x40–0x4F）

| 操作码 | 助记符    | 操作数                                               | 说明 |
|--------|-----------|------------------------------------------------------|------|
| 0x40   | br        | block_idx:u16, argc:u8, args:u16[]                  | 无条件跳转 + 传参 |
| 0x41   | br.cond   | cond:u16, t_idx:u16, t_argc:u8, t_args:u16[], f_idx:u16, f_argc:u8, f_args:u16[] | 条件跳转 |
| 0x42   | ret       | —                                                    | void 返回 |
| 0x43   | ret.val   | val:u16                                              | 值返回 |
| 0x44   | throw     | val:u16                                              | 抛出异常 |

### 调用（0x50–0x5F）

| 操作码 | 助记符     | 操作数                                | 说明 |
|--------|------------|---------------------------------------|------|
| 0x50   | call       | dst:u16, fn_idx:u32, argc:u8, args:u16[] | 静态调用 |
| 0x51   | call.void  | fn_idx:u32, argc:u8, args:u16[]       | 无返回值调用 |
| 0x52   | call.virt  | dst:u16, method_str:u32, recv:u16, argc:u8, args:u16[] | 虚方法调用 |
| 0x53   | call.async | dst:u16, fn_idx:u32, argc:u8, args:u16[] | async 调用，返回 task |
| 0x54   | await      | dst:u16, task:u16                     | 等待 task |

### 字段访问（0x60–0x6F）

| 操作码 | 助记符     | 操作数                          |
|--------|------------|---------------------------------|
| 0x60   | field.get  | dst:u16, obj:u16, field_idx:u16 |
| 0x61   | field.set  | obj:u16, field_idx:u16, val:u16 |
| 0x62   | static.get | dst:u16, field_str:u32          |
| 0x63   | static.set | field_str:u32, val:u16          |

### 对象（0x70–0x7F）

| 操作码 | 助记符      | 操作数                                    |
|--------|-------------|-------------------------------------------|
| 0x70   | obj.new     | dst:u16, class_idx:u16, argc:u8, args:u16[] |
| 0x71   | obj.is      | dst:u16, obj:u16, class_idx:u16           |
| 0x72   | obj.as      | dst:u16, obj:u16, class_idx:u16           |

### 数组（0x80–0x8F）

| 操作码 | 助记符    | 操作数                       |
|--------|-----------|------------------------------|
| 0x80   | arr.new   | dst:u16, len:u16             |
| 0x81   | arr.get   | dst:u16, arr:u16, idx:u16    |
| 0x82   | arr.set   | arr:u16, idx:u16, val:u16    |
| 0x83   | arr.len   | dst:u16, arr:u16             |

### Struct / Tuple（0x90–0x9F）

| 操作码 | 助记符      | 操作数                                    |
|--------|-------------|-------------------------------------------|
| 0x90   | struct.new  | dst:u16, type_idx:u16, argc:u8, args:u16[] |
| 0x91   | struct.get  | dst:u16, obj:u16, field_idx:u16           |
| 0x92   | struct.set  | obj:u16, field_idx:u16, val:u16           |
| 0x93   | tuple.new   | dst:u16, argc:u8, args:u16[]              |
| 0x94   | tuple.get   | dst:u16, tup:u16, idx:u8                  |

### Variant / Enum（0xA0–0xAF）

| 操作码 | 助记符       | 操作数                                        |
|--------|--------------|-----------------------------------------------|
| 0xA0   | var.new      | dst:u16, type_idx:u16, variant_idx:u8, payload:u16 |
| 0xA1   | var.tag      | dst:u16, val:u16                              |
| 0xA2   | var.cast     | dst:u16, val:u16, variant_idx:u8              |

---

## 指令编码布局

每条指令以 **4 字节头** 开始，后跟可变长度操作数：

```
Byte 0:  opcode  (u8)
Byte 1:  type    (u8)   — 结果/操作数类型标签；对控制流指令为 0x00
Byte 2-3: dst    (u16)  — 目标寄存器；无目标时为 0xFFFF
Byte 4+:  ...    — 按操作码定义的额外操作数字（u8 / u16 / u32 / i64）
```

所有多字节整数：**小端序（little-endian）**。

**编码示例**（`%2:i32 = add i32 %0, %1`）：
```
10          ; opcode = add
04          ; type = i32
02 00       ; dst = 2
00 00       ; src_a = 0
01 00       ; src_b = 1
```
共 8 字节。

---

## 二进制文件格式（.zbc）

### 文件头（16 字节）

```
[4]  magic:         0x5A 0x42 0x43 0x00   ("ZBC\0")
[2]  version_major  当前 1
[2]  version_minor  当前 27 (详见 minor changelog 表)
[2]  flags          bit0=Stripped, bit1=HasDebug, bit2=SymOnly
[2]  section_count
[4]  reserved
```

**Flags 语义**：
- `Stripped (0x01)` — `.cache/*.zbc` 模式，缺 STRS/TYPE/SIGS/EXPT/IMPT，仅供增量编译与 zpkg 索引使用
- `HasDebug (0x02)` — 文件含 DBUG section（line table + 局部变量名）
- `SymOnly (0x04)` — debug-symbol sidecar（`.zsym`），不可作为主模块加载，需与 build_id 匹配的主 zbc 配对

### Section 目录（每项 12 字节）

```
[4]  tag            NSPC / STRS / BSTR / TYPE / SIGS / IMPT / EXPT
                  / FUNC / DBUG / TIDX / FRCS / BLID
[4]  offset         从文件头起的字节偏移
[4]  size           section 字节长度
```

### STRS（String Pool，segment-dict，zbc 1.21 起）

```
[4]     seg_count                                    唯一段数量
segs:   seg_count × { varint seg_byte_len, UTF-8 seg_bytes }   段字典（去重,first-seen 序）
[4]     str_count                                    池串数量
strs:   str_count × { varint seg_n, seg_n × varint seg_idx }   每串 = 段索引序列
```

- 每个池串按 ASCII `.` 切分为段；唯一段在段字典存一次；串本身表示为「段索引序列」，
  reader 以 `segs.join(".")` 无损还原（切/拼在 `.` 上互逆，不假设串是 FQ 名）。
- **串索引不变**：`str_count` 顺序 == 旧 `count` 顺序，其它段引用 STRS 仍用同一 0-based 池索引
  （`const.str` / TYPE / SIGS / cross-zpkg token 等零改动）。
- 消除了旧编码的两处冗余：可推导的 per-string `offset`（前缀和）+ FQ 名重复前缀
  （`Std.`/`Std.String.` 等只存一次）。z42.core STRS 40420B→22475B（−44%）。
- varint = 无符号 LEB128（每字节低 7 位 + 续标志）。段长/段数/段索引均 varint。

> 1.21 前旧布局为 `count + count×{u32 offset, u32 byte_len} + UTF-8 blob`，已废弃（strict-pin 不兼容）。

### TYPE（类型描述符）

```
[4]  count
每项:
  [1]  kind         0=class, 1=struct, 2=enum
  [4]  name_str_idx
  [2]  field_count / variant_count
  fields: { [4] name_str_idx, [1] type_tag, [2] type_param } × field_count
```

### FUNC（函数体）

```
[4]  count
每个函数:
  [4]  name_str_idx
  [4]  flags          bit0=exported, bit1=async
  [2]  param_count    前 param_count 个寄存器为入参
  [2]  reg_count      函数内虚拟寄存器总数
  [2]  block_count
  [4]  instr_count    指令流总 byte 数
  [2]  exc_count      异常表条目数

  block_table:  block_count × [4 first_instr_byte_offset, u8 param_count, u16[] param_regs]
  exc_table:    exc_count × {
    [2] try_start_block
    [2] try_end_block     ; exclusive
    [2] catch_block
    [4] catch_type_str    ; 0xFFFFFFFF = catch all
    [2] catch_reg
  }
  instr_stream: instr_count 字节的指令序列
```

### EXPO（导出表）

```
[4]  count
每项: { [4] name_str_idx, [4] func_idx, [1] kind }
      kind: 0=func, 1=class, 2=global
```

### IMPO（导入表）

```
[4]  count
每项: { [4] module_str_idx, [4] name_str_idx, [1] kind }
```

### DBUG（调试信息；可选 ；1.2 重组、1.3 不变）

2026-05-10 split-debug-symbols 把 LineTable 从 FUNC body 抽出并入 DBUG，使 DBUG 成为 z42 调试信息的唯一容器：

```
[4]  func_count                    // 必须等于 FUNC.func_count（一对一对齐）
每个函数:
  [2]  line_count
  line_table: line_count × {
    [2] block_idx
    [2] instr_idx
    [4] line
    [4] file_str_idx               // 0xFFFFFFFF = 无（line 但跨文件不可定位）
    [4] column                     // 1-based；0 表示未知（trace 退化为 file:line）
  }
  [2]  var_count                   // 局部变量名表（debug 模式）
  var_table:  var_count × { [4] name_str_idx, [2] reg_id }
```

**触发条件**：模块任何函数 LineTable 或 LocalVarTable 非空即写。Stripped 模式（`.cache/*.zbc`）也写 DBUG，保证 dev workflow 异常 trace 显示 file:line:col。Strip 模式（release）剥离 DBUG → 移迁到 sidecar zsym。

### BLID（Build ID；1.2 split-debug-symbols；可选；总是最后一个 section）

```
[16] BLAKE3-128(zbc with BLID payload zeroed)
```

写入时机：strip 模式产 zbc 时追加 BLID section 为最后一个，初始 16 字节占位 0，AssembleFile 完成后回填 BLAKE3-128。sidecar 共享相同字节，runtime 据此配对。

### sidecar zbc（`.zsym`）

`ZbcFlags.SymOnly = 0x04` 标识。仅含 NSPC + STRS（debug 字符串子集）+ DBUG + BLID。不可作为主模块加载（reader 立即 bail）。runtime 加载主 zbc/zpkg 后探测同目录 `<name>.zsym`，build_id 匹配则把 DBUG 合入 FuncBody。

> **历史注**：早期文档将 META section 描述为"调试信息"，与代码不符（META 实际存模块名/版本/entry）。2026-05-10 split-debug-symbols 统一为 DBUG 单一调试容器，文档同步至此。

### TIDX（Test Index，可选；spec R1）

编译时收集的测试元数据。仅当模块含至少一个 `[Test]` / `[Benchmark]` /
`[Setup]` / `[Teardown]` / `[Ignore]` / `[Skip]` 注解的函数时由
`ZbcWriter.BuildTidxSection` 写入；缺失等价于"无测试"。z42b runner（Std.Test.Runner，R3）
读这里发现测试，**不再**扫整个 method table。

字段 little-endian 固定宽度（与其他 sections 一致）。

```
[4]  magic         "TIDX" (54 49 44 58)
[1]  version       1=R1.A+B 占位（已废弃，reader 拒收），2=当前
[4]  entry_count

每条 TestEntry（27 字节 + 4 × test_case_count）:
  [4]  method_id                   (索引 module.functions[])
  [1]  kind                        1=Test 2=Benchmark 3=Setup 4=Teardown 5=Doctest(reserved)
  [2]  flags                       bit0=Skipped bit1=Ignored bit2=ShouldThrow bit3=Doctest
  [4]  skip_reason_str_idx         (0=无；否则 1-based string pool 索引)
  [4]  skip_platform_str_idx       (0=无平台过滤；否则 platform 名 1-based idx，如 "ios")
  [4]  skip_feature_str_idx        (0=无特性过滤；否则 feature 名 1-based idx，如 "jit")
  [4]  expected_throw_type_idx     (0=无 [ShouldThrow]；R4 填入异常类型 1-based idx)
  [4]  test_case_count             ([TestCase(args)] 实例数；R4 时填非零)

  test_cases: test_case_count × {
    [4]  arg_repr_str_idx          (1-based string pool 索引，参数文本表示；R4 升级为 typed)
  }
```

**版本演化**：

- v=1（R1.A+B，2026-04-29）：原始格式，仅 `skip_reason_str_idx`
- v=2（R1.C，2026-04-29）：加 `skip_platform_str_idx` + `skip_feature_str_idx`，
  支持 `[Skip(platform: "ios", feature: "jit", reason: "...")]` 条件跳过

R1.A+B 的 v=1 在引入 parser 支持前 bump 到 v=2；无任何 v=1 文件曾被实际写
入磁盘，reader 显式拒收以避免未来歧义（pre-1.0 不留兼容路径）。

---

## 执行模型

zbc 的解释器循环、JIT 翻译模型、AOT 编译策略统一记录在 [`execution-model.md`](execution-model.md)（Interpreter / JIT / AOT 三模式以及 mixed-mode 切换）。

本文聚焦 **wire format**——指令在二进制流中如何编码、section layout、版本号；不重复执行端的 fetch-decode-dispatch 细节。

---

## 二进制 ↔ 文本互转

```bash
# 二进制 → 文本（反汇编）
z42c --disassemble foo.zbc > foo.zasm

# 文本 → 二进制（汇编）
z42c --assemble foo.zasm -o foo.zbc
```

文本格式保留所有信息（寄存器编号、类型标签、字符串池、异常表），互转无损。

---

## 版本兼容性

**Strict-pin 政策**（与 [`philosophy.md` "不为旧版本提供兼容"](../../../.claude/rules/philosophy.md#不为旧版本提供兼容2026-04-26-强化) 对齐）：

- Reader 仅接受 `major == ZbcWriter.VersionMajor && minor == ZbcWriter.VersionMinor`。pre-1.0 z42 阶段**不为旧 zbc minor 提供向前 / 向后兼容**；每次 minor bump = 所有现存 zbc artifacts 必须 regen（`./xtask build test`）。
- **当前版本**：`major=1, minor=27`（详见下方 minor changelog）
- **触发 minor bump** 的事项：新增 opcode / 新增 section id / 已定义 section 内部字段语义变化 / Flag 位语义变化
- **触发 major bump** 的事项（迄今未发生）：改 magic / 改 16B header 字段宽度或排列 / 改 section directory 12B 条目格式 / 重划 Token 编码空间（IMPORT_BASE / UNRESOLVED 等）
- **未识别 section**：reader 通过 dict-lookup 自动跳过（不在已知 tag 集合内的 section 不影响其他 section 加载）。这是 v1 内 "加 section 不破坏 reader" 的唯一保留兼容点；但前提是新 section 不携带必须信息（必须信息出现时 minor bump 本身就让旧 reader bail）。
- **未来 v2**：v1 框架内能容纳的变化都走 minor bump；只有上述 "触发 major bump" 事项才考虑 v2。v2 出现前不预留任何 v2 兼容代码。

### Minor changelog

| minor | 日期 | 触发 spec | 引入内容 |
|:-----:|------|----------|---------|
| 1.0 | 2026-05-09 | [tokenize-ir-and-zbc-bump](../../spec/archive/2026-05-09-tokenize-ir-and-zbc-bump/) | 重设结构骨架（替换 pre-1.0 sequential format）；IR 字段 tokenized via TokenAllocator (local index OR `IMPORT_BASE + STRS idx` for cross-zpkg) |
| 1.1 | 2026-05-10 | [span-column-propagate](../../spec/archive/2026-05-10-span-column-propagate/) | Line table entry 加 `u32 Column`（除 Line 外）|
| 1.2 | 2026-05-10 | [split-debug-symbols](../../spec/archive/2026-05-11-split-debug-symbols/) Phase 1 | `ZbcFlags.SymOnly` + `BLID` section（16B BLAKE3-128 build_id，always last）|
| 1.3 | 2026-05-10 | split-debug-symbols Phase 4 | `SIGS` 加 per-parameter type names（u32 strIdx × ParamCount），stack-trace signature decoration |
| 1.4 | 2026-05-11 | [add-generic-func-constraint](../../spec/archive/2026-05-11-add-generic-func-constraint/) | Constraint bundle flag 0x40 + per-param/return type-name strings (Z42FuncType signature) |
| 1.5 | 2026-05-13 | [fix-numeric-cast-lowering](../../spec/archive/2026-05-13-fix-numeric-cast-lowering/) | 新 opcode `Convert` (0xB1) 表达显式数值类型转换（替换之前 cast 为 IR no-op 的语义） |
| 1.6 | 2026-05-19 | [fix-array-default-init](../../spec/archive/2026-05-19-fix-array-default-init/) | `ArrayNew` opcode 在 `size` 之后追加 1 byte element type tag（`TypeTags::*`），驱动数组元素的 per-type 默认值（int→0 / bool→false / char→'\0' / ref→null） |
| 1.7 | 2026-05-27 | [align-zbc-reader-writer-asymmetry](../../spec/archive/2026-05-27-align-zbc-reader-writer-asymmetry/) | SIGS / TYPE 在 u8 TypeTag 之后追加 u32 type_str_idx（ret_type / field type）。Reader 优先 string 作权威类型名；tag 留作 hint。修 Read→Write byte parity；启用 ReadWriteRoundTrip CI 防线 |
| 1.8 | 2026-05-27 | [jit-type-specialization](../../spec/changes/jit-type-specialization/) P0 step 0.3/0.4 | 新 `REGT` section（`u32 fn_count` + per-fn `u32 reg_count + u8[] IrType`），承载每函数的 per-register `IrType` byte 数组。Reader 把每条解到 `Function.reg_types: Box<[IrType]>`，JIT translator 后续据此跳过 `jit_add` / `jit_eq` / `jit_and` helper、直接 emit Cranelift `iadd` / `icmp` / `band`。zpkg 0.9 同步在 packed module 加 length-prefixed `RegtData`。Pre-1.8 zbc 不可读 |
| 1.9 | 2026-05-30 | [add-test-timeout-attribute](../../spec/changes/add-test-timeout-attribute/) | TIDX section bumped to v=3：每条 `TestEntry` 在 `TestCase[]` 之后追加 `timeout_ms: i32`（`0` = 无 override，runner 用默认 300 s；正值 = `[Timeout(milliseconds: N)]` 显式 cap）。Compile-time 由 E0917 保证 `0 < N ≤ i32::MaxValue`，runtime 防御性地把负值降级回 0。zpkg 0.10 同步联动。Pre-1.9 zbc 不可读 |
| 1.10 | 2026-06-09 | [add-attribute-reflection](../../spec/changes/add-attribute-reflection/) | TYPE section 每个 class 在 type-param block 之后追加 `attr_count: u16` + `attr_count ×` (`type_name_str_idx: u32`, `factory_func_str_idx: u32`)，承载用户自定义 attribute 的 (类型名, 工厂函数名) 引用。运行期 `Type.GetCustomAttributes()` 调工厂函数构造活实例 + 缓存（C3）。Count 恒写（0=无 attribute）保证 per-class 布局统一。zpkg 0.12 同步联动。Pre-1.10 zbc 不可读 |
| 1.11 | 2026-06-09 | [add-attribute-reflection-methods](../../spec/changes/add-attribute-reflection-methods/) | SIGS section 每个 function 在 type-param block 之后追加同形的 `attr_count: u16` + (type-name, factory-func) str-idx 对，承载方法/函数级用户 attribute。运行期 `MethodInfo.GetCustomAttributes()` 据此构造活实例（C3b）。Count 恒写。ZpkgWriter 的 global SIGS 同步加同样字段。zpkg 0.13 同步联动。Pre-1.11 zbc 不可读 |
| 1.12 | 2026-06-10 | [add-reflection-type-flags](../../spec/changes/add-reflection-type-flags/) | TYPE section 每个 class 在 attr block 之后追加 `flags: u8`（bit0 abstract / bit1 sealed / bit2 struct / bit3 record），承载类修饰符。运行期载入 `TypeDesc.class_flags`，背书 `Type.IsAbstract` / `Type.IsSealed`（struct/record 位已写进 wire，将来 `IsValueType`/`IsRecord` 纯 stdlib、不再 bump 格式）。zpkg 0.14 同步联动。Pre-1.12 zbc 不可读 |
| 1.13 | 2026-06-10 | [add-reflection-static-fields](../../spec/changes/add-reflection-static-fields/) | TYPE section 每个 class 在 flags 字节之后追加静态字段块：`static_field_count: u16` + 每条 (`name: u32`, `type_tag: u8`, `type_str: u32`)，与实例字段块同形。运行期载入 `TypeDescCold.static_fields`，`Type.GetFields()` 在实例字段后追加静态字段（`FieldInfo.IsStatic = true`）。仅声明类自身静态字段（继承静态延后）。zpkg 0.15 同步联动。Pre-1.13 zbc 不可读 |
| 1.14 | 2026-06-10 | [add-field-attribute-reflection](../../spec/changes/add-field-attribute-reflection/) | TYPE section 每个字段记录（实例块 + 静态块）在 `type_str: u32` 之后追加 `attr_count: u16` + 每条 (`type_name: u32`, `factory: u32`)，承载字段级用户 attribute 引用，与 class/method attr 同形。运行期索引进 `TypeDescCold.field_attributes`，`FieldInfo.GetCustomAttributes()` 调工厂构造活实例。zpkg 0.16 同步联动。Pre-1.14 zbc 不可读 |
| 1.15 | 2026-06-10 | [add-parameter-attribute-reflection](../../spec/changes/add-parameter-attribute-reflection/) | SIGS section 每个函数记录在方法级 `attr_count` 块之后追加**每参数 attr-ref 块**——对 `param_count` 个参数（含实例方法的隐式 `this` 槽，恒空）各写 `attr_count: u16` + (`type_name: u32`, `factory: u32`) 对偶。运行期载入 `FunctionCold.param_attributes`（SIGS 对齐），`ParameterInfo.GetCustomAttributes()` 按源参数位置（= wire 索引 − this 偏移）取并调工厂。zpkg 0.17 同步联动。Pre-1.15 zbc 不可读 |
| 1.16 | 2026-06-12 | [add-reflection-array-element-type](../../spec/changes/add-reflection-array-element-type/) | `ArrayNew` opcode 在 element type tag 之后、`ArrayNewLit` 在 elem args 之后各追加 `element_type: u32`（STRS idx，元素类型 FQ 名，如 `int` / `geometry.Point` / `int[]`）。运行期数组值改由 `ArrayObj { element_type, elems }` 承载（不再类型擦除），`Value::Array(GcRef<ArrayObj>)`。背书 `Type.IsArray` / `Type.GetElementType()` 与非擦除的 `arr.GetType()`。zpkg 0.18 同步联动。Pre-1.16 zbc 不可读 |
| 1.17 | 2026-06-14 | [add-reflection-get-interfaces](../../spec/changes/add-reflection-get-interfaces/) | TYPE section 每个 class 在静态字段块之后追加**接口块**：`interface_count: u16` + `interface_name_idx[]: u32`（类直接声明的接口名，bare）。运行期载入 `TypeDescCold.interfaces`，`Type.GetInterfaces()`（`__type_interfaces` builtin）沿 base 链聚合本类 + 继承接口（按名 dedup）。Count 恒写（0=无接口）。传递接口实现（interface-extends-interface）延后。zpkg 0.19 同步联动。Pre-1.17 zbc 不可读 |
| 1.18 | 2026-06-16 | [add-reflection-generic-type-definition](../../spec/changes/add-reflection-generic-type-definition/) | 新 `Typeof` opcode（`0x73`）：`dst` + `TypeName: u32`（STRS idx，定义 FQ 名）+ `type_arg_count: u8` + `type_arg_idx[]: u32`（实例化 arg FQ 名，镜像 `ObjNew` type_args 编码）。所有 `typeof(...)` 统一 emit 它，移除 `__typeof` builtin。背书 `Type.IsGenericTypeDefinition` / `GetGenericTypeDefinition()` + 修 `typeof(Box<int>).GetGenericArguments()`。zpkg 0.20 同步联动。Pre-1.18 zbc 不可读 |
| 1.19 | 2026-06-16 | [add-reflection-interface-class-predicates](../../spec/changes/add-reflection-interface-class-predicates/) | **interface 现在 emit 一条最小 TYPE 条目**（identity + flags，无 base/字段/方法表），故 `typeof(IFoo)` 解析到真句柄。`class_flags` 字节扩 **bit4 = interface**（bit0-3 不变；bit5 预留 enum）。背书 `Type.IsInterface` + 把接口排除出 `Type.IsClass`。无新字段，仅 flags 语义扩展 + TYPE section 多接口条目。zpkg 0.21 同步联动。Pre-1.19 zbc 不可读 |
| 1.20 | 2026-06-16 | [add-reflection-assignable-from](../../spec/changes/add-reflection-assignable-from/) | TYPE section 每类的**接口块改存 FQ 名**（`Demo.IShape`，此前 bare `IShape`）。结构不变（`u16 count + str idx[]`），仅字段语义 bare→FQ。`GetInterfaces()` 据此解析到**真接口句柄**；`x is IShape`/`as`/`Type.IsAssignableFrom` 按 FQ 名做 robust 接口身份比较。zpkg 0.22 同步联动。Pre-1.20 zbc 不可读 |
| 1.21 | 2026-07-09 | [reencode-strs-segment-dict](../../spec/changes/reencode-strs-segment-dict/) | **STRS 段重编码为 segment-dict**：`seg_count + 段字典(varint len + utf8)×seg_count + str_count + (varint seg_n + varint seg_idx×seg_n)×str_count`。按 `.` 切分去重 namespace 段，串=段索引序列（reader `join('.')` 无损还原）。消除旧编码的可推导 offset 冗余 + FQ 名前缀重复（z42.core STRS −44%）。**串索引不变** → 其它段零改动。zpkg 0.25 同步联动。Pre-1.21 zbc 不可读 |
| 1.22 | 2026-07-09 | [add-enum-type-metadata](../../spec/changes/add-enum-type-metadata/)（unify-type-metadata P1-a） | **enum 作 TYPE 段类型实体**：`class_flags` 新增 **bit5=enum**；置位时类记录尾部追加 enum 成员块 `member_count:u16 + (name_idx:u32, value:i64)×n`（gated → 非 enum 类记录字节不变）。背书 `Type.IsEnum` + `Std.Enum.GetNames/GetValues/GetName` + `typeof(EnumType)`。unify-type-metadata 首砖（enum 成员值有了 TYPE 的家；TSIG enum 块 P1 仍并存，P3 删）。zpkg 0.26 同步联动。Pre-1.22 zbc 不可读 |
| 1.23 | 2026-07-10 | [add-member-visibility](../../spec/changes/add-member-visibility/)（unify-type-metadata P1-b） | **成员可见性入 TYPE/SIGS**：TYPE 段每字段块（实例 + 静态）在 attrs 之后追加 `visibility:u8`；SIGS 段每函数在 `is_static` 之后追加 `visibility:u8`（0=public / 1=private / 2=protected；non-gated → 每成员固定 +1 byte）。背书 `FieldInfo.IsPublic/IsPrivate` + `MethodInfo.IsPublic/IsPrivate`（reader 把 SIGS visibility 灌进 `Function.visibility`、TYPE visibility 灌进 `FieldSlot`/`FieldDesc`）。unify-type-metadata 第二砖（TSIG 可见性字段有了 TYPE/SIGS 的家；P3 删 TSIG）。zpkg 0.27 同步联动。Pre-1.23 zbc 不可读 |
| 1.24 | 2026-07-10 | [add-method-modifiers](../../spec/changes/add-method-modifiers/)（unify-type-metadata P1-c） | **方法修饰符入 SIGS**：SIGS 段每函数在 `visibility` 之后追加 `method_flags:u8`（bit0=virtual / bit1=abstract；`static` 仍由既有 `is_static` 字节表达，不重复；non-gated → 每函数固定 +1 byte）。背书 `MethodInfo.IsVirtual`（**权威化**——从 vtable-presence 启发式改读 flag，virtual/override/abstract 皆置 bit0，镜像 C#）+ 新增 `IsAbstract`（bit1）。reader 把 SIGS method_flags 灌进 `Function.method_flags`（与 `is_static`/`visibility` 同源同路径）。unify-type-metadata 第三砖。zpkg 0.28 同步联动。Pre-1.24 zbc 不可读 |
| 1.25 | 2026-07-10 | [add-param-metadata](../../spec/changes/add-param-metadata/)（unify-type-metadata P1-d） | **参数元数据入 SIGS**：每函数在 `method_flags` 后追加 `min_arg:u16`（必填逻辑参数个数）+ `params_from:u8`（varargs 逻辑 index，0xFF=无）；每参数在 `param_type` 后追加 `name_str_idx:u32`（源名，this 槽="this"）+ `default_kind:u8 + payload`（0=无/1=null/2=i64 8B/3=f64bits 8B/4=bool 1B/5=str u32 idx；字面量折叠，非字面量默认值 kind=0）。背书 `ParameterInfo.IsOptional/IsParams/DefaultValue` + `Name` **权威化**（SIGS 优先，DBUG 回退）。unify-type-metadata 第四砖。zpkg 0.29 同步联动。Pre-1.25 zbc 不可读 |
| 1.26 | 2026-07-11 | [add-delegate-metadata](../../spec/changes/add-delegate-metadata/)（unify-type-metadata P1-e ②） | **delegate-as-class**：`class_flags` 新增 **bit6=delegate**（无额外 payload——沿 1.19 interface「flags 语义扩展 + 新增条目」先例 bump）。每 `delegate` 声明（含泛型）emit 一条 TYPE 条目（FQ 名 + bit6 + TypeParams——泛型 tps 存 TYPE，Invoke 按名引用）+ 合成 `<FQ>.Invoke` 死体桩进 SIGS/FUNC（实例/virtual/参数源拼写+名+P1-d 元数据；真实调用走 CallIndirect，桩永不被调）。背书 `Type.IsDelegate` + Invoke 签名反射；P3 删 TSIG 的 delegate 表前置。zpkg 0.30 同步联动。Pre-1.26 zbc 不可读 |
| 1.27 | 2026-07-14 | [stabilize-dispatch-keys](../../spec/changes/stabilize-dispatch-keys/)（方案A） | **无 wire-layout 变化**：派发键从「兄弟集相关」改为「方法自身签名纯函数」（`regName` 一律全签名 mangle，协议豁免名 `ToString`/`Equals`/… 保持裸名）→ `CallInstr`/`VCall` 操作数字符串 + SIGS/导出方法名 **全局重键**（内容变、布局不变）。VM vtable 槽键随之保留 `$` 后缀（`derive_simple_method_name` 不再截断）→ VCall 与 vtable 一致 + 重载虚方法各占独立槽。bump 仅为触发 ci-bootstrap 两代自举整树重键（耦合 zpkg 0.32）。Pre-1.27 zbc 不可读 |
| 1.28 | 2026-07-18 | [fix-crosspkg-interface-impl](../../spec/changes/fix-crosspkg-interface-impl/) | **接口 TYPE 条目尾部加方法签名块**（`class_flags` bit4=interface gated：`mcount:u16 + (name_idx:u32, ret_idx:u32, pcount:u8, ptype_idx:u32×pcount)×n`，同 enum 块模式）——跨包接口实现（`class X : IFace@其他包`）须能从依赖 zpkg 恢复接口方法名/签名（抽象接口方法无 body 不入 SIGS、EXPT 已删 → TYPE 是唯一载体；`TsigReconcile._rebuildInterface` 消费）。VM 侧 parse-and-discard（接口派发走 vtable）。耦合 zpkg 0.33。Pre-1.28 zbc 不可读 |
| 1.29 | 2026-08-05 | [add-escape-analysis-stack-alloc](../../spec/changes/add-escape-analysis-stack-alloc/) | **`ObjNew`/`ArrayNew`/`ArrayNewLit` 编码尾部各加 1 个 `u8` 栈分配标志**（`1`=帧 arena 栈分配 / `0`=堆；镜像 `MkClos.stack_alloc` 先例）。由编译期逃逸分析 pass 置位；interp 据此在 per-context 栈 arena 分配、绕过 GC（JIT 忽略 flag、照常堆分配）。耦合 zpkg 0.34。Pre-1.29 zbc 不可读 |
| 1.30 | 2026-08-07 | [impl-sealed-semantics](../../spec/changes/impl-sealed-semantics/) | **SIGS `method_flags:u8` 新增 bit2=sealed**（`METHOD_FLAG_SEALED`；`sealed override` / 简写 `sealed` 方法置位，且必连带 bit0=virtual）。**字节布局不变**——bit2 先前保留为 0；strict-pin 下仍 bump 以防「同版本号、不同 bit2 语义」的静默分歧（跨 nightly 种子链）。背书 `MethodInfo.IsSealed` + 编译期 sealed-receiver 去虚化（follow-up）。耦合 zpkg 0.35。Pre-1.30 zbc 不可读 |
| 1.31 | 2026-08-09 | [add-struct-value-semantics](../../spec/changes/add-struct-value-semantics/) | **TYPE section 尾部新增值 struct 布局块**（Flags bit2=struct gated）：`size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n` = 带种类引用位图，供运行时 GC 定位 blob 内引用叶子 + StructCopy 逐叶子 clone（字段 offset/size 由 codegen 烘焙进访问指令、不经此块）。同时 **z42c 开始 emit** blob 值指令 `StructAlloc/Copy/FieldGetPrim/SetPrim`（opcode 0xC0–0xC3，A-support 已加编解码/执行）。耦合 zpkg 0.36。Pre-1.31 zbc 不可读 |
| 1.32 | 2026-08-11 | [add-struct-heap-inline](../../spec/changes/add-struct-heap-inline/) | **TYPE section 新增合成内联 struct 布局块**（`CLASS_FLAG_HAS_INLINE_STRUCT` bit7=0x80 gated，紧随 struct 块）：`size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n` = class 内联 struct 字段的**对象相对**字节区大小 + 引用位图，供 `ScriptObject` alloc（`struct_bytes`/`struct_refs` 侧表）+ 内联字段访问定位引用叶子。同 struct 块 shape（reader 复用 `StructLayoutDesc`）。背书 `class C { Point pt; }` 字节内联（D1-a：基元字节内联 + 引用叶子侧表）。耦合 zpkg 0.37。Pre-1.32 zbc 不可读 |
| 1.33 | 2026-08-13 | [enforce-class-access](../../spec/changes/enforce-class-access/) | **TYPE section 新增类声明可见性字节**（紧随 `class_flags` u8）：`visibility:u8`（0=public/1=private/2=protected/3=internal），**每条 TYPE 记录 +1 字节**（无 flag gate，恒存在）。跨包 `internal` 类引用强制的元数据载体——`TsigReconcile` 从此字节还原 `ExportedClassZ.Visibility`，`ImportedSymbolLoader` 填 `Z42ClassType.Visibility`，`AccessChecker.CheckTypeRef` 据此对 imported internal 类的跨包引用 emit E0404。VM 侧 **read-and-discard**（暂无类可见性反射面）。耦合 zpkg 0.38。Pre-1.33 zbc 不可读 |
| 1.34 | 2026-08-14 | [unify-object-byte-layout](../../spec/changes/unify-object-byte-layout/)（PR-1） | **TYPE section 新增对象全字段布局块**（普通引用类=`class_flags` 不含 struct/interface/enum/delegate=116 的**派生谓词** gated，随 inline 块之后——class flags U8 满位无空闲，故用派生谓词而非新标志位，writer/reader 同谓词锁步）：`ObjectSize:u32 + field_count:u16 + (off:u32, size:u32, kind:u8)×n + ref_count:u16 + (ref_off:u32, ref_kind:u8)×m` = 每直接字段对象相对 offset/size/kind（供 FieldGet/反射）+ 扁平 8B 引用位图（含内联 struct 内部叶子，供 GC）。**引用叶子按 8B 裸指针宽度**（C# 等价终点）。**PR-1 休眠**：`TypeDescCold.object_layout` 携带但 runtime 不消费（仍走 slots）；PR-2 切字节存储时替换 slots。OWN 字段 offset 从 0，继承 base-shift 由消费方组合。耦合 zpkg 0.39。Pre-1.34 zbc 不可读 |

> **如何 bump minor**：见 [`version-bumping.md` §"Bumping `.zbc` minor version"](../../../.claude/rules/version-bumping.md#bumping-zbc-minor-versionfreeze-zbc-v1-2026-05-14)。简而言之 — 写 `ZbcWriter.VersionMinor++` + 同步 `zbc_reader.rs` 常量 + 本表加一行 + `xtask build test` regen（原地重生 6 个 zbc-format fixture）+ commit。Invariant CI 校验三方常量一致。

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

## Deferred / Future Work

> **reader-writer asymmetry**（2026-05-14 调查 / 2026-05-27 修复落地）：原 SIGS / TYPE TypeTag 1 byte 编码是 lossy（`"int"` → `I32` → canonical `"i32"`），导致 Read→Write 字节不对账。**已通过 [align-zbc-reader-writer-asymmetry](../../spec/archive/2026-05-27-align-zbc-reader-writer-asymmetry/) (zbc 1.7) 落地 Option A**：SIGS / TYPE 在 u8 TypeTag 之后追加 u32 type_str_idx 作权威类型名；tag 留作 hint。ReadWriteRoundTrip CI 防线启用。
