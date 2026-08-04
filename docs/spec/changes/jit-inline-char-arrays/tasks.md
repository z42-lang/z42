# Tasks: JIT 内联 char[] 去箱访问

**变更说明：** 把 packed 数组的 JIT 内联去箱扩到 `char[]`（`Chars` backing，char=4 字节 scalar）。
复用 int[] 的 width-4 机制：`arr_prim_elem(Char)=(tag 3, width 4)`；`packed_num_ptr`/`packed_elem_width`
纳入 `Chars`。ArrayGet width-4 sextend 对合法 char（≤0x10FFFF，bit31=0）即 zext，正确；寄存器存 tag=3
+ 低 4 字节 codepoint（镜像 emit_const_char）。ArraySet 运行期宽度 4 → 截断存 codepoint。
**原因：** char[] 是 String.IndexOf/Split/Replace 的扫描热路径；此前元素 IrType::Char→helper。纯性能，语义不变。
**文档影响：** 无（JIT codegen 内部）。

- [x] 1.1 `types.rs`：`packed_num_ptr`/`packed_elem_width` 纳入 `Chars`（width 4）
- [x] 1.2 `jit/translate.rs`：`arr_prim_elem` 加 `IrType::Char → (3, 4)`
- [x] 1.3 正确性：charc（多字节 scalar/非 BMP emoji/char 比较/写/String 操作）interp==jit 全一致；cargo test 897/0
- [x] 1.4 性能：char[2M] scan x8 jit 376→263ms ≈1.43×
- [x] 1.5 e2e-direct 205/208（interp+jit，=baseline 同款 3 例直跑器局限，零回退）→ PR
