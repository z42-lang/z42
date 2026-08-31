# Proposal: prim 类型实例方法支持 type-based 重载（解锁 prelude String 同 arity 补齐）

> 状态：🔵 DRAFT 待审批 | 类型：feat（编译器 codegen / 语义绑定，能力扩展）| 子系统：compiler
> 两阶段（自举纪律）：本 change **只做阶段 1（support）**——扩编译器能力，z42c/stdlib 源自身**不使用**新能力；String 方法补齐是**阶段 2**（晚一个 nightly，独立 change）。

## Why

想往 prelude `Std.String`（`src/libraries/z42.core/src/String.z42`，static-imported 包装类）补齐一批**同 arity、不同参数类型**的实例重载：

- `IndexOf(char)` —— 撞现存 `IndexOf(string)`（`String.z42:94`，均 arity 1）
- `Split(char[])` —— 撞现存 `Split(string)`（`String.z42:262`，均 arity 1）
- `Trim(char)` —— 撞现存 `Trim()`（`String.z42:217`；`Trim(char)` 为 arity 1 新增）
- 同族还有 `TrimStart(char)` / `TrimEnd(char)` 等

今天往 String 加这类**同 arity type-based 重载会误报 E0436**（`DiagnosticCodes.MissingUsing`，诊断文本 "namespace X is used but not imported"），把编译整包 z42.core 卡红。根因是 z42c 对 **prim 接收者**的实例方法调用**不做类型决议**，导致同 arity 重载绑定落空、串味到下游同名方法。

### 现象 → 两个相扣缺陷（前一轮深挖已核实，行号本轮在 origin/main 上复核）

**缺陷 B（根因）——prim 接收者实例绑定不做类型决议：**
`MemberResolver.z42:129-141` 的 prim-wrapper 分支用 `_overloadKey`（`OverloadBinder.z42:176-180`，**只按 `name$arity`，不做参数类型决议**）+ `_findMethod` 查方法。注释 `:132-135` 明确写"prim 收者的实例方法保持基线键规则（bare / Name$arity），静态-only 不 mangle 实例方法"。

但对 type-based 重载（同 arity 两候选），符号收集 `MemberCollector.z42:203-211` 已把这两个方法**各自 mangle** 成 `Split$1$<参数类型>` 键（`arityDup.Get("Split$1")=="2"` → `wantMangle=true` → `OverloadResolver.MangleKey(...)`）。于是 prim 路径查 `Split$1`（`_overloadKey` 返回它仅当 `_findMethod(String,"Split$1")!=null`，而 mangle 后该键已不存在）→ 回退查裸 `Split` → 也查不到 → `_findMethod` 返回 null（`MemberResolver.z42:136`）→ 落 `wms==null` 的 loose-bind 分支（`:142-143`）：`BoundCall("instance", ..., PrimModel.Keyword(rt.Name()), mem.Name=裸"Split", ..., Z42UnknownType())`。裸名 + Unknown 类型。

**缺陷 A（症状面）——实例 DepIndex 捷径缺 local-wins 守卫：**
`CallEmitter.z42:160` 的实例 DepIndex 捷径 `if (!owns && !ifaceRecv && !virtualRecv && this._ctx.Deps != null) { DepCallEntry de = Deps.GetInstance(c.MethodName, c.ArgCount); ... TrackDepNamespace(de.Namespace) }` **缺少 `ownerIsLocal` 守卫**。对比静态路径 `CallEmitter.z42:201-202` 已有对称守卫 `bool ownerIsLocal = LocalClasses.ContainsKey(c.OwnerClass); if (c.Kind=="static" && Deps!=null && !ownerIsLocal){...}`（`fix-crosspkg-static-ns-collision` 引入，注释 `:197-200`：本包自有类恒走本地 emit、不查 DepIndex）。

缺陷 B 产出的裸名 + Unknown 使 `owns`（`CallEmitter.z42:126` = `ChainHasMethod(c.OwnerClass, c.MethodName)`）失效：`ChainHasMethod`（`EmitContext.z42:143-168`）把 "string"→"String" 查 `ct.Methods.ContainsKey("Split")`——但 String 的方法键是 mangle 的（`Split$1$...`），裸 "Split" 不在 → `owns=false` → 进 DepIndex 捷径。z42.core deps 为空（`DepScan.z42:107` `declaredCount==0` 不做过滤 → 索引所有兄弟 zpkg，且 `DepScan.z42:105` self-exclude 排除 z42.core 自身），索引里 `GetInstance("Split",1)` 只剩下游 `Std.Regex.Regex.Split(string)`（`src/libraries/z42.regex/src/Regex.z42:241`，namespace `Std.Regex`）→ 登记虚假 dep ns `Std.Regex` → `_enforceFileScope`（`CuPreprocess.z42:171-193`）发现该 ns 未被 String.z42 的 using 覆盖 → **E0436**。

