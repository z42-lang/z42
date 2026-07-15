# Design: 派发键稳定化（方案 A，静态-only）

## 概览

派发键的唯一职责：让「调用点 emit 的字符串」==「被调方注册/导出的字符串」。现行规则让键依赖
**兄弟集**（同名有几个重载、各是什么 arity）→ 键成了「全局集合的函数」，加/删一个重载会让**别处
现有方法的键漂移** → 破坏已编译产物（尤其预编译 seed）的链接。方案 A 把键变成**方法自身签名的纯
函数**，切断此耦合。

**范围裁决（2026-07-15，静态-only）**：`Path.Join` / `String.Join` 目标方法**都是静态方法**，键不稳定
只需在**静态**维度根治。全 mangle（静态+实例都改）的实测辐射面远超预期——见下「全 mangle 回退记录」。
故最终落地：**静态方法恒全签名 mangle；实例方法保持基线键规则不变**。

```
静态:  key(m) = MangleKey(m.sig)         ← 加/删重载只多一个新键，现有静态键不动（根治 Join 破坏）
实例:  key(m) = 基线规则                  ← unique 裸 / arity 重载 Name$arity / type 重载全 mangle / 协议豁免裸
                                            （与本变更前逐字节一致 → 实例/虚/接口/泛型/委托/foreach/ctor 全不动）
```

## D1: 键规则 —— 静态恒 mangle / 实例保持基线

`SymbolCollector.regName`：
- **常规静态**（`mst && !staticVirtual`）：`MangleKey(name, sig.ParamTypes, sig.ParamCount)`，零参 → `name$0`。无条件、不依赖兄弟集。
- **静态虚成员**（`mst && (override||abstract)`）：走**基线键**——INumber 的 `static abstract op_*` 接口成员 +
  各 primitive 的 `static override` 实现经 **VCall 按运行时类型派发**（`x.op_Add(x)` → VCall → `Std.Int32.op_Add`
  + arity 重试），base 声明与派生实现**必须同键**。全 mangle 会因类型参数替换令 base(`op_Add$2$T$T`)≠派生
  (`op_Add$2$i32$i32`)→ VCall 落空（`expected object, got I64`）。故与实例虚方法同理保基线键（op_* 唯一 → 裸名）。
- **实例**（`!mst`）：基线规则原样——`IsProtocolExempt(name) ? name`（协议豁免裸）
  `: arityDup[name$arity]==2 ? MangleKey(...)`（type-based 重载全 mangle）
  `: ovldInst[name] ? name$arity`（arity-based 重载）`: name`（unique 裸）。

实例分支需保留兄弟集预扫描（`ovldInst` 判 arity 重载、`arityDup` 判 type 重载）。`arityDup` **计入
static+instance 全部方法**（实例的 `wantMangle` 判据依赖跨 static/instance 的 name$arity 碰撞，镜像基线）
以保证实例键与基线逐字节一致。静态分支不读预扫描。

**为什么静态恒 mangle 安全**：静态无 vtable 派发；调用点编译期决议后 emit `RegKey`；`IrGen`(类方法,line164)
按 `md.RegKey` 命名 emit 的函数；`MemberResolver` 常规静态调用（line 280）/ prim 关键字静态调用（int.Parse/
string.FromChars, line 294）/ DepIndex 静态注册均以 `RegKey` 为单一真相 → emit==注册键恒一致。协议豁免
不适用于静态（静态无 vtable 硬查锚点）。

## D2: 单一真相键（RegKey）

`md.RegKey` = SymbolCollector 写入的最终键。静态=mangle 键、实例=基线键。消费点：常规实例/接口/静态调用
（`MemberResolver` line 42/67/231/280）本就 `_resolveOverload`→`RegKey`（键方案无关，天然自洽）；prim 静态
（line 294）改用 `RegKey`（静态-only 下 int.Parse 已 mangle，旧 `_overloadKey` 裸名查不到）；`TestIndexBuilder`
优先 `md.RegKey`（静态 [Test] 方法现为 mangle 键，需之）。

**实例侧回退**：prim 实例（`string.StartsWith`, line 91）、实例方法组（`obj.M`, line 172）、ctor（`ExprTyper._bindNew`
/`DeclBinder` base·this）、`ExportedTypeExtractor`(实例)、`IrGen`(impl)、`DependencyIndex`(实例注册) **全部回退到
基线**——实例键=基线键，基线 emit/查找逻辑原样自洽，无需 RegKey 化。

## D3: 格式 bump —— zpkg + zbc 双 bump（User 裁决 2026-07-14）

- `ZbcVersion.Minor` 1.26→1.27、`ZpkgWriterZ.Minor` 0.31→0.32；两端 Rust reader 常量 + version-pin
  cargo 测试同步。wire 布局不变（无新 opcode/section），仅字符串内容（**静态方法键**）随重键改变。
- 静态方法键全量改变 → 旧 seed 的静态调用键与新库不匹配；双 bump 触发 ci-bootstrap 版本差 gate → 两代
  自举整树重键，并让 strict-pin 拒绝残留旧产物（避免「版本未变但字节变」的静默错配）。

## D4: VM vtable —— 保持基线（实例不 mangle → derive 仍去 `$`）

实例方法键=基线，故 VM vtable 侧**回退到基线**：`derive_simple_method_name` 在 `$` 处截断
（`Foo$2`→`Foo`），VCall 对实例方法 emit 基线键（line 42 经 `RegKey`=基线键）→ 与 vtable 槽键一致。
**无需保留 `$`、无需改 VM vtable 派发**（全 mangle 时才需，静态-only 不需）。

