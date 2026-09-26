# Tasks: 字段 / 属性的 `?` 标记跨包携带

> 状态：🟢 已完成 | 创建：2026-09-26 | 完成：2026-09-26
> 分支/worktree：`carry-field-null-marks` @ `wt-inhtparam` | 基于：origin/main `460b15421` (#844)
> 类型：`fix`（**compiler** —— 骑现有 attr-ref 通道，**无格式 bump**）
> 授权：User「然后再继续推进」（可空线可选项 ②）

## 这是可空线最后一个「漏报方向」的缺口

`define-null-check-marks` 的 `?` 标记，**方法**那半已跨包携带（#791 形参 `$Nullable` /
返回值 `$RetNullable`，#806 extern 桩），**字段 / 属性**那半没有：导入的字段一律
`IsNullable = false` ⇒ **漏报**（符合 D4，但机制在跨包场景等于不存在）。

## ⭐⭐ 先纠正一条记错的代价评估

旧记录说这条「拼写走 `SurfaceTypeName`，`?` 已被擦除；改它会动 TSIG 文本，牵扯继承字段展开
与 struct 布局判别」——**据此把它排成「最贵之一」。实测：不必碰拼写。**

`?` 不需要走类型拼写那条路，走**字段自己的 attr-ref 通道**即可，四处全是现成形态：

| 环节 | 落点 | 现成先例（一字不差的同形） |
|---|---|---|
| 生产侧发哨兵 | `ClassDescBuilder` 的 `fdesc.Attrs` / `sfdesc.Attrs`；**属性**的 attr 已挂背后字段（`sbf`/`ibf`，`add-json-serde` 建的） | `$Deprecated` / `$RetNullable` |
| 过线载体 | `ExportedFieldZ.IsNullable`（**ctor 元数不变**、构造后赋值 —— 旧种子 ABI 安全） | `IsDeprecated` / `DeprecationMsg` |
| 解码 | `TsigReconcile:460`（实例字段）/ `:470`（静态字段） | 同两行的 `IrDeprecation.Has(...)` |
| 导入侧回填 | `ImportedSymbolLoader:590` | 同行的 `fsym.IsDeprecated = fd.IsDeprecated` |

哨兵常量也是现成的：`IrParamDefault.NullableSentinel = "$Nullable"` + `IsNullable(attrs, count)`。
⇒ `SurfaceTypeName` 那条拼写**只喂 `AddOwnField`**（TSIG / struct 布局判别），与 `?` 无关。

⇒ 教训（记进记忆）：**复查一条「贵」的评估，先看它给的是实现描述还是原则** ——
实现描述会随代码腐坏，这条就是：它写下时 attr 旁路通道还没建（#791 才建的）。

## 🔴 只做机制会交付一道恒不响的门

摸底：**stdlib 里标了 `?` 的字段 / 属性 = 0 处**。⇒ 机制单独上线，全仓零命中，
既证明不了它接通、也没有任何实际作用（这条线已经栽过一次「零命中不能证明 pass 接通」）。

⇒ 本变更**必须同时给出会响的东西**，两个层面各一个：
- **语言层**：跨包 e2e fixture（生产方标 `?` 字段，消费方不检查就解引用 → 必须报 E0478/E0479）
- **stdlib 层**：把第一个真候选标上（见阶段 3），让机制对**真实生产代码**生效

（同 #791 建机制 / #810 上标注的分工，只是这次两半必须一起走，否则门不响。）

## 进度概览

- [x] 1 携带机制（四处）
- [x] 2 跨包 e2e fixture（正例 + **负例**，见下）
- [x] 3 第一个真标注：`ParameterInfo.DefaultValue`（**零命中，且原因已查清**，见下）
- [x] 4 全量 GREEN + 指纹判定 + 文档 + PR

## 3 候选判读（按 #810 的两条判据）

`Std.Reflection.ParameterInfo.DefaultValue`（`public object DefaultValue;`）：

- **判据①（作者自述「null 表示没有」）**：✅ 注释原文
  「the folded literal default (**null when none** / non-literal)」。
- **判据②（有没有配独立的存在性测试）**：⚠️ 有 `IsOptional`，但它**不是可靠判据** ——
  注释自述「非字面量默认值」时 `DefaultValue` 也是 null 而 `IsOptional` 仍为真
  ⇒ 存在性只能靠检查值本身 ⇒ **不与惯用法打架**。
- **实测调用形态**：全仓 10 处全在 `src/tests/types/fold_param_defaults.z42`，
  **全部直接解引用**（`(int)(long)pOf("Neg",1).DefaultValue`），零存在性检查；
  **生产代码零命中**。且这些调用点**跨包**（测试是独立编译单元）⇒ 正好压在本机制上。

⇒ **标**。10 处测试调用点改用 `Expect("理由")`（同 #810：测试构造的输入必然命中，
而 `Assert` 对编译器不可见，改完顺带把「断言与解引用之间的空隙」关掉）。

## 2 正面对照：两条 fixture，负例才是门

- **正例** `field_null_mark_crosspkg`：生产方标 `string? Marked` / `string? MarkedProp { get; set; }`，
  各配一个没标的对照；消费方跨包走**快照 + 检查**（E0484 的强制快照规则），编过并跑出预期输出。
- **负例** `field_null_mark_crosspkg_unchecked`（`expected_build_error.txt`）：
  消费方**不检查就解引用** ⇒ 期望编不过、输出含 `is marked \`?\` and may be null here`（**E0484**）。

🔴 **为什么负例不可省**：全仓标了 `?` 的字段 / 属性原本是 **0 处**。只留正例的话，
机制**完全没接通**时它照样全绿 —— 不受检也能编过、输出一模一样。
只有负例能分辨「接通」与「恒不响」。（同本线阶段 4 的教训。）

⭐ 实施中撞到一次：正例初稿写的是 `if (h.Marked != null) { h.Marked.Length }`（就地检查），
**被 E0484 拒了** —— 那正是 #763 的 Q1 定的 (a) 强制快照规则（字段每次访问都重读、
属性每次访问都重新调用，就地检查不能让下一次读变非空）。⇒ 机制第一次跑就自证接通了。

## 3 第一个真标注：1 处真命中 + 10 处「转换位不受检」

`ParameterInfo.DefaultValue` → `object?`。

🔴 **我先说过「零命中」，那是错的 —— 探针只编了两个文件。**
跑全量 `xtask test` 才抓到真命中：`z42.core/tests/reflection.z42:723`
（`object dvs = ps[2].DefaultValue;` 快照后**不检查就 `dvs.ToString()`**）⇒ E0478。
已按 #810 的老路改 `Expect("Blend 的第 3 个形参声明了字符串字面量默认值")` ——
原先那句 `Assert` 对编译器不可见，真为 null 时炸点是 NRE。

⇒ 教训（本线第四次）：**摸底的「全仓」必须是 `xtask test`**，
单文件 / `build all` 都不算（`build all` 不含 `src/tests/`、`examples/`、`scripts/`，
而**库自己的 `tests/` 只有 stdlib 那一档会编**）。

⭐⭐ **「零命中」必须解释**（否则交付的可能是恒不响的门）：那 10 处调用点
（`src/tests/types/fold_param_defaults.z42`）全是**强制转换**
（`(int)(long)pOf("Neg",1).DefaultValue`），而空检查义务只管**解引用**。实测分辨：

```
Console.WriteLine(ps[0].DefaultValue.ToString());   // ① 解引用 → E0484 ✅ 机制活着
long v = (long)ps[0].DefaultValue;                  // ② 强制转换 → 不报
```

⇒ 所以 `fold_param_defaults.z42` 那 10 处**确实不报**，但原因不是「标注没生效」，
而是**转换位本身不在义务面内**（另记，见下）。标注既有真命中、也有 ① 的跨包验证
⇒ **不是恒不响的门**。

## 🆕 顺带发现：强制转换不算解引用（另记，不在本变更）

`(long)x` 其中 `x` 标了 `?` 且为 null —— 编译期**不报**，运行期抛
`NullReferenceException`（#746 的拆箱路径）。所以标记的义务面**不含转换位**。

这可能是该扩的（转换到值类型等于必然解引用），但扩它要独立摸底：
全仓有多少 `(T)someNullable` 形态、会不会打到「先转后判」的合法写法。⇒ 独立成刀，不夹带。

## 4 收尾
- [x] 4.1 `xtask test all` **GREEN**（7m56s；rebase 到 main 最新 + 指纹 bump 后重跑仍 GREEN 8m31s）
- [x] 4.2 指纹判定：**25 → 26，必须 bump**。理由是**诊断变**：跨包读标了 `?` 的字段 / 属性
      并直接解引用，此前**编得过**（导入侧标记位恒 false ⇒ 漏报），现在报 E0478 / E0484，
      而那类源文件**哈希一字未变** ⇒ 不 bump 会命中旧条目、把新诊断整个吞掉
      （**本线第五次**栽在缓存上，理由与 #791 / #806 一字不差）。
      配套理由二：标了 `?` 的字段其 attr 块多了哨兵 ⇒ zpkg 字节变。
      ⚠️ 取号按当时的 main 现查（原树上是 23，rebase 后 main 已到 25 ⇒ 取 26；
      链条里 24 是别人让掉的号）。
- [x] 4.3 文档同步：`reference/language/types.md` 的跨包那段 —— 把「**字段**与**接口成员**的
      标记**不跟**」改成「字段与属性**跟**、只有接口成员不跟」，并**如实补上**「转换位也不在
      义务面内」（那是下一刀，不能让读者以为已经全覆盖）
- [x] 4.4 归档（阶段 9，在本 PR 内）→ `archive/2026-09-26-carry-field-null-marks/`
