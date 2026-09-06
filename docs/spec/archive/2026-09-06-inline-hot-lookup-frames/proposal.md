# Proposal: 打掉热查找路径上的调用帧 —— StrMap/StrIndex 探测内联 + 父链哈希只算一次 + Murmur3 内环内联

## Why

#501 量出一条可复用的规律：**在解释执行的编译器里，一层纯转发 ≈ 0.5% 全负载**
（`Z42Type.Canon` 一行 `return PrimModel.Canon(n);`，profile 里**自身**就占 0.53%）。
一个 z42 调用帧要建 `VmFrame`（含两次 `Arc<str>` 克隆），远大于这些 helper 的函数体。

按这条规律重扫 #501 之后的 profile，剩下最大的一簇正是这个病：

| 位置 | self% | 函数体 |
|---|---|---|
| `StrMap.Find` | **1.20%** | `return this._find(key);` |
| `StrMap.ContainsKey` | **1.01%** | `return this._find(key) >= 0;` |
| `StrMap._find` | 1.93% | 探测循环（真正干活的） |
| `Std.String.GetHashCode` | 1.93% | 其中 **1.45% 来自 `_find`** |
| `Murmur3.Hash128` | 1.68% | 内环每 16 字节付 **24 个调用帧** |
| `StrIndex.Get` / `_hash` | 0.92% / 0.25% | `_hash` = 一次 `GetHashCode` + 一次掩码 |

整簇（StrMap + StrIndex + GetHashCode）self 合计约 **9.5%**。

另外两处是**重复算哈希**：
- `Std.String.GetHashCode` 是 runtime 里的 **O(n) FNV-1a，每次调用都重算**
  （字符串底层是 `Arc<str>`，没有地方存哈希）。
- `TypeEnv.LookupVar` / `LookupConst` / `LookupLocalFn` 沿**作用域父链逐层** `Find(name)`
  —— 同一个键，哈希白算 k 遍。`AccessEmitter._lookupIdent` 同理（const 局部 / 局部 / 字段三连查）。
- `SymbolTable` 里四处 `ContainsKey(k)` 紧跟 `Get(k)`（两次哈希 + 两次探测），
  而 `StrMap` 的头注释早就写明该用 `Find` + `ValAt` 走一次。

## What Changes

1. **`StrMap`**：`ContainsKey` / `Find` / `Get` 各自**内联**探测循环，删掉共用的 private `_find`；
   `Put` 内联 `_slotFor`；`_grow` 内联 Put（重排期每条都要一个帧）。
2. **`StrMap` 新增 `FindHashed(key, h)` / `ContainsKeyHashed(key, h)`**：哈希由调用方传入，
   供「同一个键连查多张表」。
3. **`StrIndex`**：删 `_hash` 转发层；`%` 整除换成 `& (_cap-1)`（`_cap` 恒为 2 的幂，逐位等值）；
   `_grow` 内联 Put。
4. **`Murmur3.Hash128`**：内环手工内联 `_rd32` / `_mul` / `_rotl`（`_rd32` 内联后无人调用 → 删）。
   尾部与 `_fmix` 频次是每次哈希一次，仍走 helper。
5. **调用点**：`TypeEnv` 三个父链查找 + `AccessEmitter._lookupIdent` 改为「哈希取一次、逐表传」；
   `SymbolTable` 四处 `ContainsKey`+`Get` 改 `Find`+`ValAt`。

## 等价性论证（都不改哈希表布局 / 哈希值）

- 内联是逐字面展开，探测序不变 ⇒ 槽布局不变。
- `StrIndex` 的 `(h & 0x7FFFFFFF) % cap` ≡ `h & (cap-1)`：低位掩码本就落在 `0x7FFFFFFF` 之内，
  清符号位那一步对结果无影响（`StrMap` 早前同样的改动已有此论证）。
- `_grow` 内联 Put：① 重排期 `_count*2 >= _cap` 恒不成立（旧表负载 <0.5、新表容量翻倍）⇒ 不嵌套 `_grow`；
  ② 旧表键互异 ⇒ 探测必停在第一个空槽 ⇒ 与逐条走 `Put` 槽布局相同。
- `Murmur3`：`_rotl(x,r) = ((x<<r)|(x>>(32-r))) & MASK32`、`_mul(x,c) = (x*c) & MASK32` 逐字面展开；
  `(_mul(h,5)+K) & MASK32` 保留内层掩码。41 条参考向量单测守着。

## Scope

| 文件 | 变更 |
|---|---|
| `src/libraries/z42.ir/src/StrMap.z42` | 四个查找入口内联 + `FindHashed` / `ContainsKeyHashed` + `_grow` 内联 |
| `src/libraries/z42.ir/src/StrIndex.z42` | 删 `_hash`、`%`→`&`、`_grow` 内联 |
| `src/libraries/z42.ir/src/Murmur3.z42` | 内环内联，删 `_rd32` |
| `src/compiler/z42c.semantics/src/TypeEnv.z42` | 三个父链查找哈希只算一次 |
| `src/compiler/z42c.semantics/src/AccessEmitter.z42` | `_lookupIdent` 三连查哈希只算一次 |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | 四处 `ContainsKey`+`Get` → `Find`+`ValAt` |