**反射展示（保留 demangle）**：`build_method_info` 的 `MethodInfo.Name` 反向去 `$`
（`Join$1$string`→`Join`）——静态方法现为 mangle 键，反射需展示源级名。实例 vtable 名已基线裸名，
demangle 对其为 no-op。派发键与反射显示名正交。

## D5: 硬编码裸名审计（静态-only 结论）

| 面 | 静态-only 下 | 处理 |
|----|-----------|------|
| VM `CallInstr`/`VCall` 派发 | 纯字符串匹配，emit==注册键即命中 | 无需改 |
| VM vtable 硬查 `ToString`（dispatch/jit）| 豁免→裸名不变 | ✅ |
| VM vtable 槽键派生 | **基线**（去 `$`；实例不 mangle） | ✅ 回退 `derive_simple_method_name` |
| DepIndex/TsigReconcile 协议名单 | 与 `IsProtocolExempt` 同集 | ✅ 一致 |
| 反射 by-name 显示 | 去 `$`（静态方法名 demangle） | ✅ `build_method_info` 保留 |
| entry（`Main`）/ 自由函数 | 自由函数不 mangle | ✅ 不受影响 |
| **静态方法**（含 prim int.Parse）| 恒 mangle，emit/注册同源 `RegKey` | ✅ 自洽（D1/D2） |
| **实例/虚/接口/泛型/委托/foreach/ctor** | 基线键，全不动 | ✅ 与本变更前逐字节一致 |

## D6: `Path.Join` / `String.Join` 落地

键稳定后（静态维度）加 `params` 重载不再 re-mangle 现有静态键。`Path.Join(params string[])` 新增
（保留 2-arg 热路径）；`String.Join(string, params string[])` 合并取代 `Join(string, string[])` +
`Join(string,string,string,string)`——单 params 重载同时覆盖 normal form（`Join(sep, arr)` 直传）与
expanded form（`Join(sep,a,b,c)` 打包）。二者皆静态 → 正是静态-only 覆盖的目标。

## 全 mangle 回退记录（2026-07-15，静态-only 裁决依据）

初版实施「静态+实例全 mangle + 改 VM vtable（保 `$`）+ ctor/prim/方法组随之 RegKey 化」。CI（commit
9d0dd85）结果：

- ✅ **bootstrap 两代自举全绿**（`compile-toolchain`/`verify-selfhost`：gen2 z42c+stdlib 均 minor=32、
  gen1==gen2、新 VM 接管）→ 证明 export/OwnMethod/DepIndex/byte-identity 在全 mangle 下自洽。
- ❌ **19 个 e2e golden 挂**，全在**实例派发**子系统：interface（`Demo.Dog.Name not found`）、泛型
  （`Num.CompareTo not found`）、泛型内原始类型（`expected object, got I64`）、委托/事件（`FuncRef` 类型
  不匹配）、foreach（`ArrayLen: expected array, got Object(Ring)`）。根因同类：这些子系统在多处**裸名
  emit**，全 mangle 后与 mangle 函数不匹配——辐射面横跨 5 个此前未预料的派发子系统，冷环境本地不可验、
  只能逐轮 CI 打地鼠。

裁决：静态-only。目标（Join 静态）达成；实例侧回退基线（本变更前独立验证过的绿路径）→ 19 挂点全消、
不改 VM。全 mangle 的实例键稳定收益（override 独立槽等）非本变更目标，留待独立变更按分阶段纪律推进。

## 两代自举吸收重键（bootstrap）

沿用 `fix-bootstrap-format-bump-deadlock` D7：zpkg 31≠32 → 版本差 gate → 旧 VM 跑旧 seed z42c 编当前
源（旧 runtime stdlib）产 gen1 → 旧 VM 跑 gen1 产新格式全栈（gen2，静态键重键）→ 新 VM 接管。整树 gen2
一次性重键静态键 → caller/callee 同批新键、自洽；实例键不变。

## 冷环境 fixture 处理

方案 A **不改 wire 布局**（只改静态方法键字符串 + 版本字段）→ committed 二进制 fixture 可**版本-patch**
成合法 1.27/0.32 文件。`zpkg-format` 无 CI 自动 regen（版本-patch 是其在 CI 也生效的正解，loader 测试
不校验方法键）；`zbc-format` CI 会以真 z42c 重键覆写。fixture 内方法键非静态-only-正确不影响任何断言
（loader/compat 只校验解析/加载/拒绝，不校验键）。

## Deferred

- 三处协议名单（semantics `IsProtocolExempt` / project `TsigReconcile._isProtocol` / ir DepIndex
  `_isProtocol`）合并到公共落点——承接 P3-3，本变更先对齐语义，合并留后续 refactor。
- **实例维度键稳定化**：若未来需要（加/删实例重载破坏 bootstrap），按分阶段纪律（support 先行、晚一
  nightly 再 use）单独推进全 mangle + VM vtable 保 `$`——本变更故意不含，因辐射面大且非 Join 所需。
- z42c/xtask 源码里 `Path.Join(Path.Join(a,b),c)` 嵌套写法改用 params 平铺——受分阶段引入纪律约束
  （晚一个 nightly），不在本变更内改 z42c 源调用点。

## Testing

- 本地（可验）：`cargo build` ✅；lib 单测 772/0、compression 21/0、zbc_compat 3/0、reflection/version-pin ✅。
- CI（权威）：`ci-bootstrap` 两代自举、`bootstrap-no-csharp`、全 golden regen 一致、z42c 不动点
  gen1==gen2、**e2e golden 全绿**（静态-only 下实例派发回归基线）、`typecheck_params_tests` 覆盖 Join
  normal/expanded。
