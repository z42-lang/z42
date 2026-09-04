# Proposal: 让 `MethodDecl.RegKey` 成为名副其实的单一真相源

> 类型：refactor(compiler) — 不改语法 / IR / VM 语义，**目标是字节中性**
> 创建：2026-09-04 | 状态：🟡 DRAFT，待 User 审批
> 上游：`augment-string-prelude`（#420）收尾时 User 问「之前为支持稳定键加的旧版本兼容能不能去掉」

## 一句话

`md.RegKey` 号称是方法注册键的「单一真相源」，但**有两类方法从来没被写入过**，
于是全仓库 7 处消费点都得挂着「重建 arity 键 → 退回裸名」的老式兜底。
本变更把那两类补上，让不变量真正成立，兜底才可能安全移除。

## 现状与问题

`stabilize-instance-dispatch-keys`（#391 + #414）把实例方法注册键改为
「primary（声明序首个同名）→ 裸名 / 非-primary → 全签名 `MangleKey`」，
并在 `MemberCollector.z42:220` 写 `md.RegKey = regName`，注释称其为
「单一真相源：body 绑定 / IrGen / 导出 / 派发全复用此键」。

**但这个不变量并不成立。** `MethodDecl.RegKey` 的默认值是 `""`（`Decl.z42:152` 构造函数显式初始化），
而唯一给 AST 写它的地方是 `MemberCollector._fillClass` —— 它只处理 `class` / `struct` 的
`c.Members`。以下两类方法**永远保持空串**：

| 类别 | 为何留空 | 证据 |
|---|---|---|
| **`impl Trait for Type` 块里的方法** | `MemberCollector._passMembers` 只分派 `ClassDecl` 与顶层 `MethodDecl`，**完全不处理 `ImplDecl`**；impl 方法的符号由 `InheritanceResolver._passImpls` 用**裸 `md.Name`** 注册，全程不写 `md.RegKey` | `MemberCollector.z42:17-28`；`InheritanceResolver.z42:33` |
| **decl-only partial method**（有声明无实现） | `MemberCollector.z42:205-208` 的擦除分支跳过 `:219/:220` | 同上 |

于是消费点必须写成这样才能工作：

```z42
string mkey = md.Name + "$" + md.ParamCount.ToString();   // ① 老式 arity 键重建
if (md.RegKey != "") { mkey = md.RegKey; }                 // ② 新 SoT（正常路径）
else if (!ct.Methods.ContainsKey(mkey)) { mkey = md.Name; } // ③ 退回裸名（impl 方法走这里）
```

**这三档不是历史遗留、而是当前唯一工作路径**：`impl` 块与跨-CU partial 的方法体绑定、
IR 发射、TSIG 导出全靠 ③。删掉任何一处都会立刻断功能（详见 design.md 的逐点分析）。

## 为什么值得做

1. **不变量名不副实是持续的认知负担**。注释写着「单一真相源」，实际有两个空洞；
   每个新消费点都得复制这段三档模板，且很容易写错——`stabilize-instance-dispatch-keys`
   落地时就因为「按老方案重建键」的消费点漏改而崩过（`ClassExtractor._fromSymbol`
   `ParamTypes[1]` 越界），是那次 change 的主要返工来源。
2. **它挡住了后续清理**。只要空洞在，7 处兜底一处都删不掉；
   而这些兜底本身就是「加重载 → rekey」那类补丁的温床。
3. **修复面极小且字节中性**（见下）。

## 方案（要点，细节见 design.md）

**核心：在 impl 方法与 decl-only partial 的注册点，把 `md.RegKey` 写成「消费点兜底本来就会算出的那个值」。**

- `InheritanceResolver._passImpls`：注册时补 `md.RegKey = md.Name;`
  —— 与它自己 `target.Methods.Put(md.Name, …)` 用的键完全一致。
- decl-only partial：擦除分支不注册方法，故消费点取不到符号；此处按 design.md 讨论的两个选项之一处理。

因为写入值 == 兜底算出的值，**所有消费点的行为逐字节不变**，
故可用自举不动点（gen1==gen2）+ 全套 golden 直接背书。

随后（同 PR 或紧接的下一个）移除已证明为死代码的兜底分支。

## 不做什么

- **不改键的格式或规则**（primary 裸 / 非-primary 全签名不动），故**无格式 bump、无两代自举**。
- **不动 `CallEmitter` 的静态 DepIndex 查找**（`:238`）——那是另一个维度（静态键 + 跨版本自举容忍），
  单独评估。
- 不引入新语法、新 IR、新诊断。

## 风险

| 风险 | 缓解 |
|---|---|
| 「字节中性」判断错误 → 键漂移 → 断种子 | 自举不动点 gen1==gen2 + 全套 e2e golden 是硬门；本地可完整验证（无格式 bump，warm 路径可用）|
| 跨-CU partial 的 TSIG 导出路径**零测试覆盖** | 本变更需**先补该场景的测试**再动那处（tasks.md 已列）|
| 移除兜底时误删仍活的路径 | 分两步：先补 RegKey（纯加性）验绿，再删兜底；不合并成一步 |

## 待 User 裁决

1. 方案是否走？（User 已在对话中选 ⓑ「走 DRAFT 做根因修复」，此处正式确认）
2. **范围**：只补 RegKey（让不变量成立），还是连同移除兜底一起做？
   建议**分两个 PR**——补齐是纯加性、可快速合并；移除需要先补跨-CU partial 测试。
3. decl-only partial 的处理选项（design.md §3 给了 A/B 两案）取哪个。
