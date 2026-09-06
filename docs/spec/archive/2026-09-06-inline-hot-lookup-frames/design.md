# Design: 打掉热查找路径上的调用帧

## 1. 为什么内联「重复循环文本」是对的

z42c 是解释执行的。一个 z42 调用帧要建 `VmFrame`（含两次 `Arc<str>` 克隆用于栈回溯），
成本与被调函数的**函数体大小无关**。于是出现一个反常识的结论：

> **函数体越小的 helper，被调用得越频繁，帧开销占比就越接近 100%。**

`StrMap.Find` 的函数体是 `return this._find(key);` —— 100% 是帧。profile 实测它**自身**
占全负载 1.20%，`ContainsKey` 另占 1.01%。这两个数字不是"查找慢"，是"转发贵"。

因此本变更刻意**接受循环文本重复**（`ContainsKey` / `Find` / `FindHashed` /
`ContainsKeyHashed` / `Get` 五份几乎相同的探测循环），换掉每次查找一整个帧。
这是与常规「消除重复」相反的取舍，理由写在代码注释里，避免后人"顺手重构回去"。

## 2. 哈希只算一次

`Std.String.GetHashCode` 在 runtime 里是 **O(n) FNV-1a 逐字节，每次调用都重算** ——
字符串是 `Value::Str(Str)`、底层 `Arc<str>`，**没有存哈希的地方**（要存得改 `Value::Str`
的表示，那是 runtime 侧的大改，仍待裁决，见 [[z42-perf-analysis-2026-09]] 候选 2 的第二个角度）。

script 侧能做的是**别重复问**。两个形态：

- **父链走查**（`TypeEnv.LookupVar` / `LookupConst` / `LookupLocalFn`）：
  逐层 `e.Vars.Find(name)`，同一个键、k 层就算 k 遍。改成外层取一次 `name.GetHashCode()`，
  逐表调 `FindHashed(name, h)`。
- **一键多表**（`AccessEmitter._lookupIdent`）：const 局部 / 局部 / 字段三张表连查同一个名字。同上。

`FindHashed` 只跳过哈希计算，**探测与比较逻辑逐字相同** ⇒ 结果恒等。

## 3. `ContainsKey` + `Get` 成对写法

`StrMap` 头注释早就写明：`ContainsKey(k)` 紧跟 `Get(k)` 要走两次哈希 + 两次探测，
应改用 `Find` 一次拿槽号、再 `ValAt` 取值。`SymbolTable` 里还剩四处没跟上（继承链走查、
接口派生判定、接口闭包展开、类型别名替换），本次补齐。

## 4. Murmur3 内环

每 16 字节的块要付 **24 个调用帧**：4 次 `_rd32` + 每 lane（`_mul`,`_rotl`,`_mul`）×4 +
4 次 `_rotl(h,·)` + 4 次 `_mul(h,5)`。BLID 要把整个 zpkg 过一遍（800 KB ≈ 5 万块）
⇒ ~120 万帧。内联后内环只剩算术。尾部（每次哈希一次）与 `_fmix` 保持 helper 形态。

## 5. 实测（同机交错 A/B，同一份输入源码，两侧各自配自己的 driver + libs，交替各 4 次）

| | 指令数 | 墙钟 | 峰值 RSS |
|---|---|---|---|
| base `813a8c13` | 66.271 G | 5.628 s | 994.1 MB |
| 本变更 | **64.291 G（−2.99%）** | 5.55 s（中位，−1.4%） | 990.4 MB（−0.37%） |

指令数跨运行离散度 0.05%。墙钟本机受其它会话干扰（有一次 5.86 的离群），以指令数为准。

**分层归因**：另有一轮 A/B **两侧共用了同一份 z42.ir**（见下「坑」），只测出调用点部分
= **−0.23%**；⇒ z42.ir 三件（StrMap 内联 + StrIndex + Murmur3 内环）≈ **−2.76%**，
是绝对大头。调用点的哈希只算一次收益小，因为父链多数只走一两层就命中。

## 6. ⚠️ 本次踩到的三个坑（下次别重踩）

1. **`Z42_LIBS` 会盖掉 driver dist 里的库** —— driver 的 `dist/` **不含 `z42.ir.zpkg`**，
   它从 `Z42_LIBS` 加载。只换 driver、共用一个 `Z42_LIBS` 做 A/B ⇒ **两侧跑的是同一份 z42.ir**，
   差值静默失真。**A/B 必须两侧各自配 `dist` + `libs` 一对。**
2. **别在后台构建跑着的时候改源码** —— `xtask build all` 会读到改到一半的树，产出混合工具链。
   本次第一轮 base 构建就是这么废掉的（"base" 里混进了 PR 的 semantics）。
3. **给 z42.ir 加新 API 并让 z42c 立刻用，会锁死"回退到 base 源码重建"**：
   装在树上的 PR driver 调 `StrMap.FindHashed`，而 base 版 z42.ir 没有它 ⇒
   `undefined function`，且 driver 要跑起来才能重建 z42.ir ⇒ 死锁。
   **处置：重建前把一份 base driver 重新种进去**，且种子必须覆盖**两处**：
   `artifacts/build/compiler/z42c.driver/release/dist/`（bundle）**和**
   `artifacts/build/compiler/z42c.<pkg>/release/dist/`（每包各自的 dist，也会被拾起）。
   只换前者不生效 —— 本次浪费了两轮构建才定位到。
