# Tasks: 打掉热查找路径上的调用帧

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

## 进度概览
- [x] 阶段 1: `StrMap` / `StrIndex` 探测内联 + 哈希入参版
- [x] 阶段 2: `Murmur3` 内环内联
- [x] 阶段 3: 调用点（父链哈希只算一次 + `ContainsKey`/`Get` 成对写法）
- [x] 阶段 4: 验证与量化

## 阶段 1
- [x] 1.1 `StrMap`：`ContainsKey` / `Find` / `Get` 各自内联探测循环，删 private `_find`
- [x] 1.2 `StrMap`：新增 `FindHashed(key,h)` / `ContainsKeyHashed(key,h)`（同样内联，不转发）
- [x] 1.3 `StrMap`：`Put` 内联 `_slotFor`（删）；`_grow` 内联 Put（带等价性论证注释）
- [x] 1.4 `StrIndex`：删 `_hash` 转发层；`%` → `& (_cap-1)`；`_grow` 内联 Put

## 阶段 2
- [x] 2.1 `Murmur3.Hash128` 内环内联 `_rd32` / `_mul` / `_rotl`
- [x] 2.2 `_rd32` 内联后无调用方 → 删（不留死代码）
- [x] 2.3 41 条参考向量单测（`z42.ir/tests/murmur3.z42`）首跑即过

## 阶段 3
- [x] 3.1 `TypeEnv.LookupVar` / `LookupConst` / `LookupLocalFn`：父链外取一次哈希 → `FindHashed`
- [x] 3.2 `AccessEmitter._lookupIdent`：三连查取一次哈希
- [x] 3.3 `SymbolTable` 四处 `ContainsKey`+`Get` → `Find`+`ValAt`
      （继承链走查 / `InterfaceDerivesFrom` / 接口闭包展开 / 类型别名替换）

## 阶段 4
- [x] 4.1 **验收门 1（产物逐字节）**：两侧 driver+libs 各编一次 `src/libraries/z42.net`
      → `.zpkg` `963fc176…`、`.zsym` `328d1785…` 完全相同
- [x] 4.2 **验收门 2**：`xtask test compiler` → `3/3 packages gen1==gen2`
- [x] 4.3 **验收门 3**：`xtask test` → `✅ GREEN — all stages passed`
- [x] 4.4 **验收门 4**：同机交错 A/B → 指令 66.271 G → **64.291 G（−2.99%）**、
      峰值 RSS 994.1 → 990.4 MB（−0.37%）

## 没做（留档）
- `StrMap` 加 `int[] _hashes`（原队列里候选 2 的 script 侧写法）：**本次刻意不做**。
  它只省「探测冲突时的串比较」和 `_grow` 的重算，**不省**那一次 `GetHashCode`；
  而每张表要多一个 `int[]`（每个 `Z42ClassType` 就带三张 StrMap）⇒ RSS 是净增。
  真正的大头是帧成本，已由本变更拿掉。
- 把哈希缓存进字符串对象（`Arc<StrInner{hash, s}>`）：runtime 侧改 `Value::Str` 表示，
  波及面大且与 script-first 的历史裁决相邻 —— **仍待 User 裁决**。
