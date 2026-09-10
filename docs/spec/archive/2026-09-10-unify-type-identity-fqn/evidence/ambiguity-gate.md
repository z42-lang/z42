# 跨 ns 同短名类型身份门（退回对照实测）

fixture 见 `ambiguity-fixture/`：`Alpha.Widget` 与 `Beta.Widget` 同短名分居两 ns；
`Both` 用**限定**拼写引用两者，`HolderA`/`HolderB` 各自在自己 ns 里用**非限定**拼写引用。

| 字段 | 修前（526acb72） | 只加 FQN 发射（无 A1） | 最终（含 A1 作用域修复） | 期望 |
|---|---|---|---|---|
| `Both.fromAlpha` | `unknown` | `Alpha.Widget` | `Alpha.Widget` | `Alpha.Widget` |
| `Both.fromBeta`  | `unknown` | `Beta.Widget`  | `Beta.Widget`  | `Beta.Widget` |
| `HolderA.w`      | `Widget`（无句柄合成） | 🔴 **`Beta.Widget`** | `Alpha.Widget` | `Alpha.Widget` |
| `HolderB.w`      | `Widget`（无句柄合成） | `Beta.Widget` | `Beta.Widget` | `Beta.Widget` |

三条结论：

1. **修前**两个不同 ns 的 `Widget` 在运行期退化成**同一个**无句柄合成类型 —— 「短名不是跨 ns
   唯一键」的直接证据，也是本 change 的立论依据。
2. **中间态**（只把发射端改成 FQN、还没修裸名解析）把「诚实的降级」变成了「自信的错答案」。
   这一格是本门存在的理由：**没有它，这个回归会静默进 main**。
3. 该门**判别力已验证**：修前跑同一 fixture 得到的是完全不同的输出（第 2 列），不是空门。
