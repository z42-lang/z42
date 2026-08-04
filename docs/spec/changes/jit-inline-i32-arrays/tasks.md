# Tasks: JIT 内联 int[]（I32）去箱访问

**变更说明：** 把 packed 数组 Step 4 的 JIT 内联去箱从 wide(I64/F64) 扩到 I32(int[])：ArrayGet 按
dst 编译期宽度（4=int sign-extend / 8=long·double raw）内联读；ArraySet 按**运行期 backing 宽度**内联
写（4 截断 / 8 原样 / 0 回退 helper），并放宽索引门到 I32（int 索引）。
**原因：** int[] 最常见，此前元素 I32→helper（无提升）；long/double 已 2.1×，int[] 也应吃到扫描加速。纯性能优化，语义不变。
**文档影响：** 无新语法/IR/格式；JIT codegen 内部机制。

## 类型：perf（JIT codegen 优化，语义不变）→ 最小化模式

- [x] 1.1 `types.rs`：`wide_data_ptr`→`packed_num_ptr`（+I32）；新增 `packed_elem_width()`（0/4/8）
- [x] 1.2 `jit/helpers/array.rs`：`jit_array_data`/`_opt` 加 `out_width` 出参（写 packed_elem_width）
- [x] 1.3 `jit/helpers/registry.rs`：两个 decl 加一个 ptr 出参
- [x] 1.4 `jit/translate.rs`：`arr_prim_elem`(val_tag,width) + `idx_int_ok`(I32|I64)；hoist map 带 width
- [x] 1.5 ArrayGet 内联：dst 编译期宽度（可靠）→ sextend(4)/raw(8)
- [x] 1.6 ArraySet 内联：**运行期宽度**（val 宽度对窄化存不可靠）——width==0→helper 回退分支 + 4/8 存分支
- [x] 1.7 正确性：i32c（i32::MIN/负数/窄化存/读改写）+ w0（byte[]/bool[]/char[] 走 width0 回退）+ long/double + corr
      interp==jit **全一致**；OOB 干净抛；cargo test **894/0**
- [x] 1.8 性能：int[5M] fill+scan JIT **979→657ms ≈1.5×**（long/double 不回退 464/574ms）
- [x] 1.9 完整 GREEN：cargo test 894/0 + e2e-direct 205/208（interp+jit，3 例=baseline 同款直跑器局限）；含修复闭包 env(Boxed)读段错误

## 关键设计（为什么 ArraySet 要运行期宽度）
- ArrayGet 的 dst 寄存器类型**可靠**=元素类型 → 宽度编译期可定。
- ArraySet 的 val 寄存器类型**不可靠**：窄化存 `int[i]=<i64 值>`（如字面量 -2147483648 是 i64）val 是 I64
  但槽是 4 字节；IR 的 array 寄存器是泛型 Ref、不带元素类型。按 val 宽度写 → 8 字节进 4 字节槽 → **越界写坏邻居**。
  故 ArraySet 由 `jit_array_data` 返回的**运行期 backing 宽度**决定存宽度；width==0（byte[]/Boxed/bool/char）回退 helper。
- 附带覆盖：Step 4 wide-only ArraySet 若遇 `int[i]=<i64 值>` 会踩 null（wide_data_ptr(I32)=None）；本改的运行期宽度 + 回退分支一并覆盖（实测 main 未暴露，因该模式罕见）。
