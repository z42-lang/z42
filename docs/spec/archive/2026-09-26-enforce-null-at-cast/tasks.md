# Tasks: 转换到值类型也是解引用点

> 状态：🟢 已完成 | 创建：2026-09-26 | 完成：2026-09-26
> 分支/worktree：`enforce-null-at-cast` @ `wt-inhtparam` | 基于：origin/main `540177120` (#852)
> 类型：`fix`（**compiler** —— 义务面加一个使用点；**不取新诊断码**）
> 授权：User「那请你继续修正」

## 现象

```z42
object? x = …;          // 或任何标了 `?` 的形参 / 返回值 / 字段
Console.WriteLine(x.ToString());   // ✅ 报 E0478/E0484（解引用）
long v = (long)x;                  // 🔴 **不报** —— 运行期才抛 NullReferenceException
```

空检查义务只管**解引用**（成员访问 / 调用 / 索引 / foreach / throw），**转换位是个洞**。

⭐ 这是 `carry-field-null-marks` 里发现的：`ParameterInfo.DefaultValue` 标上 `?` 后
**零命中**，追下去才看清 —— 它的 10 处调用点全是 `(int)(long)…DefaultValue` 这种**转换**，
不是解引用。⇒ 那个标注当时等于「标了但这一类用法照样不受检」。

## 为什么该报：运行期已经把它当解引用了

`make-hard-cast-fail-properly` (#746) 定的语义：

| 转换 | null 时 |
|---|---|
| `(int)o` / `(long)o` —— 目标是**值类型** | 抛 `NullReferenceException` |
| `(string)o` / `(Box)o` —— 目标是**引用类型** | **合法**，得 null（C# 同） |

⇒ 「转换到不可空值类型」在运行期**必然解引用**，编译期该和 `.Member` 同等对待；
而「转换到引用类型」不是解引用，**不得报**。这条界线不是我划的，是 #746 的运行期行为划的。

## 修法：一个使用点，不取新码

`FlowAnalyzer` 的 `BoundConvert` 分支（此前只走操作数、不查义务）：

```
if (e is BoundConvert) {
    BoundConvert cv = e as BoundConvert;
    if (TypeChecker._isNonNullableValueType(cv.Type())) { this._checkDeref(cv.E); }
    this._reads(cv.E);
    return;
}
```

- **判据复用 `TypeChecker._isNonNullableValueType`，不另写一份** ——
  「编译期与运行期各判各的」正是 `where T : struct` 那个 bug 的形状（#786 也照此办）。
- **不取新诊断码**：`_checkDeref` 是单一漏斗，按义务来源（调用结果 / 裸名 / 字段）自动选
  E0478 / E0484 与对应消息。新增使用点不需要新码。
- `as`（`BoundCast`）**不介入**：`x as T` 失败返 null，本就不是解引用。

## 摸底（必须先做）

只加检查、跑**全量 `xtask test`**（不是 `build all`），逐个判读命中：
真 bug（该报）/ 合法写法需改写（改用快照 + 检查，或 `Expect("理由")`）。
已预期会命中 `src/tests/types/fold_param_defaults.z42` 的 10 处（#810 同款：
测试构造的输入必然有默认值 ⇒ 改 `Expect("理由")`）。

## 摸底结果（全仓 `xtask test`）

**累计 2 个文件、10 处命中**，全是 E0484，全在测试里（`DefaultValue` 由
`carry-field-null-marks` 标上 `?`）：

| 文件 | 处数 | 形态 |
|---|---|---|
| `src/tests/types/fold_param_defaults.z42` | 8 | `(int)(long)pOf(…).DefaultValue` 一族 |
| `src/libraries/z42.core/tests/reflection.z42` | 2 | `(int)(long)ps[1].DefaultValue` / `(bool)ps[3].DefaultValue` |

形态**单一**：输入全是用例**自己声明**的方法，被取的形参必有字面量默认值
⇒ **没有一处是「合法写法被误伤」**，生产代码零命中。

🔴 **摸底要跑「真跑完」，不是跑到第一个红**：第一轮只看到 8 处 —— 全量在 **regen 阶段**
就因那个文件编不过而中止了，stdlib 自己的 `tests/` 还没跑到。迁完 8 处再跑，才露出 stdlib 那 2 处。
⇒ 判据：**摸底的「一遍全量」= 走完所有阶段**；中途中止的那次只能算部分覆盖。
（这是本线在「全仓」上栽的第五次，前四次分别是只搜 `src/`、探针不含 `xtask test`、
单文件探针、以及这次的「中止即当跑完」。）

**迁移取一处收口，不散写 8 遍**：加 `object dvOf(string, int)` 助手，里面一处
`Expect("本用例在 class H 里声明的这些形参都带字面量默认值")`。
8 个调用点改走它 —— 理由是**同一句**，抄 8 遍没有意义，而运行期检查仍然真实存在。
（`reflection.z42` 那 2 处分散在不同形参上、理由各不相同 ⇒ 就地各写一个 `Expect`，不强行抽助手。）

⭐ **撞到一条自家规则并确认它管用**：我最初把理由写成拼接串
（`"…" + method + "#" + idx.ToString()`），被 **E0490** 拒了 —— 那条刻意要求理由是
**字符串字面量**（拼出来的理由等于没理由）。改固定字面量后通过。

⭐ **两条 🔒 对照在同一个文件里自动成立**（无需额外构造）：
`(string)pOf("Cat",0).DefaultValue`（转**引用类型**）**正确没报**；
`pOf("Enu",0).DefaultValue == null`（比较而非解引用）也没报 ⇒ 界线划对了。

## 用例

`src/tests/types/null_mark_at_cast.z42`（NEW，interp + jit 双过）钉 🔒 那半 ——
转引用类型的 null 合法通过（`(string)o` / `(Box)o` 都得 null 且不抛）/ 比较不算解引用 /
`as` 不介入 / 窄化后可转 / `Expect` 解除义务 / 未标 `?` 的值不受检。

⚠️ **「该报」那半不能写进能跑的用例**（写了就编不过）。它由
`src/tests/cross-zpkg/field_null_mark_crosspkg_unchecked/` 那类 `expected_build_error.txt`
负例承担 —— 本刀的摸底本身（8 处真报）也是该报必报的直接证据。

## 进度概览
- [x] 1 摸底（只加检查，跑全量收集命中）
- [x] 2 逐个判读 + 迁移命中点
- [x] 3 用例（🔒 转换到引用类型不得报 + 比较 / `as` / 窄化 / `Expect` 各一格）
- [x] 4 全量 GREEN + 指纹判定 + 文档 + PR
      - `xtask test all` **GREEN**（8m39s，**走完所有阶段**）；rebase 到 main 最新 + 指纹 bump 后重跑见下
      - **指纹 31 → 32**：理由是**诊断变**（那类源码此前零诊断、哈希未变 ⇒ 不 bump 会把新诊断吞掉；
        本线**第六次**）。发码本身不变 ⇒ CI 守门测不到，按 version-bumping.md 手动 bump
      - 文档：`reference/language/types.md` 把上一刀刚写的「转换位也不在义务面内」改成
        「转值类型算、转引用类型不算、`as` 不算」，与运行期语义对齐
      - 归档 → `archive/2026-09-26-enforce-null-at-cast/`
