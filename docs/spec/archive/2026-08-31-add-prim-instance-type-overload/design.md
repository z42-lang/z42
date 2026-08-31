# Design: prim 类型实例方法 type-based 重载

## Architecture

### prim 实例调用绑定 → codegen 数据流（现状 + 修复后）

```
源码: s.Split(charArray)   （s : string）
  │
  ▼  [绑定] MemberResolver._bindInstanceMemberCall  (MemberResolver.z42:42)
  │    rt = string → 非 Z42ClassType-builtin、非 interface、非 InstantiatedType
  │    → prim-wrapper 分支 (:129)
  │        wrapper = TypeFactsTc._primWrapper("string") = "String"   (:129)
  │        wct = Symbols.GetClass("String")                          (:130-131)
  │        mkey = _overloadKey(wct, "Split", 1)                      (:135)
  │           _overloadKey: 试 "Split$1"; _findMethod(wct,"Split$1")?
  │             ├─ 无 type-based 重载: "Split$1" 键存在 → 返回 "Split$1"
  │             └─ 有 type-based 重载: MemberCollector 已 mangle 成
  │                   "Split$1$Std.String" / "Split$1$Std.Char[]"    (MemberCollector.z42:203-211)
  │                   → "Split$1" 键不存在 → _overloadKey 回退裸 "Split"
  │        wms = _findMethod(wct, mkey)                              (:136)
  │           ├─ 无重载: wms != null → BoundCall(RegKey=mkey, ret=真实)  (:137-140)  ✅
  │           └─ type-based 重载: mkey∈{"Split$1",裸"Split"} 都查不到 → wms == null
  │                                                                    ▼
  │        ┌──────────── 主修 B（新增，仅 wms==null 时）────────────┐
  │        │  rms = _resolveOverload(Symbols, wct, "Split", args, 1) │  (镜像 :57)
  │        │    → 按实参类型决议 → 命中 "Split$1$Std.Char[]" 符号     │
  │        │    if rms != null:                                       │
  │        │       BoundCall("instance", recv,                        │
  │        │          OwnerClass = PrimModel.Keyword("string"),       │  (镜像 :62/139)
  │        │          MethodName = rms.RegKey ("Split$1$Std.Char[]"), │
  │        │          ret = rms.Signature.Ret)                        │  ✅ 正确
  │        └────────────────────────────────────────────────────────┘
  │        （rms == null → 落既有 loose-bind:142-143，不变）
  ▼  [codegen] CallEmitter 实例路径
  │    owns = ChainHasMethod("string","Split$1$Std.Char[]")          (:126)
  │       → _primWrapper→"String"; LocalClasses["String"].Methods
  │         .ContainsKey("Split$1$Std.Char[]")  → true（B 修好后携正确键）✅
  │    owns==true → 跳过 DepIndex 捷径(:160) → VCall 携 RegKey        (:190)
  │
  │    ┌──── 辅修 A（防御，即使 B 修好也应存在）─────────────────────┐
  │    │  DepIndex 捷径(:160)加 !ownerIsLocalInst 守卫:              │
  │    │    ownerIsLocalInst = LocalClasses.ContainsKey(            │
  │    │        _primWrapper(c.OwnerClass))                          │  (对称 :201-202)
  │    │  → 本地 prim 类即便 owns 因故为 false 也不进捷径、不串味     │
  │    └────────────────────────────────────────────────────────────┘
  ▼  运行期 VM：VCall 按 vtable slot 派发（键 = RegKey？——待实测，Risk#3）
```

### 今天为何 class 接收者的 type-based 重载能工作、prim 不能

class 接收者路径（`MemberResolver.z42:57/62`）**一开始就用 `_resolveOverload`**（做类型决议），拿 `ms.RegKey`（mangle 键）产 `BoundCall`。而 prim-wrapper 分支（`:135-136`）**只用 `_overloadKey`+`_findMethod`**（不做类型决议，注释 `:132-135` 声明这是刻意"基线键规则"）——在同 arity 重载 mangle 后必然查空。主修 B 就是把 class 路径已有的"落空补一次类型决议"能力对称补给 prim 路径。

## Decisions

