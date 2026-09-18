# 装箱 null 的可空值类型抛内部错误 `__box_prim`

> 类型：fix（轻量变更）

## 现象

值为 `null` 的可空值类型一旦被装箱（赋给 `object`、传给取 `object` 的重载、
`Console.WriteLine(n)` 走 `WriteLine(object)`），运行期抛一条**内部实现名**：

```
Error: uncaught exception: Std.Exception: __box_prim: expected integer value, got Null
```

```z42
int? n = null;
object o = n;              // ✗ 抛 __box_prim
Console.WriteLine(n);      // ✗ 同上（重载选中 WriteLine(object)）
Console.WriteLine($"{n}"); // ✓ 打印 "null" —— 插值路径早就是对的
Console.WriteLine(n ?? -1);// ✓ -1
```

同一件事在**插值**下正常、在**装箱**下抛内部错误，口径不一致。

## 为什么这是 bug 而不是「用户用错了」

z42 的 `?` 是**纯标注、类型解析期擦除**（见 `reference/language/types.md` 的可空标记）。
擦除是彻底的——下面这行能编能跑：

```z42
int x = null;              // 编译通过，运行通过
```

所以「int 槽里装着 `Null`」是**语言明确允许的状态**，不是 VM 不变量被破坏。
既然允许它存在，就必须定义它被装箱时的行为，而不是漏到一条内部错误里。

对照 C#：`int? n = null; object o = n;` 得到 **`o == null`**，不是装箱的 0、也不是异常。

## 根因

`src/runtime/src/corelib/convert.rs:22-25`：

```rust
let raw = match inner {
    Value::I64(n) => *n,
    other => bail!("__box_prim: expected integer value, got {:?}", other),
};
```

编译器在 prim→object 转换点无条件发 `__box_prim(%value, "Std.Int32")`；`%value` 是 `Null`
时落到 `bail!`。

## 修法

`builtin_box_prim` 在取 `raw` 之前加一条：`Value::Null` 原样返回 `Value::Null`。

```rust
// 装箱一个 null 的可空值类型 → null 引用（对齐 C#：`int? n = null; object o = n;` 得 o == null）。
if matches!(inner, Value::Null) {
    return Ok(Value::Null);
}
```

放在幂等分支旁边，与「已是 BoxedStruct 则原样返」同一层。

## 取舍：会不会把真 bug 也吞掉

会少一处噪声告警。`exec_array.rs:75` 的注释记着另一桩历史事故——泛型数组未写槽位读出 `Null`，
当时正是靠这条 `__box_prim: got Null` 暴露的（已由 `fix-generic-array-value-zero-init`
在**源头**修掉：未写槽位现在填值类型零值，不再是 Null）。

判断：**这条错误不是一道有效防线**。因为 `?` 完全擦除，VM 在装箱点根本无法区分
「用户把 null 赋给了 int?」（合法，且常见）与「读到未初始化槽位」（bug）——两者在
`Value` 层面完全相同。用一条分不清好坏的信号去挡 bug，代价是每个正常用 `int?` 的人
都撞上一条内部实现名。

未初始化槽位这类真 bug 仍会在**后续使用点**暴露（拆箱、算术等；例如 `x + 1` 现在报
`type mismatch in arithmetic: Null vs I64(1)`，信息量不比 `__box_prim` 少）。

## Scope

| 文件 | 变更 | 说明 |
|---|---|---|
| `src/runtime/src/corelib/convert.rs` | MODIFY | `builtin_box_prim` 加 Null 分支 |
| `src/tests/types/box_null_nullable.z42` | NEW | golden |
| `docs/reference/src/language/types.md` | MODIFY | 「可空标记」补：null 值类型装箱得 null |

## Out of Scope

- **真正的可空值类型**（`Nullable<T>` 语义、静态空安全检查）—— 那是语言特性，
  不是本 fix 的范围。本 change 只是让「已经允许存在的状态」有一个定义好的装箱行为。
- `n?.ToString()` 在 `int?` 上报 `E0402: unsupported call form` —— 另一条缺口，另记。
