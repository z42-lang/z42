# Tasks: complete-generic-class-identity（让泛型实例化成为运行期真正的类型）

> 状态：🟡 待 User 过 6.5 gate | 创建：2026-09-25
> 分支/worktree：待开 | 基于：origin/main `ef88897ac`（#825 合入后）
> 类型：`lang` + `ir`

**变更说明：** User 早已裁决「实例化是独立类型（对齐 C#）」。#774 为 blob struct 兑现了；
普通泛型 **class** 至今没有。代价已具体化为四条 soundness 缺口。

## 进度概览

- [ ] **P1** 实例化描述符完整化（基类链 / 接口 / 代换后字段 / 静态字段）
- [ ] **P2** `is` / `as` / 模式匹配携类型实参
- [ ] **P3** 静态字段按实例化分槽（⚠️ 自举敏感，分阶段引入）
- [ ] **P4** 方法签名代换（铺满已有的 `_substGenericSig`）
- [ ] **P5** 两处解析器缺口（`(G<T>)o` / `G<int>.X`）

⚠️ **P1–P3 互相咬死，不能只做一格**（见 design.md §D1 的实测）。P5 是 P2/P3 的**可测性前置**
——不修则那两格在源码层写不出来。

## 实测取证（main `ef88897ac`，全部实跑）

| # | 用例 | 实测 | C# |
|---|---|---|---|
| A | `class G<T>{T V; T Get(){return this.V;}}` → `g.Get().X` | `struct-value handle used after its creating frame exited — value-struct lifetime unsound` | `42` |
| B | `class DInt : GBox<int> {}` → `d.V` | `null` | `0` |
| C | `GBox<int>` / `GBox<string>` 静态计数 | `4 / 4` | `2 / 2` |
| D | `o as GBox<string>`（o 是 `GBox<int>`） | 放行 → `VCall: expected object, got I64(42)` | `null` |
| — | `(GBox<string>)o` / `GBox<int>.Count` | `E0202` 解析失败 | 合法 |

### 🔬 A 的原型实证（值得照抄的取证手法）

只让 **callee** 认出 `T→P2`（走 sret），症状立刻从
`value-struct lifetime unsound` 变成 `takes 2 physical argument(s), the call passes 1`。
⇒ **「lifetime unsound」与「签名解析不到」是同一条 bug 的两副面孔**，取决于哪一侧先判出具体
类型。**这是「必须整体做」的直接证据**，不是论证。

## 关键已知事实（省掉重新发现）

- ⭐ **vtable 不需要合成**：运行期 `build_type_registry` 从 `own_methods` + 基链 merge 出来，
  **不在 TYPE 段**。#774 把它算进「完整描述符」的负担里，那一条是**高估**。
- ⭐ **`_substGenericSig` 已存在**（`MemberResolver.Subst.z42:48`，形参位 + 返回位按 receiver
  的类级实参递归代换），今天只用在 `MemberResolver.z42:211`（接口成员）与
  `ConstructTyper.z42:282`（构造器）。P4 是**铺满**它，不是发明它。
- 🔴 基类名被**显式剥成裸名**在 `ClassDescBuilder.z42:151-162`（`fix-generic-base-name`），
  注释写明理由。身份成立后该理由消失，但必须与 P2 同时落地。
- 🔴 `is`/`as` 丢实参在 `TypeOpTyper.z42:46`（as）与 `:334`（is）；运行期按**名字符串**比
  （`dispatch.rs` 的 `is_subclass_or_eq_td` 首行 `derived == target`）。
- 🔴 静态字段键在 `AccessEmitter.z42`（`QualifyClass(裸名) + "." + 字段`）；运行期按 FQN
  字符串索引（`vm_context/types.rs` 的 `static_field_index`）。
- 解析器两处前瞻：cast 在 `ExprParser.z42:365`（定长 `( Ident )`），泛型出口在 `:114-130`
  （`<…>` 后须紧跟 `(`）。`_parseType()` 本身**早已支持**闭合泛型（`as` 走的就是它）。

## Out of Scope（已实测定性，别混进来）

- **单字段 struct 无值语义**：`struct S1 { int F; }` 的数组元素读
  `FieldGet: expected object, got Null`。**非泛型同样崩** ⇒ 与泛型无关，根因是
  `IsBlobStruct` 硬性要求 `FieldCount >= 2`。单独登记。
- 跨包模板投送（`complete-generic-instantiation` S2）、容器密集化。

## 验证纪律（照抄，别重新踩）

- ⚠️ **本地 `xtask test all` ≠ CI**：本地只含 e2e + stdlib + compiler，**不含 `test lines` /
  `walkers` / `docs` / `diagcodes`**。要么逐个点名，要么承认只有 CI 权威。
- ⚠️ **全绿 ≠ JIT 验过**：golden 只跑 interp，必须显式 `xtask test e2e --mode jit`。
- ⚠️ **单文件用例对「按 CU / 按包」的机制没有判别力**。
- ⚠️ **对照实验的两棵树只能差「我的改动」一个变量**；基线不同就不是对照。
- ⚠️ `git checkout <别的分支> -- src` 会**留下**该分支独有的文件（`checkout HEAD -- src` 不删）。