### Decision 1: 缺陷 B（根因）—— prim 实例绑定不做类型决议

**问题：** prim 接收者的同 arity type-based 重载，`_overloadKey`（`OverloadBinder.z42:176-180`，只按 `name$arity`）查不到 mangle 后的键 → `_findMethod` 返回 null → loose-bind 裸名 + Unknown。

**事实链（本轮在 origin/main 复核）：**
- `MemberCollector.z42:205-208`：同 arity 重载（`arityDup.Get("Split$1")=="2"` 且非协议豁免名）→ `wantMangle=true` → `regName = OverloadResolver.MangleKey(...)`；`ct.Methods.Put(regName, msym)`（`:223`）。→ String 里 `Split(string)`/`Split(char[])` 的键是 `Split$1$<类型>`，不是裸 "Split" 也不是 "Split$1"。
- `MemberResolver.z42:135`：`_overloadKey(wct,"Split",1)` 试 "Split$1"，`_findMethod` 查不到（键已 mangle）→ 返回裸 "Split"。
- `MemberResolver.z42:136`：`_findMethod(wct,"Split")` 也查不到 → `wms==null`。
- `MemberResolver.z42:142-143`：落 loose-bind `BoundCall(..., PrimModel.Keyword(rt.Name()), mem.Name=裸"Split", ..., Z42UnknownType())`。

**决定：** **主修 B**——在 prim-wrapper 分支 `wms==null` 处（`:141` 之后、`:142` loose-bind 之前）**追加一次** `_resolveOverload`（`OverloadBinder.z42:199`，与 class 路径 `:57` 同款）：
```
（伪码，紧接现 :140 的 if 之后）
MethodSymbol rms = this._tc._overload._resolveOverload(env.Symbols, wct, mem.Name, args, argCount, sp);
if (rms != null) {
    // 访问/弃用检查（镜像 class 路径 :59-60）
    args = this._tc._overload._withDefaults(rms, args, rawArgs, argCount, env, sp);  // 或 FillDeferredArgs，见实施
    return new BoundCall("instance", true, recv, PrimModel.Keyword(rt.Name()), rms.RegKey, args, args.Length, rms.Signature.Ret, sp);
}
// rms == null → 落既有 loose-bind（:142-143 不变）
```

### Decision 2: 缺陷 A（症状面）—— 实例 DepIndex 捷径缺 local-wins 守卫

**问题：** `CallEmitter.z42:160` 的实例 DepIndex 捷径无 `ownerIsLocal` 守卫（静态路径 `:201-202` 有）。缺陷 B 产出裸名+Unknown → `owns`（`:126` `ChainHasMethod("string","Split")`）因 String.Methods 键是 mangle 的而查空 → `owns=false` → 进捷径 → `GetInstance("Split",1)` 命中下游 `Std.Regex.Regex.Split`（z42.core 空 deps，`DepScan.z42:107` `declaredCount==0` 索引所有兄弟；`:105` self-exclude 排掉自身）→ `TrackDepNamespace("Std.Regex")`（`:164`）→ E0436。

**决定：** **辅修 A**——`CallEmitter.z42:160` 捷径条件追加 `!ownerIsLocalInst`：
```
bool ownerIsLocalInst = this._ctx.LocalClasses != null
    && this._ctx.LocalClasses.ContainsKey(TypeFactsTc._primWrapper(c.OwnerClass));
if (!owns && !ifaceRecv && !virtualRecv && !ownerIsLocalInst && this._ctx.Deps != null) { ... }
```
`c.OwnerClass` 对 prim 实例调用是关键字小写（"string"，见 `MemberResolver.z42:139/143` 的 `PrimModel.Keyword`），故须经 `_primWrapper` 映射 "string"→"String" 再查 `LocalClasses`（与 `ChainHasMethod` `:158` 同款映射）。

> **助手选型（Open Question）**：任务原文写 `EmitContext._primWrapper`，但它是 `private static`（`EmitContext.z42:325`），CallEmitter 不可达。可达等价物 = `TypeFactsTc._primWrapper`（`public static`，`:40`，`MemberResolver.z42:129` 已用）。本 design 采 `TypeFactsTc._primWrapper`；若 User/实施偏好提升 `EmitContext._primWrapper` 可见性亦可，语义等价。

