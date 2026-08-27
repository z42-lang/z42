# Proposal: 收缩 primitive 协议 native interop（9 个 builtin → 脚本）

## Why

extern 审计 Wave 0–3（2026-04）已把 assert/bool/math/path/split/join/concat/format 等 38 个
builtin 迁成纯脚本。但当时把 `int/double/char` 的 `Equals`/`GetHashCode` 连同 `char.ToLower`/
`ToUpper`、`str.ToString` 一并归入「🟢 必须保留 · Object 协议 ABI」（见 `src/libraries/README.md`
「Primitive 协议」表），**分类标准是错的**：

- 判据应是「native 实现是否平凡 / 纯脚本能否等价表达」，而非「是否 Object 协议成员」。
  同属协议的 `bool.Equals`/`GetHashCode` 早在 `wave1-bool-script` 就迁成脚本了。
- 审计后逐个核对这 9 个 builtin 的 Rust 实现，全部平凡或名不副实：
  - `__int32_equals` = `a == b`；`__int32_hash_code` = 恒等返回；`__char_*` 同理。
  - `__char_to_lower`/`__char_to_upper` 实为 **`to_ascii_lowercase`/`to_ascii_uppercase`**（纯 ASCII），
    README 却标注「Unicode 分类表 native」——描述与实现不符。
  - `__str_to_string` 实为原样返回自身（`s.to_owned()`）。

保留它们只是增大可审计的 native ABI 面、无收益。收缩后 primitive-protocol builtin 从 ~43 → ~34，
与 bool 的既定处置保持一致。

顺带修一个潜在 bug：`Int64`/`UInt64.GetHashCode` 现走 `__int32_hash_code`，返回完整 i64 当作
`int` 型 hash（未截断/未折叠）。迁脚本时按 C# 语义折叠为 `(int)(v ^ (v >> 32))`。

## 两阶段（bootstrap-seed 纪律，必须遵守）

删除这 9 个 builtin **不能与源迁移同一 PR 做**——已发布的 nightly 种子 z42c.zpkg 内部
（Dictionary<int>、hashing、字符串等）编译期就引用 `__int32_equals` 等 builtin。零格式-bump 时
`ci-bootstrap` 的版本差 gate（`seed_minor == cur_minor`）**跳过两代自举、当前 VM 直接跑下载的旧
种子 z42c** → 若当前 VM 已删 builtin 则 `unknown builtin __int32_equals` panic（实测 PR #310 首版
CI 全红，`resolver.rs:392`）。故按 bootstrap-seed.md「support 先行、use 晚一 nightly」拆两阶段：

- **Stage 1（本 PR）**：只把 z42 源里 primitive 协议方法从 `[Native]` extern 迁为脚本体；**9 个
  builtin 在 VM 侧原样保留**。结果：新编译的 zpkg（含新 z42c）不再引用这些 builtin，但 VM 仍有它们
  → 旧种子仍能加载。跑完整 GREEN、合并、随 nightly 发布。
- **Stage 2（follow-up PR，等 Stage 1 进 nightly 后）**：种子 z42c 不再引用这些 builtin，此时删
  Rust 实现 + 派发登记。

## What Changes（本 PR = Stage 1）

- **9 个 primitive 协议方法源迁脚本**（z42 侧 `[Native]` extern → 脚本方法体）：
  `Int32/Int16/SByte/Byte/UInt16/UInt32/Int64/UInt64/Double/Single/Char` 的 `Equals`/`GetHashCode`、
  `Char.ToLower`/`ToUpper`、`String.ToString`。对应旧 builtin：`__int32_equals`、`__int32_hash_code`、
  `__double_equals`、`__double_hash_code`、`__char_equals`、`__char_hash_code`、`__char_to_lower`、
  `__char_to_upper`、`__str_to_string`。
- **VM 侧 9 个 builtin 保留不动**（Stage 2 才删）。
- **保留 native**（纯脚本做不了或不划算，本 change 不动）：所有 `*ToString`（数值/浮点格式化）、
  所有 `*Parse`、`__char_is_whitespace`（真 Unicode）、`__str_equals`（类型宽容 `Equals(object?)`）、
  String UTF-8 intrinsic、Math libm、BitConverter、Object 协议 5 件套、Type/反射、GC、Enum、
  delegate、Clock/OS/Platform，以及 io/net/threading/compression 全部（真 syscall/原生库）。
- **账本纠正**：`src/libraries/README.md`「Primitive 协议」表把这 9 项标为「已删（脚本）」，
  修正 char casing 的「Unicode native」错误描述，更新汇总数字。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Primitives/Int32.z42` | MODIFY | Equals/GetHashCode extern → 脚本 |
| `src/libraries/z42.core/src/Primitives/Int64.z42` | MODIFY | 同上 + hash 折叠 |
| `src/libraries/z42.core/src/Primitives/Int16.z42` | MODIFY | 同 Int32 |
| `src/libraries/z42.core/src/Primitives/SByte.z42` | MODIFY | 同 Int32 |
| `src/libraries/z42.core/src/Primitives/Byte.z42` | MODIFY | 同 Int32 |
| `src/libraries/z42.core/src/Primitives/UInt16.z42` | MODIFY | 同 Int32 |
| `src/libraries/z42.core/src/Primitives/UInt32.z42` | MODIFY | 同 Int32 |
| `src/libraries/z42.core/src/Primitives/UInt64.z42` | MODIFY | 同 Int64（hash 折叠） |
| `src/libraries/z42.core/src/Primitives/Double.z42` | MODIFY | Equals + GetHashCode(BitConverter) → 脚本 |
| `src/libraries/z42.core/src/Primitives/Single.z42` | MODIFY | 同 Double（SingleToBits） |
| `src/libraries/z42.core/src/Primitives/Char.z42` | MODIFY | Equals/GetHashCode/ToLower/ToUpper → 脚本 |
| `src/libraries/z42.core/src/String.z42` | MODIFY | ToString extern → `return this;` |
| `src/libraries/z42.core/tests/primitive_protocol_script.z42` | NEW | 迁脚本行为回归测试 |
| `src/libraries/README.md` | MODIFY | Primitive 协议表标 🟡 源迁（builtin 留 Stage 2）+ char casing 描述纠正 |
| `docs/spec/changes/shrink-primitive-native-interop/` | NEW | 本 change 文档 |

> **Stage 1 不改任何 Rust 文件**——`src/runtime/src/corelib/{convert,char,string,mod}.rs` 的 builtin
> 删除是 Stage 2 follow-up 的 Scope，本 PR 保持不动。

**只读引用**：
- `src/libraries/z42.core/src/Primitives/Boolean.z42` — 脚本迁移先例模板
- `src/libraries/z42.core/src/BitConverter.z42` — DoubleToBits/SingleToBits 签名
- `src/runtime/src/corelib/convert.rs`、`char.rs`、`string.rs` — 现 builtin 实现语义

## Out of Scope

- ToString / Parse / `__str_equals` / `__char_is_whitespace` 的迁移（保留 native，见上）
- Double.Equals 的 NaN 语义对齐 C#（现 native 即 `a==b`，脚本保持一致，不改）
- io/net/threading/compression 的任何 builtin（均真 syscall）
- Convert.ToInt32/ToInt64/ToDouble 的接口去重（C# 亦保留 Convert.* 与 X.Parse 双入口）

## Open Questions

- 无（scope 与 hash 语义已与 User 确认：primitives Eq/Hash + double.GetHashCode + long/ulong 折叠 +
  char casing + str.ToString 全部迁；ToString 保留）
