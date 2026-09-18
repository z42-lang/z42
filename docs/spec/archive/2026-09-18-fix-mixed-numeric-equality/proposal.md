# 整数与浮点的 `==` / `!=` 恒假 / 恒真

> 类型：fix（轻量变更，无需规范先行——这是既有语义的实现缺陷，不是新语义）

## 现象

操作数一边是整型、一边是浮点型时，`==` **永远返回 `false`**，`!=` **永远返回 `true`**，
与实际数值无关。同样的操作数，`<` `<=` `>` `>=` **全部正确**。

```z42
int i = 5; double d = 5.0;
i == d          // false  ← 错
i != d          // true   ← 错
i <= d          // true   ← 对
i <  d          // false  ← 对
i + d           // 10     ← 对（算术加宽正常）
(double)i == d  // true   ← 显式转换可绕开
```

覆盖 `int`/`long`/`byte` × `float`/`double`，两个方向，字面量与变量都一样。

## 根因（VM，一个函数）

`src/runtime/src/semantics.rs:94-103`：

```rust
CmpOp::Lt => numeric_lt(va, vb)?,   // numeric_lt 有 (F64,I64)/(I64,F64) 加宽臂
CmpOp::Le => !numeric_lt(vb, va)?,
CmpOp::Gt => numeric_lt(vb, va)?,
CmpOp::Ge => !numeric_lt(va, vb)?,
CmpOp::Eq => va == vb,              // ← 走 Value 的 PartialEq，不加宽
CmpOp::Ne => va != vb,
```

`impl PartialEq for Value`（`src/runtime/src/metadata/types/value.rs:339-395`）按变体配对：
`(I64,I64)`、`(F64,F64)` 各有臂，**没有混合臂**，落到 `_ => false`（`:393`）。

**不是编译器的问题。** IR dump 显示 `eq` 与 `le` 形状完全一致，两者都拿到裸 `I64` + 裸 `F64`：

```
%4  = eq i32 %1, %3      ← == ：无 convert
%7  = le i32 %1, %3      ← <= ：无 convert，形状相同
%14 = eq f64 %13, %3     ← 显式 cast 后才有 convert
```

编译器对**所有**二元运算符都不插 convert，一律交给 VM 运行期加宽
（算术 `semantics::int_binop`、关系 `numeric_lt`）。只有相等那条臂漏了加宽。

三条执行路径（interp / 融合 `CmpBr` / JIT 回退）全部汇到同一个 `eval_cmp`，改一处即可。
JIT 只在 `reg_types` 证明两侧同类时才内联，混合操作数必然走 helper。

## 波及面（实测）

- `double x` 写 `while (x != 3)` **永不终止**。
- `Assert.Equal(5, d)` 抛 `Std.TestFailure`——读者最可能用来自测的那个断言本身是坏的。
- `object oi = i; object od = d; oi == od` → `false`。

## 修法：加宽后比较

不采用「编译期拒绝」：`conversions.md:51` 明确把 `int → double` 列为**隐式**转换，且关系运算符
对同一组操作数已经在加宽——拒绝 `i == d` 却接受 `i < d` 自相矛盾，且会破坏现有代码。

```rust
fn numeric_eq(va: &Value, vb: &Value) -> bool {
    match (va, vb) {
        (Value::F64(x), Value::I64(y)) => *x == (*y as f64),
        (Value::I64(x), Value::F64(y)) => (*x as f64) == *y,
        (Value::Char(x), Value::I64(y)) => (*x as u32 as i64) == *y,
        (Value::I64(x), Value::Char(y)) => *x == (*y as u32 as i64),
        _ => va == vb,
    }
}
// CmpOp::Eq => numeric_eq(va, vb),   CmpOp::Ne => !numeric_eq(va, vb),
```

三点必须照做：

1. **改 `eval_cmp`，不改 `Value::PartialEq`。** 后者还服务于哈希表查键、`List.Contains`、
   模式匹配；让 `I64(5) == F64(5.0)` 在那里为真而不同步改 `Hash`，会破坏 hash/eq 不变式。
2. **`Ne` 必须写成 `!numeric_eq(...)`**，不能另写 `!=`——否则破坏 NaN 规则
   （`semantics.rs:26`：`Ne` 是 unordered，`NaN != NaN` 为 `true`）。`!(NaN == NaN)` 仍是 `true`，
   既有测试 `semantics_tests.rs:234-237` 保持绿。
3. **装箱路径是另一件事。** `Assert.Equal(5, d)` 经 `BoxedStruct` 逐字节比较
   （`value.rs:376-385`），`eval_cmp` 根本看不到。C# 在那里也是 `false`
   （`((object)5).Equals((object)5.0)` 为 false），**建议单独裁决、本 change 不动**——
   但要注意：不动它，就不能用 `Assert.Equal` 来断言本次修复。

## 测试缺口（为什么写了测试也没抓到）

- `src/tests/operators/comparison_operators.z42` 是专门的比较 golden，**每一条都是同类比较**
  （int/int、double/double、bool/bool、string/string），没有一行混合。
- `src/tests/types/type_conversions.z42` 标题就叫「numeric promotion」，本该抓到——但所有
  promotion 断言都写成**经关系运算符的区间检查**：
  ```z42
  var sum = 5 + 2.5;
  Assert.True(sum > 7.4); Assert.True(sum < 7.6);   // 从不写 Assert.Equal(7.5, sum)
  ```
  整个「提升」测试只走了**好的那条路径**。
- Rust 侧 `semantics_tests.rs` 只有一个 `eval_cmp` 测试（NaN，F64×F64）。
- JIT 差分测试按构造抓不到：混合操作数永远不进内联路径。

补：(1) `src/tests/operators/` 加 golden，覆盖 `int==double` / `long==double` / `byte==double` /
`double==int 字面量` / 各自的 `!=` / `while (x != 3)` 终止；(2) `semantics_tests.rs` 补
`eval_cmp` 的四种跨类顺序 × `Eq`/`Ne`。

## 文档

- `docs/reference/src/language/operators.md` **没有**「二元数值提升」一节（现在只讲优先级、
  短路、位运算整型要求、复合赋值），需要补——这是用户可见行为却无规范页。
- `semantics.rs:18-30` 的模块头表格加一行加宽规则。
