# Design: 硬转换 `(T)x` 失败时正确报错

## Architecture

```
语义层                             发射层                          运行期
─────────                         ─────────                       ─────────
(T)x  → BoundCast{IsHardCast=1} ─→ ① IsInst  x, T                 isa_td / prim_isa
x as T → BoundCast{IsHardCast=0}    ② 假 → 抛（异常按下表选）        （与 as 同一判定）
                                    ③ AsCast x, T（不变）
```

`as` 的路径**一条指令都不变**（`AsCast`）。只有硬转换多出「先查」的前缀。
全部用现有指令 ⇒ **零格式 bump**。

---

## Decisions

### D1：在编译期降解，不新增 IR 指令

**选项：**

| | 做法 | 代价 |
|---|---|---|
| A | 新增 `HardCastInstr` opcode，运行期抛 | 格式 bump（zbc + zpkg + 9 步 + fixture 重生 + CI artifact overlay） |
| B | `AsCastInstr` 加一个 flag 字段 | 同样是 wire 变更 ⇒ 同样 bump |
| **C（选定）** | 编译期降解成 `IsInst` + 分支 + `Throw` | 零 bump；多两三条指令 |

C 的代价是每个硬转换点多几条指令。可接受：硬转换在热路径上本就不常见，且
`IrDeadBranch` / 常量折叠对静态可判的情形（源类型即目标类型）能消掉整段——
**静态类型已等于目标时编译期直接省略检查**（见 D4）。

### D2：异常的选择 —— 对齐 C#

| `x` | 目标 `T` | 结果 |
|---|---|---|
| null | 值类型 | `NullReferenceException` |
| null | 引用类型 | **不抛**，结果是 null |
| 非 null，`x is T` | — | 正常转换 |
| 非 null，`!(x is T)` | — | `InvalidCastException` |

第二行是 `is` 与 `as_cast` 唯一的语义分歧点（`as_cast` 有 `Value::Null => true`，`is` 对 null 返假），
必须显式处理，否则 `(string)nullObj` 会被误抛。

### D3：`InvalidCastException` 是新类

`src/libraries/z42.core/src/Exceptions/` 下 18 个异常类里没有它。按同形补一个。

⚠️ 新增 stdlib 公开类 ⇒ 元数据变 ⇒ 按 version-bumping 的「编译器语义指纹」
**`CacheStore.CompilerFingerprint++`**（编出的 zpkg 字节变、格式不变）。

### D4：静态可判时省略检查

`(T)x` 中若 `x` 的静态类型**已经是** `T` 或 `T` 的子类型 ⇒ 转换恒成功 ⇒ 不发检查。
这一条既省指令，也避免给现存代码里大量「形式上的 cast」（`(int)someInt`）加无谓开销。

判据用既有的 `Z42Type.IsAssignableTo` / `Conversion.Classify(...).Kind == Identity`。

### D5：运行期两条 `bail!` 改真异常

`semantics.rs` 的数值转换路径在源不是数值时 `bail!("InvalidCastException: …")`：
内部错误、不可 catch、消息是 Rust Debug 格式（`Str("hello")` / `type tag 0x04`）。

改为构造真异常。⚠️ 这条**独立于** D1——即便编译期检查挡住了大部分，
反射 / 泛型擦除路径仍可能走到它，不能只靠编译期。

---

## Risks

| 风险 | 缓解 |
|---|---|
| 误抛（`is` 与 `as` 判定不一致） | Q1 已按构造排除（同一个 `isa_td`）；null 那条单独处理 |
| 现存代码里有依赖「硬转换不抛」的地方 | **摸底**：先只加检查跑全仓，看命中；命中即是真 bug 或需改写的地方 |
| JIT 另有 cast 快路 | 先 grep 确认；有则同步，并用 `--mode jit` 用例双验 |
| 指令数增长影响 golden | 静态可判时省略（D4）⇒ 大部分 cast 不变；仍需核对 golden diff |
