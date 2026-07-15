# Tasks: 派发键稳定化 + Join 落地 params

> 状态：🟡 实现完成，CI 验证待确认 | 创建：2026-07-14
> 类型：lang/ir/vm（完整流程）。冷环境本地不可验自举链 → GREEN 以 CI 为权威。

## 进度概览
- [x] 阶段 1: 键规则（compiler）
- [x] 阶段 2: 单一真相键 + 审计对齐
- [x] 阶段 3: 格式 bump（zbc 1.27 / zpkg 0.32）
- [x] 阶段 4: VM vtable 键保留 `$` + 反射去 `$`
- [x] 阶段 5: Join 落地 params
- [x] 阶段 6: fixture 版本-patch + 文档
- [x] 阶段 7: 本地可验测试（cargo build + Rust 单测）
- [ ] 阶段 8: CI 全绿（push 后盯：ci-bootstrap 两代自举 / bootstrap-no-csharp / golden regen / 不动点）

## 明细
- [x] 1.1 `SymbolCollector.regName` 恒 MangleKey（豁免名裸）；删兄弟集预扫描
- [x] 2.1 `ExportedTypeExtractor`(实例) / `IrGen`(impl) / `TestIndexBuilder` 优先 `md.RegKey`
- [x] 2.2 `DependencyIndex.AddModule` 注册完整实例键
- [x] 3.1 `ZbcFormat.z42` 1.27 + `ZpkgWriter.z42` 0.32 + reader 常量 + version-pin 测试 + changelog
- [x] 4.1 `types.rs` `derive_simple_method_name` 保留 `$`
- [x] 4.2 `reflection.rs` `build_method_info` 显示名去 `$`
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
