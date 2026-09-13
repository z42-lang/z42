# Proposal: 给「零构造器」一个可区分的编码 —— 关掉站点 ③ 的 `argc == 0` 缺口

> **状态：📝 DRAFT（待 User 裁决）** | 创建：2026-09-13
> 前置：`fix-silent-symbol-resolution`（PR #614）、`fix-ctor-arity-skew`（PR #620）均已合并。
> 本 change 是那条线**唯一还开着的**一项（见 memory `dep-version-skew-program` 的「② 的墙」）。

## Why

`ObjNew` 的构造器解析不到时，runtime 今天的判据是**有没有实参**：

```rust
// src/runtime/src/vm_context/symres.rs:139
if argc == 0 { return None; }      // ← 缺口：零实参一律按「本来就无 ctor」放行
```

之所以只能这么判，是因为**编译器发的名字本身分不出两种情况**：
`stabilize-instance-dispatch-keys` 规定 primary 构造器占**裸键**，于是

| 源码 | ObjNew 的 ctor 名 |
|------|------------------|
| `class C { }`（零构造器） | `Ns.C.C` |
| `class C { C() {…} }`（单构造器） | `Ns.C.C` |

**同形**。运行期拿到「`Ns.C.C` 解析不到」这一个事实，无法区分「这个类本来就没有构造器」与
「构造器本该在、但装的包比编译时旧」。于是 `new C()` 在版本 skew 下**静默**返回一个全零字段的
对象，错误现场离根因任意远 —— 正是 `philosophy.md` 点名的反例。

> 站点 ⑤（`fix-ctor-arity-skew`）修的是「键解析**到了**但撞上错的重载」；本 change 修的是
> 「键**解析不到**且零实参」。两者互补，合起来才把 `ObjNew` 的符号完整性判定补全。

## What Changes（User 2026-09-13 裁决后的 v2）

**给 `ObjNew` 加一个正向位 `CtorKnown`：编译器在整包装配后，凡是能**看见**该构造器函数的站点
就置位。**于是 runtime 的判据变成：

| `CtorKnown` | ctor 名解析不到时 |
|-------------|------------------|
| `true`（编译时确实看见了这个构造器） | **定案缺失** ⇒ 抛 `MissingSymbolException`（无论 argc） |
| `false`（编译时就没看见 / 裸分配 / 证不出来） | 照常零初始化（保守放行） |

### 为什么是**正向**位，而不是「空 ctor 名 = 零构造器」

初版 DRAFT 提议复用 `IrLoopAllocReuse` 的空名编码（零构造器 ⇒ 发空名），**User 否决**，
理由成立：那个编码会**新引入一种静默** —— 编译时依赖 v2 的 `class C { }`（发空名），
运行时装到 v1 的 `class C { C() {…} }` ⇒ v1 的构造器被悄悄跳过。

正向位没有这个问题：**位的缺席是保守态**。编译时没看见构造器 ⇒ 不置位 ⇒ 名字原样保留 ⇒
运行时若真有就照常调用（今天的行为，不退化）。

三种状态（已证实 / 确实零构造器 / 证不出来）只有三态编码能表达，而 ctor 名字段只能表达两态
（原名 / 改名，且改名必然破坏解析）⇒ **必须加 wire 位**，没有第三条路。

### 判据与落点：整包装配后的 IR fixup（User 裁决覆盖本地类）

落在 `PackageCompile` 的 ⑩ 组装点 —— 那里同时握有**本包全部 `IrModule`**（含增量 cached）
与 `DependencyIndex`：

```
allFns = ⋃ 本包每个 IrModule 的全部已发射函数名
对每条 CtorName 非空的 ObjNewInstr：
    CtorKnown = allFns.Contains(CtorName) || Deps.Statics.ContainsKey(CtorName)
```

- **本地类也覆盖了**（User 裁决）：`allFns` 是本包的精确 oracle，合成构造器
  （`_emitSynthCtor`）本身就是一个已发射函数，天然在里面 —— memory 记的那面墙
  （「合成 ctor 不是 MethodSymbol」「`_synthCtors` 跑在绑定之后」「本 CU 看不到同包其它
  文件」）**在装配点全部消失**，因为那时所有文件的 IR 都已发射完毕。
- **增量安全**：fixup 每次装配都**重算**（不是只置位、不是 OR），故「A 文件缓存着旧结论、
  B 文件后来改了构造器」会被自动纠正。
- **imported 走 `Deps.Statics`**：`AddModule` 用 `Statics.TryAdd(name, entry)` 把每个依赖包的
  每一个已发射函数按完整 FQ 注册，精确且现成。

