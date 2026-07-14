# Design: 派发键稳定化（方案 A）

## 概览

派发键的唯一职责：让「调用点 emit 的字符串」==「被调方注册/导出的字符串」。现行规则让键依赖
**兄弟集**（同名有几个重载、各是什么 arity）→ 键成了「全局集合的函数」，加/删一个重载会让**别处
现有方法的键漂移** → 破坏已编译产物（尤其预编译 seed）的链接。方案 A 把键变成**方法自身签名的纯
函数**，切断此耦合。

```
现行:  key(m) = f(m, siblings(m))     ← 加重载 → 现有键漂移 → bootstrap 破坏
方案A: key(m) = MangleKey(m.sig)        ← 加重载 → 只多一个新键，现有键不动
                例外: !static && IsProtocolExempt(m.name) → 裸名（VM/编译器硬查锚点）
```

## D1: 键规则 —— 恒全签名 mangle + 协议豁免裸名

`SymbolCollector.regName`：`!mst && IsProtocolExempt(name) ? name : MangleKey(name, sig.ParamTypes,
sig.ParamCount)`。零参 → `name$0`。删除原兄弟集预扫描（seen/ovld/arityDup 不再需要）。

豁免名（`ToString`/`Equals`/`GetHashCode`/`GetType`/`get_Item`/`set_Item`）保持裸名——它们是跨语言
协议锚点，被 VM vtable / DepIndex / TypeChecker 按裸字面量硬查；这些名在源语言层天然唯一（不重载），
裸名不产生歧义，与键稳定不冲突。仅**实例**豁免（静态无 vtable 派发）。

## D2: 单一真相键（消 P3-3 重复）

`md.RegKey` 是 SymbolCollector 写入的最终键。所有消费点统一「RegKey 非空则用之，否则回落既有
Name$arity/bare 探测」——`ExportedTypeExtractor`(实例)、`IrGen`(impl 方法)、`TestIndexBuilder` 从
「重算」改为「读 RegKey」（`IrGen`(类方法)/`DeclBinder` 早已如此）。impl/接口方法 RegKey 为空 →
保持既有裸/arity 行为，与自身 emit/查找自洽（每方法键三处一致即可，不要求全树同风格）。

`DependencyIndex.AddModule`：实例方法除 bare/bare$arity 外，**再注册完整 method 键**，使调用方按
`md.RegKey`（全签名键）发的 `GetInstance` 兜底能命中。

## D3: 格式 bump —— zpkg + zbc 双 bump（User 裁决 2026-07-14）

- `ZbcVersion.Minor` 1.26→1.27、`ZpkgWriterZ.Minor` 0.31→0.32；两端 Rust reader 常量 + version-pin
  cargo 测试同步。wire 布局不变（无新 opcode/section），仅字符串内容随重键改变。
- 双 bump（而非只 zpkg）杜绝「版本未变但字节变」的 golden/fixture 误判。触发 ci-bootstrap 版本差
  gate → 两代自举整树重键。

## D4: VM vtable 键必须保留 `$`（方案 A 的 VM 侧硬要求）

虚方法调用 `VCall` 传 `ms.RegKey`（全 mangle）；VM vtable 槽键由 `derive_simple_method_name` 从
qualified func 名派生。原实现在 `$` 处截断（`Foo$1$int`→`Foo`）→ 全实例 mangle 后 VCall 传
`Foo$1$int` 而 vtable 键仍 `Foo` → **未命中 → 虚派发崩**。故改为**保留 `$`**（vtable 键 = 全 mangle
键）：

- VCall `Foo$1$int` ↔ vtable `Foo$1$int` 一致命中；
- override 由「签名纯函数」保证 base/derived 同键 → 同槽（正确）；
- 重载虚方法 `Foo$1$int`/`Foo$1$string` 各占独立槽（修正原「塌成一个 Foo 槽」的缺陷）；
- 豁免名/属性索引器访问器（`ToString`/`get_X`/`get_Item`…）无 `$` → 裸槽键不变，与其裸 VCall 名一致。

