# Design: packed primitive 数组（C#-like typed-backing）

## 参考：C# CLR 数组模型
- **值类型数组**（`int[]`/`byte[]`/`char[]`/struct[]）：元素**内联 packed**存储（`base + header + i*sizeof(T)`），
  **无逐元素装箱**；GC **不扫描**（无内部对象指针）；FFI/`fixed`/`Span<T>` 直接拿连续缓冲指针。
- **引用类型数组**（`object[]`/`string[]`）：存对象**指针**；GC 扫描。
- z42 对齐：primitive backing = 值类型数组；`Boxed(Vec<Value>)` = 引用数组。

## ArrayBacking
```rust
pub struct ArrayObj { pub element_type: Arc<str>, pub backing: ArrayBacking }
pub enum ArrayBacking {
    Bool(Vec<bool>),
    Bytes(Vec<u8>),      // byte/sbyte（窄整型并入，box 时按 element_type/tag 定符号）
    I32(Vec<i32>),       // int/uint/short/ushort/char? （char 单列见下）
    I64(Vec<i64>),       // long/ulong
    Chars(Vec<char>),    // char[]（scalar，与 String.ToCharArray 对齐）
    F64(Vec<f64>),       // double/float
    Boxed(Vec<Value>),   // object/string/数组/nullable/struct-ref …（= C# 引用数组）
}
```
box/unbox 边界（interp 寄存器是 Value）：`get_boxed(i)->Value`（Bytes→`Value::I64(b as i64)`、Chars→`Value::Char`…）；
`set_boxed(i,v)`（`Value::I64(n)→n as u8/i32/i64`、`Value::Char→char`…）。FFI：`as_bytes()->Option<&[u8]>` 等零拷贝切片。

## Decisions
### D1: 增量 GREEN 策略（关键——避免核心大改长期破 GREEN）
- **Step 1a（纯重构，全 Boxed）**：ArrayObj→`{backing}`，`ArrayBacking` 先只 `Boxed`；把 4 opcode + 10 处 `.elems`
  + GC + JIT 全改走 `len()/get_boxed()/set_boxed()/iter_boxed()` 访问器。**行为逐字节等价**→编译 + 全 GREEN。
- **Step 1b（逐类型 packed）**：加 primitive backing，`alloc_array_typed` 按 element_type 选 backing；get/set box/unbox。
  行为保持（值不变、存储 packed）。cargo test + xtask test 绿 + 内存实测。
- **Step 2** GC 跳过 primitive backing；**Step 3** FFI 直读切片；**Step 4** JIT/interp **去箱访问**（性能争取超 1.35×）；
  **Step 5** 收尾 + bench。每步独立 commit + GREEN。

### D2: char 单列 `Chars(Vec<char>)`
char[] 与 `String.ToCharArray()`（已 packed 概念）对齐，且 FFI/文本处理常用。scalar 语义。

### D3: 窄整型（sbyte/short/ushort/uint/…）并入 I32/I64/Bytes
不为每个窄类型单开 backing；按 element_type 决定 box 时的符号/宽度语义。backing 少 = box/unbox 分支少。

### D4: 去箱访问（性能目标，Step 4）
C# 值类型数组访问直接进值寄存器、不装箱。z42 interp 寄存器是 Value（装箱不可免），但 **JIT** 可对
「已知 primitive backing 的 ArrayGet/Set」生成直接 `buf[i]` 原生访问、不过 Value（配合 perf-vm-iteration
Phase 4 去箱）。interp 侧可加 typed-array 快 opcode（如 `ByteArrayGet`）省 box 分支。这步把扫描/访问拉过 1.35×。

## Testing Strategy
- 每 Step：`cargo build` + `cargo test`（gc/array/corelib）+ `xtask test`（e2e/stdlib[Test]/自举）+ 自举不动点。
- Step 1b/3：内存实测（大 byte[] 占用 before/after）；Step 3：FFI marshal before/after（compress/hash 大 buffer）；
  Step 4：数组扫描 before/after（目标 > 1.35×）。
- 反射：`arr.GetType().GetElementType()` 回归。
