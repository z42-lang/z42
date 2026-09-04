# Proposal: 方法注册键的推导收敛到单一 owner

> 类型：refactor(compiler) — 不改语法 / IR / VM 语义，**目标字节中性**
> 创建：2026-09-04 | 修订：2026-09-04（方向重定：从「补字段」改为「收敛推导逻辑」）
> 状态：🟡 DRAFT，待 User 审批
> 上游：`augment-string-prelude`（#420）收尾时 User 问「之前为支持稳定键加的旧版本兼容能不能去掉」

## 一句话

方法注册键的推导逻辑**被复制了 7 份**，散在 6 个文件里，没有单一 owner。
本变更把**写侧**（注册）与**读侧**（解析）各收敛到一个口子，
让「键怎么算」这份知识只存在一处。

## 问题

### 表层：7 份手抄的三档模板

每个消费点都写着同一段：

```z42
string mkey = md.Name + "$" + md.ParamCount.ToString();   // ① 老式 arity 键重建
if (md.RegKey != "") { mkey = md.RegKey; }                 // ② 新 SoT（正常路径）
else if (!methods.ContainsKey(mkey)) { mkey = md.Name; }   // ③ 裸名回落
```

| # | 位置 | 查的表 |
|---|---|---|
| 1 | `ClassExtractor.z42:194-196`（祖先/自身实例方法导出）| `chain[mci].Methods` |
| 2 | `ClassExtractor.z42:328-330`（自有静态方法导出）| `ct.Methods` |
| 3 | `DeclBinder.z42:188-190`（`_bindImpl`）| `ct.Methods` |
| 4 | `DeclBinder.z42:237-239`（`_bindClass` 方法体绑定）| `ct.Methods` |
| 5 | `DeclBinder.z42:336-338`（`_checkExposure`）| `ct.Methods` |
| 6 | `IrGenTypeEmitter.z42:122-124`（`EmitImpl`）| `iowner.Methods` |
| 7 | `IrGenMemberEmitter.z42:18-20`（`EmitMethod`）| （不查表，只拼 IR 名）|

**这份复制正是 `stabilize-instance-dispatch-keys`（#414）返工的直接原因**：
键规则一改，7 个抄本得同步 7 次，漏了 `ClassExtractor._fromSymbol` 那处 →
拿错 symbol → `sig.ParamTypes[1]` 在 len-1 上越界 → gen2 z42c 建 z42.core 直接崩。
不是「谁忘了填字段」，是**同一份知识没有 owner**。

### 里层：`md.RegKey` 混了两个概念

| | 含义 | 对**被擦除**的 decl-only partial |
|---|---|---|
| ① | 这个声明**注册在哪个键下** | 无定义（它根本没注册）|
| ② | 要把这个声明**解析到符号**该用哪个键 | 有定义（= 另一碎片里实现所用的键）|

消费点 #3/#4/#6/#7 要 ①；#1/#5 要 ②。把两者塞进同一个字段，正是那圈兜底存在的原因。

### 为什么字段现在还有空洞

`MethodDecl.RegKey` 默认 `""`（`Decl.z42:152` 构造函数显式初始化），
唯一给 AST 写它的是 `MemberCollector._fillClass`（`:220`）。两类方法永远保持空串：

| 类别 | 为何留空 |
|---|---|
| **`impl Trait for Type` 块方法** | `MemberCollector._passMembers`（`:17-28`）**不分派 `ImplDecl`**；符号由 `InheritanceResolver._passImpls`（`:33`）用裸 `md.Name` 注册，全程不写 RegKey |
| **decl-only partial method** | `MemberCollector.z42:205-208` 擦除分支跳过 `:219/:220` |

所以 ③ 裸名回落**不是历史遗留，是 impl 块与跨-CU partial 赖以工作的唯一路径**。

## 方案

**写侧 —— 单一注册入口。** 算键 / 写 `md.RegKey` / 写 `sym.RegKey` / `Methods.Put`
四件事绑成一个动作：

```z42
private void _registerMethod(Z42ClassType ct, MethodDecl md, MethodSymbol sym, string key)
```

`MemberCollector._fillClass` 与 `InheritanceResolver._passImpls` 都改走它。
此后**结构上不可能「注册了却没填 RegKey」**——填字段不再是「记得做的事」，
而是注册这个动作本身的一部分。impl 方法恒裸名（不参与 primary/非-primary）这条约束，
也在这里表达一次、注释一次。

**读侧 —— 单一解析 helper。** 语义明确为上面的 ②：

```z42
// 「这个声明应解析到哪个注册键」——含对被擦除 decl-only partial 的回落，
// 这是全仓库唯一保留该回落的地方。
public static string MethodKeyOf(StrMap methods, MethodDecl md)
```

7 份抄本 → 7 个一行调用（#7 无表查询，单独处理）。
残留的回落逻辑**只存在一处、只需注释一次**。

### 字节中性

helper 内部逐字复刻今天那三档的行为，故**所有消费点输出不变**。
可用自举不动点（gen1==gen2 逐字节）+ 全套 golden 直接背书。

## 收益

| | 现状 | 本变更后 |
|---|---|---|
| 下次改键规则要同步几处 | **7** | **1** |
| 「漏改一个抄本」这类回归 | 已发生过一次（#414）| 结构上不可能 |
| 「键怎么算」的知识 | 散在 6 个文件 | 单一 owner |
| `RegKey` 的语义 | 混了「注册键」与「解析键」 | 读侧 helper 明确表达「解析键」|

## 不做什么

- **不改键的格式或规则**（primary 裸 / 非-primary 全签名不动）→ **无格式 bump、无两代自举**
- **不把 impl 方法并入 primary/非-primary 规则**（那会 rekey + 需格式 bump，是独立语义扩展）
- **不动 `CallEmitter.z42:238-243`** 的静态 DepIndex 查找 —— 另一维度（静态键 + 跨版本自举容忍），
  且其 arityKey 在当前格式下已是死路，单独评估
- **不消除跨-CU partial 的 TSIG 导出零测试覆盖** —— 独立缺口，见下

## 风险

| 风险 | 缓解 |
|---|---|
| 「字节中性」判断错误 → 键漂移 → 断种子 | 自举不动点 + 全套 e2e golden；本地可完整验证（无格式 bump）。**改动前先存 z42c 三包 zpkg，事后逐字节对账** |
| 跨-CU partial 的 TSIG 导出**零测试覆盖**（`src/tests/partial-types/*` 全是单文件用例）| helper **不消除**此缺口。收敛阶段字节中性、安全；**真要移除回落**必须先补该测试 |
| 7 处一起改，红了难定位 | 分批提交（见 tasks.md），每批各自验绿 |

## 待 User 裁决

1. 方向确认（本文修订后的「收敛推导逻辑」，而非原先的「补字段」）
2. 范围：收敛（本变更）与**移除回落**是否分两个 PR —— **建议分开**：
   收敛是纯重构、字节中性、可快速合并；移除需要先补跨-CU partial 测试

> **修订说明**：本 DRAFT 初版把问题定为「`RegKey` 字段有两个空洞」，给出 A（不填）/ B（填一行）两案。
> 那是**表层**——A/B 都只动字段，7 份抄本原封不动，下次改键规则照样漏。
> User 追问「哪个是最本质的实现」后重新定位到「推导逻辑无 owner」，A/B 随之降级为
> helper 的内部实现细节（见 design.md §3），不再是需要裁决的分叉。
