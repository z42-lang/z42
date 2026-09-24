# carry-null-marks-through-native-stubs

**类型**：`fix`（最小化模式 —— 不新增语法 / IR 指令 / VM 行为，只让**已定的规则**
在一条被跳过的代码路径上生效）

**母线**：`docs/spec/archive/2026-09-23-define-null-check-marks/`（已归档）。
该 change 的 §3.3 把 `?` 标记的跨包携带做完了，但在「不携带的三类」里把
**extern 桩**列为刻意缺口。本 change 只关掉这一类。

---

## 为什么推翻归档里的「不做」

归档给③的理由只有一句实现描述（「`_emitNativeStub` 不写 `Attrs`/`ParamAttrs`」），
**没有原则性理由**（不像①接口成员那条要扩 zbc 格式）。2026-09-25 重新摸底，前提变了：

1. **stdlib 里已经有 9 处 extern 标了 `?`**，编译器却静默忽略 ——
   不是「漏报一种形态」，而是**作者已经 opt in、机制假装没看见**。
   其中 6 处是返回值：`Environment.GetEnvironmentVariable` / `RuntimeConfig.Get`·`Describe` /
   `AppProperties.Get`·`Raw` / `ProcessNative.Which`；
   3 处是形参：`Object.ReferenceEquals(Object?, Object?)` / `Object.Equals(Object?)` /
   `String.Equals(object?)`。
2. **它是「人工标注 stdlib」的前置**。母线记的下一步（人工标注「可能没有」API）
   的最佳候选里，`Type.GetElementType` / `Type.GetGenericArguments` /
   `WeakHandle.Upgrade` / `PropertyInfo.GetValue` 等**都是 extern** ⇒
   不先补这条，标了就是空操作。

⇒ User 2026-09-25 裁决：走这条。

## 实证（带阳性对照，不是「零命中」那种无效证据）

母线铁律：**全量零命中既不能证明规则对、也不能证明 pass 接通**。故两条一起跑：

| 探针 | 被调 API | 是否 extern | 结果 |
|---|---|---|---|
| 阴性（修前应当漏） | `Environment.GetEnvironmentVariable("PATH").Length` | ✅ extern | **编过，零诊断** ⇒ 缺口坐实 |
| 阳性对照 | `IPAddress.TryParse("1.2.3.4").ToString()` | ❌ 非 extern | **E0478** ⇒ 证明跨包 pass 本身是通的 |

两者都是**跨包**调用真 stdlib（`Z42_LIBS` 指向本树 `artifacts/.z42/libs`），
只差「被调方是不是 extern」这**一个变量**（母线教训：阴性对照必须只变一个变量）。

## 落地

- [x] 1. `IrGenMemberEmitter.EmitMethod` 的 **extern 分支**补两行
      （`stub.Attrs = _attrRefs(rawMem)` / `stub.ParamAttrs = _paramAttrRefs(md, !static)`），
      **与 abstract 桩那一支逐字同源**。
  - ⭐ 敲成「与 abstract 分支对齐」而不是「只补可空位」：这条分支历来是
    **每来一个特性就补一次**（紧邻那行注释自述「add-member-visibility：extern 桩**此前漏设**」，
    `add-param-metadata` 也是事后补的）。只补可空位等于把同一个坑再埋一次。
  - 顺带把重复计算的 `IrGenFacts._hasWord(md.Mods, "static")` 收敛成局部 `natStatic`。
- [x] 2. `CompilerFingerprint` **14 → 15**。
  - ⚠️ **bump 的真正理由是诊断变**：跨包调用这些 extern 并直接解引用的源文件
    **此前编得过（零诊断）**、现在该报 E0478/E0479，而它们**哈希一字未变** ⇒
    不 bump 就命中旧缓存条目、新诊断整个被吞（本线第四次栽在缓存上）。
    配套理由二：标了 `?` 的 extern 桩 attr 块多了哨兵 ⇒ zpkg 字节变。
  - ⚠️ **取号现查**：母线记忆里写的「当前最高 12」早已过期（#795 占 13、#796 占 14）。
