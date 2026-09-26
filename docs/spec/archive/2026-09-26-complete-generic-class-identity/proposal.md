# Proposal: 让泛型实例化成为运行期真正的类型

> 类型：`lang` + `ir`（走阶段 1–9 完整流程）
> 前序：`generic-struct-erased-slot-value-copy`（#774）/ `complete-generic-instantiation`（#820、#825）
> 创建：2026-09-25

## Why

User 早已裁决「**实例化是独立类型**（对齐 C#）」。#774 为 **blob struct** 兑现了这条；
普通泛型 **class** 至今没有 —— 当时的记录是「给它独立身份要合成完整类描述符，远超本 change
范围，另开 change」。本 change 就是那个 change。

拖着不做的代价已经具体化为**四条 soundness 缺口**，全部在 main 上实测复现：

| # | 形态 | 实测 | C# |
|---|---|---|---|
| A | `class G<T>{T V; T Get(){return this.V;}}` → `g.Get().X` | `struct-value handle used after its creating frame exited — value-struct lifetime unsound` | `42` |
| B | `class DInt : GBox<int> {}` 的继承字段 | `null` | `0` |
| C | 静态字段跨实例化共享（`GBox<int>` 与 `GBox<string>` 同一槽） | 计数 `4 / 4` | `2 / 2` |
| D | `o as GBox<string>`（o 是 `GBox<int>`）**放行** | 随后 `VCall: expected object, got I64(42)` | `null` |

D 是**类型混淆**：`as` 本该返回 null 却放行，用户随后拿 `int` 当 `string` 用，崩在一个不可
catch 的内部错误上。

## 根因：一个，不是四个

> **实例化的「声明形状」没有成为真相** —— 布局与体已经单调化（#774 / #820 / #825），
> 但**描述符、基类链、方法签名、静态字段键、`is`/`as` 的目标名**仍然是擦除的。

四条缺口是这一条根因的四个切面：

| 缺口 | 擦除在哪一处 |
|---|---|
| A | 方法签名未代换 ⇒ 调用方与 callee **都**看不出返回的是 blob struct ⇒ 两侧都不走 sret ⇒ 返回**已死帧的 arena 句柄** |
| B | `ClassDescBuilder` 把泛型基类名**显式剥成裸名**（`fix-generic-base-name`）⇒ 运行期从 `GBox` 合并字段，`V` 的 `type_tag` 还是 `T` ⇒ `ObjNew` 取默认值给 `null` |
| C | 静态字段键 = `QualifyClass(裸名) + "." + 字段` |
| D | `_bindIsExpr` / `_bindAsExpr` 取 `(NamedType).Name`，**把类型实参扔掉**；运行期又按**名字符串**比 |

## 为什么必须一起做（做一半会更糟）

三处互相咬死，实测确认：

1. **只给身份、不改 `is`/`as`** ⇒ `x is GBox<int>` 编成 `is_instance x, GBox`（擦除名），
   而运行期类型变成了 `GBox<int>` ⇒ **从 true 变 false，且静默**。
   本线已反复证明：静默错值比崩溃贵得多。
2. **基类链二选一**：`GBox<int>` 的 base 记 `Std.Object` ⇒ `is GBox` 断；记 `GBox` ⇒ 运行期
   把定义的**擦除字段**也合并进来 ⇒ 同名两份槽、偏移错位。两个都要 ⇒ 必须先让 `is`/`as` 携实参。
3. **A 的两副面孔**：实测原型只让 callee 认出 `T→P2`（走 sret），症状立刻从
   「lifetime unsound」变成 `takes 2 physical argument(s), the call passes 1` ——
   **同一条 bug，取决于哪一侧先判出具体类型**。只修一侧＝换个地方错。

## What Changes

1. **实例化描述符完整化**：基类链 / 接口 / 字段（**代换后的 type_tag**）/ 静态字段。
   > ⭐ **vtable 不需要合成** —— 已实测：它由运行期 `build_type_registry` 从 `own_methods` +
   > 基链 merge 出来，不在 TYPE 段。这比 #774 当初的评估少一大块。
2. **`is` / `as` / 模式匹配携类型实参**：绑定期保留 `NamedType.Args`，走与实例化身份名**同一个**
   规范名出口。
3. **静态字段按实例化分槽**。⚠️ **自举敏感**（换键 ⇒ 旧种子打旧键、当前源打新键），
   User 已裁决走 `bootstrap-seed.md` 的分阶段引入：support 先行、晚一个 nightly 再 use。
4. **方法签名代换**：`_substGenericSig` **已存在**（`MemberResolver.Subst.z42:48`，形参位 + 返回位
   按 receiver 的类级实参递归代换），今天只用在接口成员解析与构造器两处。缺口是**一般实例方法
   调用那条路没用它** ⇒ 把它铺满。
   > 这条是**补完已有机制**，不是发明新机制。
5. **两处解析器缺口**（不修则 2/3 在源码层无法书写、无法测试）：
   - `(GBox<string>)o` —— cast 前瞻是定长 `( Ident )`，`<` 直接落空
   - `GBox<int>.Count` —— 泛型出口要求 `<…>` 后紧跟 `(`，`.` 则回滚成二元 `<`
   > `_parseType()` 本身早已支持闭合泛型（`as` 走的就是它），瓶颈纯在**前瞻形状**。

## Out of Scope

- **单字段 struct 无值语义**：`struct S1 { int F; }` 的数组元素读 `FieldGet: expected object,
  got Null`。实测**与泛型无关**（非泛型同样崩），根因是 `IsBlobStruct` 硬性要求
  `FieldCount >= 2`。**单独登记**，不混进本线。
- 跨包模板投送（`complete-generic-instantiation` 的 S2）。
- 容器密集化 / 删 P3a 装箱。

## Open Questions

- [ ] 给**所有**泛型 class 实例化独立身份，还是只给「布局确实不同」的？前者一致、后者省字节。
      #774 在 struct 那边选了**前者**（身份与布局解耦，User 裁决），本线应对齐。
- [ ] 静态字段分槽的分阶段引入具体分几步、过渡期两种键如何共存。
- [ ] `is`/`as` 携实参后，**擦除名回落**（`vcall_resolve`）要不要同步收紧（那是
      `complete-generic-instantiation` 的 S3）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