### 为何阻塞 String 补齐

`library_review` 迭代计划里 String 补齐（口令"推进 library 迭代"）明确记录："E0436 编译器修复 support 先行，勿直接往 prelude String 加会撞 E0433+E0436 两墙"。本 change 就是那道 support——修好 prim 实例 type-based 重载绑定后，阶段 2 才能安全给 String 加同 arity 重载。

## What Changes

**主修 B（根因，最小增量）**：`MemberResolver.z42:130-141` prim-wrapper 分支，在 `_overloadKey`+`_findMethod` 落空（`wms==null`）时，**追加一次**基于类型的重载决议（复用 `_resolveOverload`，`OverloadBinder.z42:199`，与 class 接收者路径 `MemberResolver.z42:57/62` 同款），命中则用其 `MethodSymbol.RegKey`（正确 mangle 键）+ 真实返回类型产出 `BoundCall`。**仅 `wms==null` 时追加**，保证无 type-based 重载时逐字节等价。

**辅修 A（对称守卫，防御）**：`CallEmitter.z42:160` 的实例 DepIndex 捷径条件追加 `!ownerIsLocalInst` 守卫，其中 `ownerIsLocalInst = LocalClasses != null && LocalClasses.ContainsKey(<prim-wrapper>(c.OwnerClass))`（`c.OwnerClass` 是 prim 关键字小写如 "string"，需 wrapper 映射到 "String" 再查 LocalClasses）。与静态路径 `:201-202` 对称，是正确的 local-wins 设计，即使 B 修好也应存在。

两处对现有代码**字节中性**（论证见 design "字节中性" 决策）：今天无任何 prim 类同 arity type-based 重载 → B 的追加分支只在 `wms==null` 且 `_resolveOverload` 命中一个 mangle 候选时改变输出（今天不存在此组合）；A 的守卫仅在"本地 prim 类被 DepIndex 捷径劫持"这一今天就是误编译的组合下改变输出。

**无 zbc/zpkg 格式变更、无新语法、无新 IR**：纯编译期绑定/codegen 逻辑修正；产物里派发键（RegKey）机制既有，type-based 重载对 class 接收者今天已工作。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | 主修 B：prim-wrapper 分支 `wms==null` 时追加 `_resolveOverload` 类型决议，命中用 `RegKey`+真实返回类型（约 `:141` 后、`:142` 前） |
| `src/compiler/z42c.semantics/src/CallEmitter.z42` | MODIFY | 辅修 A：实例 DepIndex 捷径（`:160`）加 `!ownerIsLocalInst` 守卫，与静态路径 `:201-202` 对称 |
| `src/compiler/z42c.semantics/tests/<prim_instance_overload>.z42` | NEW | 单测：prim 类同 arity type-based 实例重载 → 绑定到正确 mangle RegKey；无重载时键不变 |
| `src/compiler/z42c.semantics/tests/e2e/<...>` 或既有 e2e harness | NEW | e2e：给 String 临时加 `Split(char[])`，验运行期 VCall 派发到正确重载（见 Open Questions Risk#3） |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 若职责描述涉及 MemberResolver 的 prim 实例绑定，补注 type-based 重载支持 |
| `docs/book/src/compiler/<重载解析 / 派发键机制页>` | MODIFY | 补 prim 接收者实例方法 type-based 重载的绑定→codegen 数据流（若已有对应机制页；无则询问 User 落点） |

## 只读引用（根因锚点，本轮在 origin/main 复核）