- [x] 3. 迁移：**实测为 0 处** —— `xtask test` 全量 **GREEN**（8m25s，15 个 stage 全过）。
      预期中风险最大的单参 `GetEnvironmentVariable` 站点确实都已写成 `string? v = …` 并自带检查。
- [x] 4. 用例：`src/tests/cross-zpkg/` 两条
  - `nullable_marks_extern_stub_unchecked`（**阴性**）：跨包 extern 的 `?` 返回值直接解引用 → E0478
  - `nullable_marks_extern_stub`（**阳性 + 支点对照**）：同一个 builtin 的两个 extern、只差一个 `?`。
    标了的先查再用；**没标的直接解引用必须照样编得过** —— 它一旦跟着报错，
    就说明「缺席 = 不强制」塔成了「所有引用类型都查」。
  - ⭐ **fixture 本身做了阴性对照**：把 `expected_build_error.txt` 改成错名字 ⇒ 立刻 `FAIL`
    （否则「PASS」证明不了它走到过那条路）。
- [x] 5. 文档
  - 母线归档页的缺口③补一条「已关闭」指针。
  - ⭐ `docs/reference/src/language/types.md` **不用改**：它写的是「类方法与自由函数的
    形参/返回值标记跟过包边界」，而 extern 本来就是类方法 ⇒ **文档一直是对的，
    是实现没跟上**（不同于字段/接口成员那两条，文档是明写了「不跟」的）。

## 实测迁移代价（改动前量的）

| API | 跨包调用点 | 备注 |
|---|---|---|
| `Environment.GetEnvironmentVariable`（**单参**） | 12 | 双参重载 101 处**不受影响**（返回不带 `?`） |
| `Process.Which` | 22 | 非 extern 包装层，**本就已生效** |
| `RuntimeConfig.Describe` | 3 | |
| `RuntimeConfig.Get` / `AppProperties.Get`·`Raw` / `ProcessNative.Which` | 各 1 | |

单参 `GetEnvironmentVariable` 的站点多数已经写成 `string? v = …` 并自带检查 ⇒
预期真正要改的很少。**以实跑为准，不以此表为准。**

## 明确不做（留理由，别当遗漏）

- **extern 属性 getter**（`IrGenMemberEmitter.EmitProperty` 的 `pstub` 那支）同样不写 attr 块，
  但**全仓 `?` 标记的 extern 属性 = 0 处** ⇒ 零实例。且它的返回标记要从合成的
  `synthPd` 取（`_attrRefs` 的 `$RetNullable` 判据只认 `MethodDecl`，
  拿 `PropertyDecl` 恒落空），属于**字段/属性那条缺口②**的范畴，随②一起做。
- **缺口①接口成员 / ②字段**：维持归档里的结论，各自独立 change。

## 顺手清掉的母线欠账（与主修无关，可单独评审）

#741 把 `TryParse` 全迁成 `bool TryX(ref T)` 了，但**注释没跟** —— 11 处仍写着
「returns `null` on malformed input」。最糟的是 `Int32.TryParse`：它自述「mirrors
IPAddress.TryParse — nullable return in place of C#'s `out` parameter」，而现在**正好反过来**
（`IPAddress` 是引用类型、仍返 `IPAddress?`；标量类型因**值类型永不可空**而走 `ref`）。
改法上保留了「为什么是 `ref` 而不是可空返回」这个**反直觉约束的解释**（指向 E0476），
而不是把句子压短 —— 压注释往往刚好删掉的就是这类解释。

涉及：`Int32` / `Int64` / `Double` / `SByte` / `Int16` / `UInt16` / `UInt32` / `UInt64` /
`Byte` / `Guid`，加 `docs/library_review.md` 那条同款过期断言。