### Decision 3: 最小增量原则（防大面积字节漂移）

**问题：** 主修 B 若误改成**整体替换** `_overloadKey`→`_resolveOverload`（不是"仅落空时追加"），会波及全库**所有** string/int/char prim 实例调用的绑定路径 → 大面积 zbc 字节漂移、自举不动点崩。

**决定：** **B 只在 `wms==null` 时追加 `_resolveOverload`**，既有 `wms!=null` 路径（`:137-140`）一字不动。理由（字节中性论证见 Decision 4）：今天所有 prim 实例方法要么唯一、要么仅 arity 不同（`Split(string)`/`Split(string,int)` → `ovldInst` 命中 → 键 `Split$1`/`Split$2`，`_overloadKey` 恒命中、`wms!=null`），追加分支永不触发。

### Decision 4: 字节中性论证（byte-identical-safe）

**主修 B**：追加分支仅 `wms==null` 时进入。今天 prim 类无同 arity type-based 重载，`wms==null` 只发生于"方法真正不存在"——此时 `_resolveOverload` 收集到 0 候选（`OverloadBinder.z42:201-202`）返回 null → 仍落既有 loose-bind（`:142-143`）→ 输出不变。故现有树上 B **零漂移**。

**辅修 A**：新守卫 `!ownerIsLocalInst` 只在"本地 prim 类的实例调用被 DepIndex 捷径命中"时改变输出。这一组合今天**就是误编译**（本地 String 调用被下游同名劫持）；正常绿树上要么 `owns=true`（本地类自有裸名方法今天命中）不进捷径，要么 owner 非本地（守卫本就为 false-无效）。故正常树上 A 亦零漂移。

> 结论：现有树（无 prim 同 arity 重载）上 gen1==gen2 自举字节不动点应保持——**必须以此为 GREEN 硬门**（Testing）。B 的新行为只在阶段 2 给 String 加同 arity 重载后才显现。

### Decision 5: 两阶段拆分（自举纪律，本 change = 阶段 1 support）

**问题：** 能否一个 change 连编译器修复 + String 方法一起做？

