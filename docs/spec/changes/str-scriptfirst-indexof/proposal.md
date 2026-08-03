# Proposal: Script-First 字符串搜索——char[] view 原语 + 脚本 IndexOf（B）

> 总纲 improve-stdlib-org-perf B 轴。类型：**vm**（新 corelib builtin）+ stdlib。
> 实现 perf-vm-iteration Phase 5「native str builtin」——但以 **Script-First** 形态：
> **一个** char[]-view 原语，字符串算法留在**脚本**，而非 per-op native extern（用户定的终极目标）。

## Why（剖析 + 三方实测）

B0 基准 + `sample` 剖析：string-heavy ~96% 时间在解释器逐字符扫描，锁零占比。根因是
`String.IndexOf`/`Contains` 逐字符 `CharAt`——而 `CharAt` 是 **builtin 调用**（查表 + marshaling +
fn 派发），`arr[i]` 是**内联 ArrayGet opcode**。三方同构实测（同一 IndexOf 算法）：

| 实现 | interp | jit |
|---|---:|---:|
| per-char `CharAt`（旧） | 2128ms | 2524ms |
| **char[] 脚本（bulk view + ArrayGet）** | **237ms (9×)** | **185ms** |
| native `str::find`（per-op extern） | 2ms | 1ms |

native 最快但是 per-op extern（**否决**：终极目标是不 per-op 依赖 native）。char[] 脚本 9×、
**只需一个 view 原语**、算法全脚本——选它。真实 `String.IndexOf` 落地后实测 **248ms（8.6×）**，
正确性含 UTF-8 scalar（`"héllo".IndexOf("llo")==2`、`"日本語テスト".IndexOf("テスト")==3`）。

## 设计前提（已与用户定案）

- **String 表示不变**：UTF-8 `Arc<str>`，**不重写 UTF-16/32**（省 2-4× 内存 + 代理对复杂度）。
- **双索引，scalar 默认**（像 Rust）：`Length`/`CharAt`/`IndexOf` 按 Unicode scalar（`"😀".Length==1`）；
  byte 视图（`ByteLength` 已有）供 byte-语义快路。本 change 走 **scalar** 语义。
- **数组 packed 布局**：实测仅 ~1.35×（Vec<char> vs 当前 Vec<Value>），而耦合面 123 处——**边际、暂不做**
  （大头是解释器派发 57×，非布局）。见 design「已否决/延后」。

## What Changes

- **VM 新增 bulk builtin** `__str_to_chars(this) -> char[]`（一次调用物化整个 scalar char[]）——
  **the ONE array-view 原语**（C#/Rust「string ops over char buffer」模型）。
- **`z42.core/String.z42`**：`ToCharArray()` 从 per-char `CharAt` 循环改为 `[Native("__str_to_chars")]`
  bulk（顺带提速所有 ToCharArray 调用方）；`IndexOf` 改为 char[] 脚本 over `ToCharArray()`；`Contains`
  不变（`IndexOf>=0` 自动获益）。

## Scope

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/runtime/src/corelib/string.rs` | MODIFY | `builtin_str_to_chars`（bulk scalar char[]）|
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 `__str_to_chars` |
| `src/runtime/src/corelib/string_tests.rs` | NEW | `__str_to_chars` 单测（scalar + 空）|
| `src/libraries/z42.core/src/String.z42` | MODIFY | `ToCharArray`→bulk native；`IndexOf`→char[] 脚本 |
| `src/libraries/z42.core/tests/…` | NEW/MODIFY | IndexOf/Contains 行为回归（ASCII + UTF-8）|
| `bench/results/MODE-COMPARISON.md` | MODIFY | before/after |
| `docs/spec/changes/perf-vm-iteration/tasks.md` | MODIFY | Phase 5 勾选（Script-First 形态）|

## Out of Scope
- 数组 packed 布局重写（实测边际 + 123 处，延后/暂不做）。
- StartsWith/EndsWith/Replace/Split 转 char[]（同款机械 follow-up，本 change 只做 IndexOf）。
- byte-index 快路 API（后续按需）。
- native per-op str builtin（已否决）。

## Open Questions
- [ ] 无（bootstrap：新 builtin 不需两-nightly，fresh z42vm cargo-first 必含——见 design）。
