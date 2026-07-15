# Tasks: 派发键稳定化 + Join 落地 params

> 状态：🟡 静态-only 重做完成，CI 验证待确认 | 创建：2026-07-14 | 裁决重做：2026-07-15
> 类型：lang/ir/vm（完整流程）。冷环境本地不可验自举链 → GREEN 以 CI 为权威。

## 进度概览
- [x] 阶段 1: 键规则（compiler）
- [x] 阶段 2: 单一真相键 + 审计对齐
- [x] 阶段 3: 格式 bump（zbc 1.27 / zpkg 0.32）
- [x] 阶段 4: VM vtable 保持基线（实例不 mangle，derive 去 `$`）+ 反射去 `$`（静态方法名）
- [x] 阶段 5: Join 落地 params
- [x] 阶段 6: fixture 版本-patch + 文档
- [x] 阶段 7: 本地可验测试（cargo build + Rust 单测）
- [ ] 阶段 8: CI 全绿（push 后盯：ci-bootstrap 两代自举 / bootstrap-no-csharp / golden regen / 不动点）

## 明细
- [x] 1.1 `SymbolCollector.regName` 静态恒 MangleKey / 实例基线键（保留兄弟集预扫描；静态-only 裁决第四轮重做）
- [x] 2.1 `TestIndexBuilder` 优先 `md.RegKey`（静态 [Test]）；`ExportedTypeExtractor`/`IrGen`(impl) 回退基线
- [x] 2.2 `MemberResolver` prim 静态（int.Parse）用 RegKey；prim 实例/方法组回退基线；`DependencyIndex` 回退基线
- [x] 3.1 `ZbcFormat.z42` 1.27 + `ZpkgWriter.z42` 0.32 + reader 常量 + version-pin 测试 + changelog
- [x] 4.1 `types.rs` `derive_simple_method_name` 保持基线（去 `$`；实例不 mangle）
- [x] 4.2 `reflection.rs` `build_method_info` 显示名去 `$`（静态方法 demangle）
- [x] 5.1 `Path.Join(params string[])`（保留 2-arg）
- [x] 5.2 `String.Join(string, params string[])`（合并取代 string[] + 3-arg）
- [x] 6.1 `zbc_tests.z42` golden hex minor 1a00→1b00
- [x] 6.2 zbc-format(6) + zpkg-format(4+indexed zbc/hash) fixture 版本-patch
- [x] 6.3 `docs/design/runtime/{zbc,zpkg}.md` changelog + 当前版本
- [x] 6.4 `ACTIVE.md` 登记锁
- [x] 7.1 `cargo build --release`（z42vm）✅
- [x] 7.2 Rust 单测：version-pin 2/2 / reflection 15/15 / sidecar 9/9 / loader 48/0 / zbc_compat 3/3 / metadata 177/0 ✅

## 备注
- 冷环境（无 seed、nightly 403）：z42c 自举 / golden regen / `xtask test` 本地不可跑；这些以 CI 为准。
- fixture 版本-patch 合法性：方案 A 不改 wire 布局，仅版本字段 + 字符串内容变 → header 版本-patch 产
  合法 1.27/0.32 文件，loader/compat 不校验方法键。`zbc-format` CI 会以真 z42c 重键覆写；`zpkg-format`
  无 CI 自动 regen，版本-patch 即其正解。
- 中断记录：会话中 change 目录曾被并行 worktree git 竞争清掉（memory reference_shared_worktree_git_race），
  已重建 proposal/design/spec/tasks。

## CI 结果（首轮，2026-07-14，commit e4aece6）
- ✅ **方案A 核心成立**：`compile-toolchain` / `verify-selfhost` 两代自举**完成**——gen2 z42c + stdlib 均 minor=32、gen1==gen2、新 VM 接管。全局重键 + 格式 bump + 两代自举吸收 = 通过。
- ❌ 后续步骤 `[2/5] seed z42c builds current xtask.zpkg` 崩：gen2 z42c（跑在**新编译的方案A stdlib**上）解析 xtask manifest 时
  `FieldGet: not an object or known value type, got Null` @ `Std.Toml.TomlParser.ParseDocument$0` → `TomlValue.Parse$1$string` → `ManifestLoader.ParseText$1$string`。
  - 已排除：ctor 命名/派发一致（IrGen 用 md.RegKey）、`File.ReadAllText` 缺文件是 throw 非 null、TomlParser 无重载/无继承。
  - 结论：新 stdlib 某方法被方案A 微妙误编返回 null（"line 0"=无调试信息）。冷环境**本地不可复现**（无 seed / nightly 403）。
  - 疑点：也可能撞已知 `reference_release_vm_jit_miscompiles_default_params`（release-JIT 误编 → 空输出/null；compile-toolchain 是 release + z42c 默认 JIT）。
- 其余红 job（test-host ×4 / bench-*）为该步失败的级联 + format-bump 当次 download-bootstrap 一次性红（预期，自愈）。