**决定：** **不能——必须两阶段跨两个 nightly**（[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 分阶段引入纪律）：
- **阶段 1（support，本 change）**：只扩 z42c 绑定能力（主修 B + 辅修 A）。z42c 源、stdlib 源自身**不使用** prim 类 type-based 重载 → 上一 nightly z42c 恒能编当前源（`xtask test bootstrap` 恒绿）→ 产出"支持 prim 实例 type-based 重载"的新 z42c → 发 nightly。
- **阶段 2（use，独立 change，晚一个 nightly）**：阶段 1 nightly 发布后，才往 `Std.String` 加 `IndexOf(char)` / `Split(char[])` / `Trim(char)` 等同 arity 重载（那时的种子 z42c 已具备阶段 1 能力，能编）。

> 本 change 唯一"使用"新能力的地方是**临时 e2e fixture**（Testing）——它是测试用例、跑完删除、不进 prelude、不随源码发货，故不违反纪律。
>
> **是否 bump 自举能力版本号**：倾向否（无新语法/格式，纯绑定逻辑）。列 Open Question，待 User/实施定；若定 bump 则同步 `xtask test bootstrap` 的版本校验。

## Implementation Notes（精确到 file:line）

1. **主修 B —— `MemberResolver.z42`**（prim-wrapper 分支，现 `:129-143`）：
   - 在 `:140`（`if (wms != null){...}` 闭合）之后、`:142`（loose-bind）之前，插入 `_resolveOverload` 追加块（伪码见 Decision 1）。
   - 复用 class 路径 `:57-62` 的形态：`_resolveOverload` 决议 → 命中做访问检查（`_access.CheckAccess` / `CheckDeprecatedM`，镜像 `:59-60`，若 prim 路径需要）→ `_withDefaults`（`:61`）填实参 → `BoundCall("instance", true, recv, PrimModel.Keyword(rt.Name()), rms.RegKey, fa, fa.Length, rms.Signature.Ret, sp)`。
   - **务必在 `if (env.Symbols.HasClass(wrapper))`（`:130`）块内**（wct 已取）；仅 `wms==null` 追加，`wms!=null`（`:137-140`）不动。
   - 实参填充选 `_withDefaults` 还是 `FillDeferredArgs`：镜像 class 路径用 `_withDefaults`（`:61`）；但现 prim loose-bind 用 `FillDeferredArgs`（`:142`）。命中真实符号时应与 class 路径一致用 `_withDefaults` —— 实施时对拍确认。

2. **辅修 A —— `CallEmitter.z42`**（实例 DepIndex 捷径，现 `:160`）：
   - 在 `:160` 之前算 `bool ownerIsLocalInst = this._ctx.LocalClasses != null && this._ctx.LocalClasses.ContainsKey(TypeFactsTc._primWrapper(c.OwnerClass));`
   - 捷径条件改 `if (!owns && !ifaceRecv && !virtualRecv && !ownerIsLocalInst && this._ctx.Deps != null)`。
   - 确认 `CallEmitter` 能引用 `TypeFactsTc`（同 `z42c.semantics` 簇；`MemberResolver`/`EmitContext` 均在用）；若命名空间未 using，补 using。

3. **只读依赖**：`MemberCollector.z42:203-211` 的 mangle 逻辑、`OverloadResolver.MangleKey`、`OverloadBinder._resolveOverload/_collectOverloads` 均不改，只被消费。

## Testing Strategy

1. **单元（z42c.semantics tests）**：构造一个含同 arity type-based 实例重载的 prim/类 fixture，断言绑定产出的 `BoundCall.MethodName` = 正确 mangle RegKey、类型 = 真实返回类型（非 Unknown）；再断言"无重载时"键与今天一致（防漂移的绑定级单测）。
2. **gen1==gen2 自举字节不动点（硬门）**：`xtask test compiler`（z42c 自编 z42c）在**现有树**（无 prim 同 arity 重载）上 gen1==gen2 逐字节相同——证 Decision 4 字节中性。**任何漂移 = 违反最小增量，立即排查**（尤其防 Decision 3 的整体替换误改）。
3. **type-based 重载 e2e 实测 vtable 派发（Risk#3，本地不可推、必须跑）**：临时给 `Std.String` 加 `Split(char[])`（与 `Split(string)` 同 arity），写 e2e 对 `string` 值分别调 `Split(string)` 与 `Split(char[])`，**运行期断言各自派发到正确重载**（返回值区分）。→ 坐实"prim 接收者 VM 以 RegKey 为 vtable 派发键"。**测完删除临时 fixture**（不进 prelude、不违反两阶段纪律）。
   - 若实测发现 VM **不以 RegKey** 派发 prim 接收者 VCall → 主修 B 单独不够，需 VM 侧配合 → 停下回报 User 重裁（proposal Open Questions）。
4. **E0436 回归**：在有临时 `Split(char[])` 的树上编 z42.core，断言**不再报** `namespace Std.Regex is used but not imported`（症状消失）。
5. **`xtask test bootstrap`（越界检查）**：上一 nightly z42c 能编当前 z42c 源——本 change 是 support、源不使用新能力 → 恒绿；若红说明源意外用了新能力，须回退。
6. **完整 GREEN**：`xtask test` 全 stage（e2e + cross-zpkg + stdlib + 自举不动点 + vscode-syntax）。
   - ⚠️ GREEN 前 `rm -rf /tmp/z42c-e2e-*`（stale tmp e2e 假失败，见 memory `stale-tmp-e2e-buildtext-false-fail`）。

## Deferred / Future Work

- **阶段 2：String 同 arity 重载补齐**（`IndexOf(char)` / `Split(char[])` / `Trim(char)` / `TrimStart(char)` / `TrimEnd(char)` 等）——独立 change，等本 change 随 nightly 发布后开工（library_review 迭代 String 补齐项）。
- **其它 prim 类型的 type-based 实例重载**（Int32 / Char / Double …）——能力已就绪，随各 library 需求落。
- **自举能力版本号语义**（若本 change 定 bump）——与 `xtask test bootstrap` 版本校验联动，待 Open Question 裁决。
