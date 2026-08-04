# Proposal: packed primitive 数组（C#-like 值类型数组 typed-backing）

> 类型：**vm**（核心 Value/数组表示变更）→ 完整 spec-first + **分阶段增量**（每阶段独立 GREEN）。
> 参考 **C# CLR 数组模型**：值类型数组内联 packed 存储、无逐元素装箱、GC 不扫描；引用类型数组存指针、GC 扫描。

## Why（用户动机 + 实测依据）

z42 `T[]` 现为 `Value::Array(GcRef<ArrayObj>)`，`ArrayObj.elems: Vec<Value>`——**每个元素一个 24B tagged Value**，
即使 `byte[]`/`int[]`/`char[]` 也是 24B/元素。三重代价：

1. **FFI marshal（用户核心动机「简化 extern call」）**：native 拿 `byte[]` 要**逐字节 unbox**
   （`ext.rs:384` 压缩、`fs.rs:398` 文件写：`Value::I64(n)→push(n as u8)`；输出逐字节 box）。`PinPtr` 救不了
   （字节不连续）。packed `Vec<u8>` → native 直接 `&[u8]`，marshal 循环消失。
2. **内存**：primitive 数组 24B/元素 → packed 后 1B(byte)/4B(char,int)/8B(long,double) = **6–24× 缩减**。
   crypto.rs:27 注释自陈「would balloon the i64-per-byte Value::Array representation」。
3. **GC**：primitive backing 无 Value 引用 → GC **不扫描**（如 C# 值类型数组），减 mark 负载。

（扫描性能仅 ~1.35×，非主因；主因是上面 FFI + 内存 + GC。）

## What Changes

- **`ArrayObj` 换 typed backing**：`ArrayObj { element_type, backing: ArrayBacking }`，
  `ArrayBacking = Bytes(Vec<u8>) | Chars(Vec<char>) | I32(Vec<i32>) | I64(Vec<i64>) | F64(Vec<f64>) |
  Bool(Vec<bool>) | Boxed(Vec<Value>)`（Boxed = 对象/字符串/嵌套数组，= C# 引用数组）。
- **数组 opcode**（array_new/new_lit/get/set/len）：按 elem_tag 选 backing；get/set 在**边界 box/unbox**
  （寄存器仍是 Value）。
- **FFI**：`ArrayObj::as_bytes()->Option<&[u8]>`（及 as_i32s 等）直接给 native 连续缓冲；重写
  ext.rs/fs.rs/network.rs/crypto.rs 的逐字节 marshal 为切片操作。
- **GC**：`arc_heap` 扫描数组时只扫 `Boxed`，primitive backing 跳过。
- **元素类型/反射**：`element_type` 保留，`GetElementType()` 不变。

## Scope（分阶段——每阶段编译 + cargo test + xtask test 绿）

| Phase | 内容 | 产出 |
|------|------|------|
| **P1** | `ArrayBacking` 枚举 + `ArrayObj` 重构 + `len/get_boxed/set_boxed/as_*` API + 4 opcode + alloc + default | 编译绿、行为等价（primitive 数组 packed，box/unbox 边界）|
| **P2** | GC：primitive backing 不扫描；write-barrier 只对 Boxed | GC 套件绿 |
| **P3** | FFI 直读切片：ext.rs/fs.rs/network.rs/crypto.rs marshal 循环 → `as_bytes()` | 压缩/文件/crypto byte[] 免逐字节 marshal + 内存实测 |
| **P4** | JIT：translate.rs/closure.rs `.elems` 适配 typed backing（interp 全绿后）| JIT 一致性绿 |
| **P5** | 收尾：`.elems` 残余 10 处 + 反射 + 文档 + bench（FFI/内存 before/after）| 全绿 + 数字 |

## Out of Scope
- String 表示（保持 UTF-8 dual-index，已定案）。
- JIT 去箱进原生（primitive 数组访问不经 Value）——后续 perf-vm-iteration Phase 4。
- packed 数组的 zbc 序列化优化（数组不入 zbc except literal；不改格式）。

## Open Questions
- [ ] `float`(F32) 单列 `F32(Vec<f32>)` 还是并入 F64？（z42 float→double 提升？）——P1 design 定。
- [ ] `sbyte`(I8)/`short`(I16)/`ushort`(U16)/`uint`(U32)/`ulong`(U64) 是否各自 backing 还是窄类型并入 I32/I64？倾向窄类型并入（backing 少、box/unbox 简单），P1 定。
