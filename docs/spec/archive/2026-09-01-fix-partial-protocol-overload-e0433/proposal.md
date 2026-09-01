# Proposal: partial 类型协议豁免重载误报 E0433 修复

> 类型：fix（编译器 / 语义收集诊断） | 子系统：compiler | 创建：2026-09-01
> 分支/worktree：`fix-partial-protocol-overload-e0433`（基于 origin/main `696d41e`）

## What / Why

`partial` 类型的「重复成员」检测（`MemberCollector._fillClass`）此前按 **RegKey**（`regName`）判重：
`ct.IsPartial && ct.Methods.ContainsKey(regName)` → 报 `E0433`。但**协议豁免方法**
（`ToString` / `Equals` / `GetHashCode` / `GetType` / `get_Item` / `set_Item`，见
`SymbolCollector.IsProtocolExempt`）的 RegKey 恒为**裸名**——于是 `Equals(object?)` 与
`Equals(string)` 都注册为 `"Equals"`，第二个被误判为「跨/同碎片重复成员」→ 假 E0433。

- **非 partial 类型不受影响**：该检查仅在 `ct.IsPartial` 时执行；非 partial 时两 Equals 直接
  `Put("Equals")` 后写覆盖前（last-wins），因协议方法走 VM vtable 裸名派发、且 String 两 Equals
  同 `__str_equals` 实现，覆盖无害——这是既有且正确的语义。
- **latent bug**：现有 partial 类型无一声明协议豁免方法的重载（grep 证实），故此假阳性从未被触发。

**动机**：stdlib 要把 prelude `Std.String`（已含 `Equals(object?)`/`Equals(string)`）拆成 `partial`
以在 500 行/文件硬限内补齐 char-based BCL 方法（`IndexOf(char)`/`LastIndexOf`/`Split(char[])`/
`Trim(char[])`/`Insert`/`Remove`/`Pad*`，library_review.md §109）。partial 化立刻撞此 E0433。本
change 是该 String 补齐（阶段 2，晚一个 nightly 的独立 change）的**前置 support**。

## 修法

`MemberCollector._fillClass` 的方法重复检测改为按**完整签名**判重，而非 RegKey：碰撞 RegKey 时
比较已注册符号与新符号的完整参数签名（`_sameSignature`：参数个数 + 各参数 canonical 类型名逐一比较）；
**仅签名相同**才报 E0433（真重复）；签名不同的合法重载（协议豁免裸键 / 其它共享 RegKey 的重载）放行，
`Put` 覆盖，与非-partial 的 last-wins 语义一致。字段重复检测（`E0433` 字段侧）不变。

## 自举 / 两-nightly

- z42c 自身源码**不使用** partial 类型的协议豁免重载 → 本 change 对 z42c self-build **字节中性**
  （gen1==gen2 硬门验证）。ships nightly 后，String 补齐（阶段 2）才能 partial 化。
- 自举能力版本号：**不 bump**（未新增语法/格式；靠 `xtask test bootstrap` 实编验越界）。

## Scope（改动文件）

- `src/compiler/z42c.semantics/src/MemberCollector.z42`：方法重复检测改全签名判重 + `_sameSignature` helper。
- `src/compiler/z42c.semantics/tests/collect/collect_tests.z42`：+2 collect 测试（协议重载放行 / 真重复仍报）。
- `src/tests/partial-types/partial_protocol_overload.z42`：+1 e2e（partial 类协议重载共存 + 非协议 type-based 重载派发）。
- `docs/book/src/language/partial-types.md`：E0433 语义精确化（全签名判重 + 协议重载可共存）。

## 验证

`xtask test` 全绿 + gen1==gen2 3/3 逐字节 + `xtask test bootstrap` 无越界。