- `src/compiler/z42c.semantics/src/MemberResolver.z42:57` / `:62` — class 接收者路径 `_resolveOverload` + `ms.RegKey`（主修 B 的镜像模板）
- `src/compiler/z42c.semantics/src/MemberResolver.z42:129-143` — prim-wrapper 分支（主修 B 现场；`:135` `_overloadKey`、`:136` `_findMethod`、`:137-140` wms!=null、`:142-143` loose-bind Unknown）
- `src/compiler/z42c.semantics/src/OverloadBinder.z42:176-180` — `_overloadKey`（只按 name$arity）
- `src/compiler/z42c.semantics/src/OverloadBinder.z42:199` — `_resolveOverload`（类型决议，主修 B 复用）
- `src/compiler/z42c.semantics/src/MemberCollector.z42:203-211` — 实例方法 mangle（`arityDup==2 → wantMangle → OverloadResolver.MangleKey`）
- `src/compiler/z42c.semantics/src/CallEmitter.z42:126` — `owns = ChainHasMethod(c.OwnerClass, c.MethodName)`
- `src/compiler/z42c.semantics/src/CallEmitter.z42:160-178` — 实例 DepIndex 捷径（辅修 A 现场）
- `src/compiler/z42c.semantics/src/CallEmitter.z42:197-217` — 静态路径 local-wins 守卫（辅修 A 的对称模板）
- `src/compiler/z42c.semantics/src/EmitContext.z42:143-168` — `ChainHasMethod`（prim→wrapper via `_primWrapper`）
- `src/compiler/z42c.semantics/src/EmitContext.z42:325` — `EmitContext._primWrapper`（**private static**，CallEmitter 不可达）
- `src/compiler/z42c.semantics/src/TypeFactsTc.z42:40` — `TypeFactsTc._primWrapper`（**public static**，辅修 A 应用这一份，见 Open Questions）
- `src/compiler/z42c.semantics/src/CuPreprocess.z42:171-194` — `_enforceFileScope`（E0436 触发点；`:183` `DiagnosticCodes.MissingUsing`）
- `src/compiler/z42c.pipeline/src/DepScan.z42:105` / `:107` — self-exclude + `declaredCount==0` 无过滤（空 deps 索引所有兄弟）
- `src/libraries/z42.regex/src/Regex.z42:241` — 下游 `Regex.Split(string)`（namespace `Std.Regex`，被误绑的那份）
- `src/libraries/z42.core/src/String.z42:94/217/262/311` — String 现存 IndexOf/Trim/Split 重载（阶段 2 落点，本 change 不动）
- `docs/spec/changes/fix-crosspkg-static-ns-collision/proposal.md` — 静态路径 local-wins 守卫的引入背景（辅修 A 是其实例侧对称补齐）

## Out of Scope

- **String 方法本身（`IndexOf(char)` / `Split(char[])` / `Trim(char)` 等）= 阶段 2**：本 change 只做编译器能力（support），不往 z42.core 加任何同 arity 重载（临时 e2e fixture 除外，跑完删除，不进 prelude）。
- **其它 prim 类型的重载扩展**（Int32 / Char / Double 等的同 arity type-based 实例重载）：修好后能力对所有 prim 包装类一致生效，但本 change 不主动往它们加方法；有真实需求时随各自 library 迭代落。
- **prim 静态方法**（`int.Parse` / `string.FromChars` 等）的重载：静态路径已用 RegKey mangle（`MemberResolver.z42:132-134` 注释"prim 静态走下方独立路径，那里才用 RegKey"），不在本 change 现场。
- **class / struct / interface / 泛型实例化接收者**的 type-based 重载：今天已工作（`MemberResolver.z42:57/62`、`:95/107`），不动。
- **E0433**（library_review 记录的另一墙）：若 String 补齐还需修 E0433，那是独立诊断路径，不在本 change（本 change 只根治 E0436 那条 prim 实例绑定链）。

## Open Questions

- [ ] **Risk#3（本地不可验，需 e2e 实测）**：runtime VCall 是否以 **RegKey（mangle 键）** 为 vtable 派发键？这决定"辅修 A 单独够不够、主修 B 产出的 mangle-RegKey VCall 能否被 VM 正确派发"。前一轮推断：class 接收者 type-based 重载今天能工作，正因 `MemberResolver.z42:62` 携 `ms.RegKey`=mangle 键做 VCall 派发；但 **prim 接收者的 vtable 派发键需实测坐实**。→ 写一个 String type-based 重载 e2e（临时给 String 加 `Split(char[])`），验运行期派发到正确重载。**若 prim 接收者 VM 不以 RegKey 派发**，则方案需调整（可能需 VM 侧配合），须回报 User 重新裁决。
- [ ] **辅修 A 的 prim→wrapper 助手选型**：任务原文写 `EmitContext._primWrapper(c.OwnerClass)`，但该助手是 `private static`（`EmitContext.z42:325`），CallEmitter 不可达；可达的公开等价物是 `TypeFactsTc._primWrapper`（`public static`，`TypeFactsTc.z42:40`，`MemberResolver.z42:129` 已用它）。**倾向用 `TypeFactsTc._primWrapper`**；或把 `EmitContext._primWrapper` 提升可见性——待 design/实施定，二者对 wrapper 映射语义等价（都含 string→String）。
- [ ] **是否需要 bump z42c 自举能力版本号**？倾向否——无新语法/格式，仅编译器绑定逻辑修正；且本 change 是 support、z42c/stdlib 源自身不使用新能力 → 上一 nightly z42c 恒能编当前源（`xtask test bootstrap` 应恒绿）。待 design 定。
- [ ] **book 机制页落点**：z42c 是否已有"重载解析 / RegKey 派发键"机制页？若无，实现原理文档该新建还是并入哪页——按 doc-system.md §5.1"不确定该不该写/落哪 → 停下问 User"处理。
