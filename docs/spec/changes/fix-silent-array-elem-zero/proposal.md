# `Array.SetValue` / `CopyRange` 的类型不符不再静默清零

> 类型：**fix（vm 类 ⇒ 规范先行）**｜ 创建：2026-09-27｜ 状态：**已裁决 → IMPL**（2026-09-27）
> 出身：#906（`fix-silent-prim-field-write`）的直接续作。那刀堵了「对象的基元**字段**」，
> 本刀是同一族的**数组元素**那格。两者同属结构审计 R3「`Value::Null` 六义哨兵 —— 修复全在读侧」。

## 1. 实测：普通合法 z42 代码静默损坏数据，且只在 release

`Std.Array` 的无类型 setter 声明是 `public extern void SetValue(Object value, int index);`
（`z42.core/src/Array.z42:50`）—— 形参**就是** `Object`，所以传任何东西都编译得过，
这正是这个 API 的设计本意。

`int[0]` 原值 **9**：

| 写入 | release 结果 |
|---|---|
| `a.SetValue(objNull, 0)` | 🔴 静默，9 → **0** |
| `a.SetValue(objStr, 0)`（`"not a number"`） | 🔴 静默，9 → **0** |
| `a.SetValue(objDouble, 0)`（`1.5`） | 🔴 静默，9 → **0** |
| `a.SetValue(obj42, 0)` | 42 ✓ |

### 🔴 最有力的一格：`double[]` 收整数

`double[0]` 原值 **7.5**：

```
d.SetValue(objInt42, 0)   =>  d[0] = 0      🔴
L.SetValue(objInt3, 0)    =>  L[0] = 3      ✓（long[] 收 I64）
d2.SetValue(objF125, 0)   =>  d2[0] = 1.25  ✓
```

**给 `double[]` 传一个整数在任何合理读法下都不是类型错误** —— C# 做拓宽，z42 编译器自己到处
允许 `int → double`（`PrimModel.CanWiden`）。而这里**静默写 0**，把原值毁掉。
这不是边角滥用，是日常写法。

### debug 会响，release 不响

| VM | 行为 |
|---|---|
| debug | panic：`array set_boxed: int[] backing got a non-matching Value (Null) — stored a zero.` |
| release | 静默清零 |

⇒ **release-only 的静默数据损坏**。

## 2. 根因：一条正确的策略被用在了它不适用的路上

`ArrayObj::set_boxed` 的六个基元臂在类型不符时走
`prim_value_mismatch`（`types/array.rs:117-131`，change `make-silent-fallbacks-signal`）。
那段头注把取舍写得很清楚，而且**论证本身是对的**：

> 取 `debug_assert!` 而不是 `bail!` …… 「类型不符」是编译器该在 `ArraySet` / `ArrayNewLit`
> 站点转换/拆箱掉的事，**不是用户的错**，所以 release 放行（不拿用户崩溃换诊断能力）。

⭐ **问题不在这条策略，在它的适用范围**：它是为 **IR 的 `ArraySet`** 写的 —— 那里编译器确实
该先转换，值到不了这儿就说明编译器有 bug，于是「不是用户的错 ⇒ release 放行」成立。

但 `builtin_array_set`（`corelib/array.rs:112`）是**另一条路**：形参声明就是 `Object`，
**没有任何编译器站点能转换它**，编译器行为完全正确。这条路上的值是**用户给的**，
那条论证一个字都不适用。

⇒ 与 #906 是**同一个结构**：一条按「这是谁的错」划分的正确策略，被一个共用的底层写入原语
顺带套到了另一类调用方身上。

### 顺带：debug 的 panic 消息会把人引向错误方向

消息写着 "The compiler should have converted/unboxed before this point"。
从 `SetValue` 这条路撞上它的人会去查编译器 —— 而编译器没有问题。

## 3. 修法：在 corelib 边界校验，不动底层原语

`builtin_array_set` **本来就有四道校验**（null 数组 / 非数组 / 负索引 / 越界），
只是缺了第五道「值装不装得进元素类型」。补在同一处，与
`reflection/accessors.rs:367` 用 `encode_prim(..)?` 的做法完全平行。

**`set_boxed` 与它的 `debug_assert` 一个字不动** —— IR `ArraySet` 那条路上，
原策略依然正确。

## 4. 裁决结果（2026-09-27）

| 裁决点 | 结论 |
|---|---|
| 合法拓宽 | **严格：只许同种，其余全抛**（不走 `CanWiden`）。`double[] <- 整数` 会抛 |
| 异常类型 | 裸 `Std.Exception`（与 #906 及本函数既有四道 `bail!` 一致） |
| 范围 | **连带扫** `Fill` / `Copy` / `pack_backing` |
| debug 消息 | **改**（它把人引向编译器，而那条路上编译器没问题） |

⚠️ **严格那条的代价我已向 User 摆明并被知情选择**：`double[].SetValue(42, 0)` 之后会抛，
而那在 C# 里合法。理由是判据无歧义，且**严格版随时能放宽、反过来不行**。

