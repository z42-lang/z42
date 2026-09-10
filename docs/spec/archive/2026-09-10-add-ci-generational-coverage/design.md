# Design: 一个有牙的 stage，和三处漏掉的屏障

## Decisions

### Decision 1: 用**编译器自建**，不用 golden 套件

先试的是「把 golden 套件用分代 + 1M nursery 再跑一遍」（16 s）。**它是空转的**：
把 #537 的缺陷放回去，整套 golden 依然全绿 —— 每个 golden 都是短命的小程序，
一次 minor 都不做。

真正抓得住的是**编译器自己编一个包**：#537 / #539 三个缺陷当年都是以
「编 z42c.semantics 编到一半崩在悬垂引用上」的形式暴露的。所以 stage 就跑那件事。

### Decision 2: nursery 定 16M —— 两头都反证过

nursery 决定 minor 跑多少次，而这些缺陷要**对象熬过好几次 minor** 才显形：

| nursery | minor 次数 | 放回 #539 的缺陷 | 干净树 |
|---|---|---|---|
| 32M（默认） | 10 | ✅ 绿（**抓不到**） | 绿 |
| **16M** | **26** | ❌ **红**（`ArrayGet: expected array, got Null`） | **绿** |
| 1M | 96+ | 红 | ❌ **红**（见下，known-open） |

16M 是「抓得住 + 干净树上是绿的」唯一档位。**这条推理必须留在代码注释里** ——
不然下一个人会顺手把它调小或调大，两边都会坏。

### Decision 3: 三处漏掉的数组写屏障 —— 顺手修，但如实说明它不是 1M 崩溃的原因

追 1M 崩溃时通读了所有「往数组里写堆引用」的路径，发现三处**从不发写屏障**：

| 路径 | 说明 |
|---|---|
| `Array.SetValue`（`builtin_array_set`） | 反射/`Std.Array` 的单元素写 |
| `Array.Copy`（`builtin_array_copy`） | **最尖锐的一处**：`perf-bulk-array-copy` 把脚本侧一个 `ArraySet` 循环换成了一条 bulk 原语——而**那个循环的每一次迭代都发屏障**，bulk 版一次都不发。它自己的注释还写着「a copy is indistinguishable from the loop it replaces」 |
| 经 `ref` 写数组元素（`frame.rs` 的 `RefKind::Array`） | `ArraySet` 发，这条不发 |

三处都是真的洞：老数组收到年轻元素却不置卡，下一次 minor 就不会 re-root 它。
**但补完之后 1M 仍然崩** —— 所以它们不是那个缺陷的成因，本 change 不声称修好了它。

屏障的卡键在**数组头自己的条目**上，与元素下标无关，所以 bulk copy 只要有一个堆引用
元素就够了；`barrier_copied_range` 因此只扫一遍被写的区间、跳过纯基元的拷贝
（有专门的测试盯着「纯基元拷贝不置脏」——屏障要精确，不能退化成「这个数组被写过」）。

## 未修完的那一个（known-open）

```bash
env Z42_GC_MODE=generational Z42_GC_NURSERY_BYTES=1M \
  artifacts/build/runtime/release/z42vm artifacts/.z42/programs/z42c/z42c.driver.zpkg \
  -- build src/compiler/z42c.semantics/z42c.semantics.z42.toml --release --no-incremental
# → 96 次 minor 之后：__str_hash_code: arg 0 expected string, got Null   （3/3 必现）
```

**早于 #552 / #553**（`git checkout 3935d8f1 -- src/runtime` 对照过，症状与 minor 次数
逐字相同）。2M 及以上全绿。

已有的探针形状（在 minor 的 sweep 之后，按 GC 自己的 `trace_children` 走全部活对象、
检查孩子是否已死）：

```
DANGLING ARRAY age=2 len=8 ch=117 e=76 card=true inYoung=false marked=false borrowed=false dead=1
DANGLING ARRAY age=1 len=4096 ch=137 e=2 card=false inYoung=true  marked=false borrowed=false dead=1
```

读法：

- 第一条 —— **老数组、卡是脏的、chunk 没被 TLAB 借出**，孩子却被扫了。
  卡在那儿、`iterate_dirty_cards` 也没有理由跳过它，**所以问题在标记阶段走到它之后**，
  不是漏发屏障（补了三处屏障之后症状不变，也印证了这一点）。
  ⚠️ 注意 `card=true` 是 **sweep 之后**读的，可能是这一轮晋升时
  `dirty_cards_for_newly_old_*` 重新置的，不能直接当作「mark 时它就是脏的」。
- 第二条 —— **年轻数组（age 1）、在 young 表里**，孩子却已死。

**下一步该怎么查**（省下重新摸索）：把探针挪到 **mark 与 sweep 之间**，
对每个「自己会存活」的 owner 检查它是否有**未被标记的年轻孩子** ——
那直接区分「标记没走到」与「扫描杀错了」。（这一步本次没做完：改动 `generational.rs`
时把模块头注释一起替换掉了，编译失败，随后回退。探针代码本身是对的，重来时注意
只替换函数体。）

## Testing Strategy

- gate stage 本身：**两头反证**（见决策 2 的表）
- 三个屏障回归测试，**都验证过「不打补丁就红」**：
  `array_set_value_dirties_the_card` / `array_copy_dirties_the_card` /
  `a_primitive_only_copy_dirties_nothing`（后者保证屏障是精确的，不是一律置脏）
- `./xtask test` 全绿；新 stage 7.1 s，gate 2m50s → 2m54s（+2.4%）