## 实测覆盖面 —— 装配点普查（`f092b97ef` / `4a5459a14`）

把 `CtorKnownFixup` 的判据原样搬成一个只打印不置位的 census pass，挂在同一个装配点，
跑完整 `build compiler` + `build stdlib`：

| 归类 | 站点数 | 判定 |
|------|-------:|------|
| `pkg`（本包已发射函数里找得到） | 2426 | 置位 |
| `dep`（`DependencyIndex` 里找得到） | 1361 | 置位 |
| `NONE`（非空名、哪儿都找不到） | 106 | 不置位 |
| `empty`（`IrLoopAllocReuse` 裸分配） | 5 | 不置位 |
| **合计** | **3898** | **置位率 97.2%** |

**106 个 `NONE` 只来自 13 个类，逐个核过全是货真价实的零构造器**（`Z42ErrorType` /
`Z42VoidType` / `YamlValue` / `JsonValue` / `TomlValue` / `NoReplCompiler` / `WorkloadBase` /
`NoCompiler` / `BuildHooks` / `JsonMember` / `LoopCfg` / `ForwardGenerator` / `YamlWriter`
—— 既无显式构造器，也无字段初始化器故不会合成隐式构造器；`JsonValue` 那批看着像构造器的是
**静态工厂**）。⇒ **零误判**，本地类的名字口径（design D4 标的风险）就此实测排除。

**本仓自身几乎不产生阳性证据** ——「置位但运行期解析不到」在本仓永远不发生（包是自洽的）。
memory `e0456-声明位` 的教训直接适用：新诊断在现有代码上触发 0 次 = 零证据 ⇒ 必须自带 fixture，
并做退回对照。已做，见下。

## 退回对照（原封 main，`4a5459a14`）

三条 fixture 在**未改动的编译器**上跑 `xtask test e2e --dir cross-zpkg`：

```
  ── ctorless_objnew_skew ──
    expected: caught MissingSymbolException|after
    actual:   constructed 0|after            ← 正是要修的那个静默错误答案
  FAIL ctorless_objnew_skew
  PASS ctorless_objnew_present
  PASS ctorless_objnew_absent
  Total: 42 passed, 1 failed
```

阳性条 FAIL、两条对照 PASS ⇒ 门有判别力，不是空门也不是恒红。

## 格式 bump（User 裁决：做，GREEN 以 CI 为准）

wire 加位 ⇒ zbc `1.38 → 1.39`、zpkg `0.43 → 0.44`。

- **CI 侧可直接做**：两代自举回归已由 PR #383 修好（根因 = `DepScanCache` 无 mtime 的 stale
  cache），探针 PR #385 实测 `compile-toolchain` 绿。
- **本地无法全绿**（已知环境墙）：格式常量住 `z42.ir`，z42c 运行时用的是**已编译好的那份**
  ⇒ gen1 只能产「旧壳 + 新常量」、gen2 才写新格式，两代都要在能读旧格式的 VM 下跑；
  macOS 本地两代自举实测多轮均败（`unify-type-identity-fqn` 正因此裁决不 bump）。
- **绕法**（`escape-stack-format-bump-ci-learnings` §3）：committed fixture
  （`src/tests/zpkg-format/{packed,indexed}-minimal/`）用**临时 CI 步骤**在已自举出新工具链的
  `compile-toolchain` job 里重生 + `upload-artifact`，本地 `gh run download` 取回放置，
  用本地新格式 cargo z42vm 验证后 commit 并删临时步骤。
- **冷路径的 GREEN 判定以 CI 为准** —— `bootstrap-seed.md` 本就明写这一条。

## Scope

- **改**：`IrInstrObject.z42`（加字段 + Dump）、`ZbcInstr.z42` / `ZbcReaderInstr.z42`（wire 对称）、
  新 `CtorKnownFixup`（整包 fixup）、`PackageCompile`（接线）、格式常量双 bump；
  Rust 侧 `ObjNewInsn` / `instr_decode.rs` / `exec_object.rs` / `jit/helpers/object.rs` /
  `symres.rs` / strict-pin 常量。
- **不改**：ctor 名的取值规则（发射端一个字符都不动 ⇒ 除新增尾字节外零语义漂移）、
  `IrLoopAllocReuse`（它的空名裸分配天然 `CtorKnown=false`）。
- **判据合并而非替换**：runtime 保留今天的 `argc > 0` 规则作为并集下限
  （`if !ctor_known && argc == 0 { return None; }`），只增覆盖、不减。