### 扫描结果：范围被**测量**收窄了（而不是盲目扩大）

User 要求连带扫，逐个测下来只有两处是用户可达的：

| 入口 | 用户能喂无类型值？ | 处置 |
|---|---|---|
| `SetValue(Object value, int index)` | ✅ 形参就是 `Object` | 🔴 修（实证 4 格） |
| `CopyRange(Array,…,Array,…)` | ✅ 两侧元素类型可不同 | 🔴 修（实证 2 格） |
| `Fill<T>(T[] array, T value)` | ❌ **泛型且有类型** —— 值在调用点绑到元素类型，编译器把关 | 不修 |
| `pack_backing` / `ArrayObj::typed` | ❌ 5 个喂值方全是 IR 站点（`ArrayNew`/`ArrayNewLit`）或 VM 自建（`char[]` from string、`byte[]` from bytes、`CreateInstance` 的 default） | 不修 —— 原 debug-only 策略在那里**正确** |

🔴 **更正我自己的一句话**：我一度报告「`Array.Fill` 不存在」。**错的** —— 它存在
（`Array.z42:84` / `:369` 两个重载），只是**泛型**，所以没有无类型缺口。结论相同，理由不同。

### 落地明细

判据**只写一份**，放在 `set_boxed` 旁边（`array_access.rs`）：

- `prim_backing_kind() -> Option<&'static str>` —— 基元 backing 的种类标签（非基元 → `None`）
- `prim_backing_accepts(&Value) -> bool` —— **该问题的唯一判据**
- `try_set_boxed(i, val) -> Result<()>` —— 校验后写入（与 #906 的 `try_set_field_value` 同款）

消费方两处：`builtin_array_set` 逐值校验；`builtin_array_copy` **两侧同种 ⇒ 零成本跳过**
（真实用法几乎全在这条，`perf-bulk-array-copy` 的理由不受影响），否则逐元素校验。

`set_boxed` 与它的 `debug_assert` **一个字没动** —— IR `ArraySet` 那条路上原策略正确。
`prim_value_mismatch` 的头注已补注适用范围，消息已按裁决 ④ 区分两类调用方。

### 🔴 防漂移测试第一次跑就抓到了东西

`prim_backing_accepts_agrees_with_set_boxed` 逐格核对「判据说能存 ⇒ `set_boxed` 真的忠实存了」。
第一版**红了** —— 抓的是我自己测试的播种值：拿 `I64(0)` 去播 `double[]`，在
**构造期**（`pack_backing`，与 `set_boxed` 共用同一套信号）就 debug-panic 了。
⇒ 既证明这套 debug 信号在 IR 那条路上真管用，也说明这条防漂移测试方向对。

⚠️ 它**只断言正方向**：反方向（判据说不能 ⇒ `set_boxed` 存 0）在 debug 下会撞
`debug_assert!(false)` 而 panic，无法断言。注释里写明了这一点，没有假装覆盖全。

### 阴性对照：对**入库的那条 fixture** 做

撤回两处改动、重建 release VM、跑 `src/tests/types/array_untyped_write_strict`：

```
int[] <- null  : no throw
Error: uncaught exception: Std.TestFailure: values not equal   ← Assert.Equal(9, a[0]) 失败
```

⇒ 第一格就打出 `no throw`，随后断言失败，因为元素被**静默清零**。这是被测 bug 的本体。

> 原 §5（三个待裁决问题与我给出的推荐）已由上面的裁决表取代。⚠️ 需要留在记录里的一点：
> **我当时推荐的是 (b) 按编译器拓宽集，User 裁的是 (a) 严格。** 我的理由是「运行期与编译期
> 不各判各的」，User 的理由是「判据无歧义、严格版随时能放宽」。两者都成立，最终按 (a) 落地。

## 6. 验收判据（含阴性对照）

- **正面**：上面矩阵里每个 🔴 格各一条 e2e + Rust 单测，从「静默」变成「抛」（或按 (b) 拓宽）。
- **回归门**（缺一不可，否则「把整条路堵死」也会变绿）：
  - `int[] <- I64` 照旧通过（**正常路径**）；
  - `long[] <- I64` / `double[] <- F64` / `string[] <- null` 照旧；
  - 若走 (b)，`double[] <- I64` 存 42.0 且**不抛**。
- **两个 profile**：release 必须报（这是本刀的全部要点）；debug 行为不回退。
- `cargo test --lib`（不带过滤）+ `xtask test e2e` + `test stdlib` + `test compiler`。
- 预期**零格式 bump、零指纹变更**（纯运行期行为）。

## 7. 不做

- **不动 `set_boxed` / `prim_value_mismatch` 的 `debug_assert`** —— IR `ArraySet` 那条路上
  原策略正确（见 §2）。
- 不碰 `Value::Null` 的表示本身（R3 的根治是另一个量级的工程）。