**反射展示分离**：`build_method_info` 的 `MethodInfo.Name` 反向去 `$`（`Foo$1$int`→`Foo`），派发仍用
qualified 名。派发键（含 `$`）与反射显示名（去 `$`）是两个正交面。

## D5: 硬编码裸名审计（结论）

| 面 | 方案 A 下 | 处理 |
|----|-----------|------|
| VM `CallInstr`/`VCall` 派发 | 纯字符串匹配，emit==注册键即命中 | 无需改 |
| VM vtable 硬查 `ToString`（dispatch/jit）| 豁免→裸名不变 | ✅ |
| VM vtable 槽键派生 | 保留 `$`（D4） | ✅ 改 `derive_simple_method_name` |
| DepIndex/TsigReconcile 协议名单 | 与 `IsProtocolExempt` 同集 | ✅ 一致 |
| 反射 by-name 显示 | 去 `$`（D4） | ✅ 改 `build_method_info` |
| entry（`Main`）/ 自由函数 | 自由函数不 mangle（`Main` 是自由函数）| ✅ 不受影响 |
| ctor | 走 regName（全 mangle），emit/def 同源 `md.RegKey` | ✅ 自洽 |

## D6: `Path.Join` / `String.Join` 落地

键稳定后加 `params` 重载不再 re-mangle 现有键。`Path.Join(params string[])` 新增（保留 2-arg 热
路径 + normal-form 直传目标，非 params 精确 arity 重载优先决议）；`String.Join(string, params
string[])` 合并取代 `Join(string, string[])`（同签名不可并存）+ `Join(string,string,string,string)`
——单 params 重载同时覆盖 normal form（`Join(sep, arr)` 直传）与 expanded form（`Join(sep,a,b,c)`
打包）。

## 两代自举吸收重键（D4/bootstrap）

沿用 `fix-bootstrap-format-bump-deadlock` D7：zpkg 31≠32 → 版本差 gate → 旧 VM 跑旧 seed z42c 编当前
源（旧 runtime stdlib）产 gen1（方案A 逻辑）→ 旧 VM 跑 gen1 产新格式全栈（gen2，方案A 键）→ 新 VM
接管。整树 gen2 一次性重键 → caller/callee 同批新键、自洽；旧 seed 只用自带旧 runtime stdlib、不与
新键库链接 → 无 undefined。实测该路径 5+ 次真实 bump 全绿。

## 冷环境 fixture 处理

方案 A **不改 wire 布局**（只改字符串内容 + 版本字段）→ committed 二进制 fixture 可**版本-patch**
成合法 1.27/0.32 文件（`zbc-format` header `1a00→1b00`；`zpkg-format` outer `1f00→2000` + indexed 内
zbc minor + 重算 BLAKE3-128 内容 hash）。`zpkg-format` 无 CI 自动 regen（版本-patch 是其在 CI 也生效
的正解，loader 测试不校验方法键）；`zbc-format` CI `xtask test all` 会以真 z42c 重键覆写（本地 patch
仅为本地 `zbc_compat` 绿 + 保持树内一致）。fixture 内方法键非方案A-正确不影响任何断言（loader/compat
只校验解析/加载/拒绝）。

## Deferred

- 三处协议名单（semantics `IsProtocolExempt` / project `TsigReconcile._isProtocol` / ir DepIndex
  `_isProtocol`）合并到公共落点——承接 P3-3，本变更先对齐语义，合并留后续 refactor。
- z42c/xtask 源码里 `Path.Join(Path.Join(a,b),c)` 嵌套写法改用 params 平铺——受分阶段引入纪律约束
  （晚一个 nightly），不在本变更内改 z42c 源调用点。

## Testing

- 本地（可验）：`cargo build` ✅；version-pin / reflection / sidecar / loader / zbc_compat Rust 单测 ✅。
- CI（权威）：`ci-bootstrap` 两代自举、`bootstrap-no-csharp`、全 golden regen 一致、z42c 不动点
  gen1==gen2、`typecheck_params_tests` 覆盖 Join normal/expanded。
