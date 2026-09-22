# Proposal: 窄基元实例方法的运行期不可达

## Why

**`sbyte` / `short` / `byte` / `ushort` / `uint` / `ulong` / `long` / `float` 这 8 个包装类型上
声明的每一个实例方法，运行期一个都到不了。**

机制：`vcall()` **不携带接收者的编译期类型名**，prim 接收者的类由 `primitive_class_name(obj_val)`
**从运行期 `Value` 反推**（`interp/exec_vcall.rs`）：

```rust
Value::I64(_)  => Some(STD_INT32),     // sbyte/short/byte/ushort/uint/ulong/long 全塌到这里
Value::F64(_)  => Some(STD_DOUBLE),    // float 塌到这里
```

于是 `ulong.ToString()` 解析到 `Std.Int32.ToString`，`float.GetHashCode()` 解析到
`Std.Double.GetHashCode`。`exec_vcall.rs` 原注释写着「narrow int / long values are tagged with
class FQN at compile-time in VCall instructions」——**与 `vcall()` 的实际签名不符，是过期陈述**。

### 为什么长期没人发现

这些类型的 `Equals` / `GetHashCode` / `CompareTo` 实现与 `Int32` / `Double` 版**恰好等价**，
所以派发到"错"的类也看不出差别。实测真正分道的有两处（修复前后对拍）：

| 表达式 | 修复前 | 修复后 |
|---|---|---|
| `UInt64.Parse("9223372036854775808").ToString()` | `-9223372036854775808` | `9223372036854775808` |
| `UInt64.Parse("18446744073709551615").ToString()` | `-1` | `18446744073709551615` |
| `(1.5f).GetHashCode()` | `1073217536`（= `double` 的 64 位折叠） | `1069547520`（= `SingleToBits(1.5)`） |
| `(1.5).GetHashCode()` | `1073217536` | `1073217536`（对照组，未变） |

属 [[silent-feature-masks-other-bugs]] 同族：**声明了却从不执行的代码，会一直看起来是对的**。

## What Changes

1. **编译期静态派发**（`CallEmitter` 实例路径新增一条分支）：接收者静态类型属于上述 8 个时，
   发 `Call Std.<Wrapper>.<method>` 而非 `VCall`。基元是 **sealed 值类型、没有虚派发**，编译期
   直呼语义上就是对的——emitter 里早有同款先例（blob 值 struct 的实例调用就走静态 `Call`，
   理由一字不差）。
2. **判据与目标解析**收在 `EmitContext.NarrowPrimTarget`：先 `PrimModel.Canon` 归一，再查
   `LocalClasses` / `Deps.Statics` 校验目标**确为已发射函数**；解析不出即返回 `""` → 落回 VCall
   （= 今天的行为），**永不 miscall**。
3. **`UInt64.ToString` 改绑专用 builtin `__uint64_to_string`**（按 u64 重解释再格式化）。
   这一条在 `symmetrize-primitive-api` 里曾被撤回——当时发现"只加 builtin 没用"，真因就是本变更
   要修的派发洞；现在前置条件齐了才真正生效。

### 为什么不给 VCall 加接收者类型字段

那要再吃一次 zbc/zpkg 格式 bump，运行期还多一次查表。而基元没有虚派发，编译期就能定死——
静态派发是更小也更正确的解。

## 不做（本 PR）

- `int` / `double` / `bool` / `char` / `string` 的派发**不动**：运行期反推本就正确，改了只会带来
  字节漂移，还要碰 runtime 对 string / 装箱基元的既有特殊处理。
- 不改 `vcall()` 签名、不动 wire 格式。**零格式 bump。**

## 行为变化（需要认清的一处）

`float.GetHashCode()` 的返回值变了（从 Double 的 64 位折叠 → Single 的 32 位位模式）。
**不违反哈希契约**（等值仍等哈希，且同一进程内稳定），但 `Dictionary<float,…>` 的桶分布与之前
不同。这正是修复的意义：`Single.z42` 声明的实现终于真的在跑。

## 验证

- 修复前后用**同一个探针**对拍（见上表），四种接收者形态逐一覆盖
- `xtask test` 全 stage 绿；`cargo test` 1447 passed / 0 failed
- 新测试含**退回对照**：`double` 哈希不变、窄整型其余实例方法（Equals/CompareTo/ToString）语义不变、
  `Single` 与 `Double` 的哈希现在必须**不同**（塌回同一实现就会红）

## 顺带带走的归档

`drop-short-primitive-aliases`（#730）与 `symmetrize-primitive-api`（#736）都已合并，但归档没随
各自 PR 落地（违反 workflow 阶段 9 铁律）。规则同时禁止事后单独推一个 `docs: 归档`，故按 User
裁决**搭本 PR 一起带走**：两个目录移入 `docs/spec/archive/2026-09-22-*`，tasks.md 状态转终态。
