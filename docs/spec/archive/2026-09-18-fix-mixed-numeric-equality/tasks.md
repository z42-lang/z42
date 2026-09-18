# Tasks: 整数与浮点的 `==` / `!=` 恒假 / 恒真

> 状态：🟢 已完成 | 完成：2026-09-18

- [x] 1 `semantics.rs` 新增 `numeric_eq`，加宽臂与 `numeric_lt` **一一对应**
      （整数升 f64、Char 升 i64），其余类型退回 `Value: PartialEq`
- [x] 2 `eval_cmp` 的 `Eq` / `Ne` 改走 `numeric_eq`；`Ne` 写成 `!numeric_eq(..)` 以保 NaN unordered
- [x] 3 🔴 **`jit_eq` / `jit_ne` 同样改走 `numeric_eq`** —— 见下「诊断漏掉的第二处」
- [x] 4 golden `src/tests/operators/mixed_numeric_equality.z42`
- [x] 5 `semantics_tests.rs` 补 3 个 `eval_cmp` 单测（加宽 / 不等仍为假 / 非数值不受影响）
- [x] 6 `reference/language/operators.md` 新增「二元数值提升」一节（此前**整页没有**这个概念）
- [x] 7 `internals/runtime/interp-jit-semantics.md` 记录这次漏网 + 差分测试盲区
- [x] 8 `semantics.rs` 模块头表格加「混合数值比较」行 + 盲区警告

## 诊断漏掉的第二处（必须记下来）

第一轮诊断的结论是「JIT 混合操作数必然回退到 helper → `semantics::eval_cmp`，所以改
`eval_cmp` 一处即可覆盖三路」。**前半句对，后半句错**：

```rust
// 修复前 jit_eq —— 根本没经过 semantics
let result = regs[a as usize] == regs[b as usize];   // 直接用 Value: PartialEq
```

`jit_lt` / `jit_le` / `jit_gt` / `jit_ge` 早就收敛到 `semantics::numeric_lt` 了，唯独相等那两个
从没跟上，而 `interp-jit-semantics.md` 却写着「路径 1、2 对同一规则只有一份实现」。
**只改 `eval_cmp` 会得到一个 interp 绿、JIT 红的修复。**

抓到它靠的是去读 `jit_eq` 的函数体，而不是信「回退到 helper」这句话。

## 验证

| 项 | 结果 |
|---|---|
| 20 条断言 × interp / jit 两种模式 | 全部符合预期 |
| golden 对**修复前**的 VM | ✅ 两种模式都在第 25 行红（真回归测试） |
| `cargo test --lib`（debug） | 1359 passed / 0 failed |
| 4 个新 `eval_cmp` 单测 | passed |
| 既有算子 golden ×4 + `type_conversions` | 全 OK |
| `xtask test docs` | 绿 |
| 文档里 7 条断言逐条实跑 | 全部核实（含 53 位尾数舍入那条） |

「golden 对修复前的 VM 会红」这一步不能省——本 change 的起因就是既有测试
**用没坏的运算符去验坏的那个**，不验证新测试会红，等于又造一个。

## 明确不做

**装箱相等不动。** `(object)5 == (object)5.0` 仍为 `false`（走 `BoxedStruct` 逐字节比较，
`eval_cmp` 看不到）。与 C# 的 `((object)5).Equals((object)5.0)` 一致，是正确行为，已写进
`operators.md`。副作用：`Assert.Equal(5, d)` 仍会失败，所以 golden 一律写成
`Assert.Equal(true, <比较表达式>)`（两边都是 bool）。

## 后续

- `docs/learn/src/basics/operators.md`（第 6 章，PR 未合）里有一段
  「⚠️ 别把整数和小数放在 `==` 两边」——本 change 合并后**必须删掉**，否则手册教一个已不存在的坑。