## 根因 + 修复（第二轮，2026-07-15）
**根因**：ctor 调用键仍走旧方案（`_ctorKey`：裸名 / `Name$argCount`）。方案A 下 ctor 一律全签名
mangle（`TomlParser$1$string`），于是 `new C(args)` emit 的 `fqCtor` 与 ctor 函数名不匹配 →
`ObjNew`（exec_object.rs）`func_index.get(ctor_name)` 落空 → **静默不跑 ctor**（无报错）→ 字段停
默认值（`TomlParser._src=null`）→ ParseDocument `_src.Length` FieldGet-on-null 崩。
- **确认是 interp**（ci-bootstrap step [2/5] `--mode interp`）→ 排除 release-JIT，是真误编。
**修复（3 处）**：
- `ExprTyper._bindNew`：ctor 解析改走统一重载决议 `_resolveOverload` → `CtorName = ctor.RegKey`
  （命名实参 AssignExpr 解包绑 value 取类型）；无 MethodSymbol（合成 ctor）回落裸类名（与合成 ctor
  的裸键 `C.C` 一致）。
- `DeclBinder`（base/this ctor 委托）：同样 `_resolveOverload` → RegKey，兜底旧 `_ctorKey`。
- `loader.rs build_type_registry`：ctor-skip 用 demangle（`method` 首段 `$` 前）比 simple_name，
  防 mangled ctor 泄漏进 vtable/反射。
- 本地验：`cargo build` ✅ + loader 48/0（z42c 侧 CI 验）。

## 根因 + 修复（第三轮，2026-07-15，commit 7017b41 之后）
ctor 修复后崩点前移到 `SourceDiscovery._expand` 的 `pattern.StartsWith("**/")`：
`VCall: expected object, got Str("**/")`。**同类根因**：prim 收者（string/int/…）方法解析仍走旧
`_overloadKey`/`_findMethod`（裸名/Name$arity），查不到已全签名 mangle 的方法 → 回落 bare emit →
VM prim 派发（`Std.String.StartsWith` + `$arity` 重试）都落空（函数是 `StartsWith$1$string`）→ 崩。
**修复（MemberResolver.z42，3 处）**：
- prim 实例调用（`string.StartsWith`）：`_resolveOverload` → `wms.RegKey` + `_withDefaults`。
- prim 关键字静态调用（`int.Parse`/`string.FromChars`）：`_resolveOverload` → `wms2.RegKey`。
- 实例方法组 `obj.M`（委托值）：`_collectOverloads` 按源名收候选、携 `RegKey`（thunk 内 VCall 用它派发）。
已核对无遗漏的旧方案派发点：Object-exempt（generic param→Object，裸名 OK）、get_X/add_X 访问器（裸注册
OK）、普通 class/静态调用（早已 `_resolveOverload`+RegKey）、操作符（早已 `_resolveOverload`）。

## 范围裁决 + 重做（第四轮，2026-07-15，全 mangle → 静态-only）
**触发**：全 mangle（commit 9d0dd85）CI 结果——bootstrap 两代自举**全绿**（证明 export/byte-identity
自洽），但 **19 个 e2e golden 挂**，全在**实例派发**子系统（interface / 泛型 / 泛型内原始类型 /
委托·事件 / foreach）。这些子系统多处裸名 emit，全 mangle 后与 mangle 函数不匹配；辐射面横跨 5 个
此前未预料的子系统，冷环境不可本地验、只能逐轮 CI 打地鼠。

**事实校正 → User 裁决（AskUserQuestion 工具连续失败，改 prose 呈报 + "请你修复啊" 授权）**：
`Path.Join`/`String.Join` 皆**静态** → 键不稳定只需在静态维度根治。改走**静态-only**：静态恒 mangle、
实例保持基线键（本变更前独立验证过的绿路径）→ 19 挂点全消、不改 VM、辐射面缩到「静态调用一条路」。

**重做（改哪些）**：
- `SymbolCollector.regName`：静态→恒 `MangleKey`；实例→**恢复基线**（bare / Name$arity / type-overload
  全 mangle / 协议豁免裸）。恢复兄弟集预扫描（`ovldInst`+`arityDup`，`arityDup` 计 static+instance 全量以
  保实例键与基线逐字节一致）。
- `MemberResolver`：prim 实例（site1）/ 实例方法组（site2）**回退基线**；prim 静态 int.Parse（site3）
  **保留** `_resolveOverload`→`RegKey`（静态已 mangle，旧裸键查不到）。
- **回退基线**（git checkout）：`ExprTyper`(ctor) / `DeclBinder`(base·this ctor) / `ExportedTypeExtractor`(实例)
  / `IrGen`(impl) / `DependencyIndex`(实例注册) / `types.rs`(`derive_simple_method_name` 复位去 `$`) /
  `loader.rs`(ctor-skip 复位)。
- **保留**：`reflection.rs`(demangle 显示——静态方法名需去 `$`) / 格式 bump（zbc27/zpkg32 + reader + golden
  hex + fixture）/ Join params（Path/String）/ `TestIndexBuilder`(RegKey——静态 [Test] 需之)。

**本地验**：`cargo build --release` ✅ / lib 772·0 / compression 21·0 / zbc_compat 3·0 ✅。
**待 CI**：e2e golden（静态-only 下实例派发回归基线应全绿）+ 两代自举 + 不动点。
