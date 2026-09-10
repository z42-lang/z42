# Design: 测试流程系统化（参考 Rust / Cargo）

> **触发**：`[Test]` 贴实例方法编译期不报错（[enforce-test-attr-placement](../enforce-test-attr-placement/design.md)）
> 只是症状。往上追一层，问题是**测试代码在 z42 里没有"编译期身份"**——它靠目录约定与产物隔离，
> 而不是靠机制。本设计把整条测试流程系统化。
> **参考系**：Rust/Cargo（`#[cfg(test)]` / unit vs integration test / 一个 crate 一个测试二进制）。

---

## 0. TL;DR

**现状全景**：`[Test]` → 编译器识别（HandlerRegistry）→ TestIndexBuilder 建 TIDX 表 → 写进 zbc 的
**专属 TIDX section** → z42b（z42.builder.zpkg，z42 自己写的反射式 runner）`ModuleLoader.Load` 读回
→ 按 FQN `__invoke_static` 调用。目标模型（`[[test]]`/`[[bench]]`/`[[example]]` + 约定发现 + 三层
dev-deps）已经相当 Cargo-like。

**七个问题**（§3，按严重度）：

| | 问题 | 严重度 |
|---|---|---|
| P0 | `[Test]` 无位置/签名校验 → 运行期 arity 崩 | 🔴 已单独立项 |
| P1 | **没有条件编译**——"测试不进产物"靠目录约定，不靠机制 | 🔴 |
| P2 | **测试是跨包消费者，看不见 `internal`** → 无法单元测试实现细节 | 🔴 |
| P3 | 测试发现靠**源码子串扫描**（`.Contains("[Test]")`）| 🟡 |
| P4 | 编译粒度 = **一个测试文件一个 zpkg**（每单元一次完整 bootstrap）| 🟡 |
| P5 | 测试知识渗入编译器 **12 个文件 + 一个 zbc section 的版本史** | 🟡 |
| P6 | 规则归属错位 + analyzer 纯 opt-in（0/28 包声明）| 🟡 |
| P7 | 结构化输出退化（z42b 只有 pretty + exit code）| 🟢 已记录 |

**已定**：测试产物 kind **由 harness 决定**（true→lib / false→exe），与父项目 kind 无关；
**exe 项目允许写单元测试**（cfg 剪枝自动保证它们不进发布产物，无需另立禁令）。§4.8–4.9。

**核心设计**：引入 **`[Cfg("test")]` directive**（声明级条件编译，复用既有 directive 机制），
并让 **`[Test]` 隐含 `cfg(test)`**。一击解决 P1 + P2 + 大半 P4：

- release 构建：`[Test]` 声明**整体剪除**，不写 TIDX ⇒ *"默认不进 zpkg"*，与 Rust 一致；
- test 构建：保留 ⇒ 测试与被测代码**同包**编译 ⇒ **看得见 `internal`**（Rust `mod tests` 的能力）；
- 顺带成为 **TIDX 退休的前置**（§4.6）。

---

## 1. 现状全景（实测）

### 1.1 数据流

```
.z42 源码            z42c                                   .zbc/.zpkg              运行
────────            ────                                   ──────────              ────
[Test]        ①HandlerRegistry.IsTestHandlerAttr 认 8 个名
void f() {}   ②BenchmarkDesugar（form-2 → 零参 wrapper）
              ③TestIndexBuilder 扫 CU → IrTestEntry[]  ──►  TIDX section  ──►  z42b
              ④IrGen/ZbcWriter 写 TIDX（v1→v2 版本史）      （method_id,       ↳ ModuleLoader.Load
                                                            kind, flags…）      ↳ __invoke_static(FQN)
```

runner 是 **z42b**（`z42.builder.zpkg`，用 z42 自己写的 `Std.Test.Runner`，经 z42vm 运行；
retire-test-runner 2026-06-30 取代了原 Rust runner）。

### 1.2 目标模型（已经很 Cargo-like）

```toml
[tests]                      # 段级：约定扫描开关 + include/exclude glob + Auto
[tests.dependencies]         # dev-dependencies（"release zpkg 元数据不含"）
[[test]]                     # 显式目标：name / harness / entry / sources / deps
name = "unit_ok"
harness = true               # true → z42b 反射跑 [Test]；false → 跑 entry Main，退出码判定
[[bench]] [[example]]        # 同构；example 另有 test=true 决定是否纳入 xtask test
```

三层 dep 合并（`[dependencies]` → `[tests.dependencies]` → `[[test]].dependencies`）已实现，
文档明写 *"仅编译 test/bench/example 时合入，release zpkg 元数据不含"*
（[TargetSection.z42](../../../../src/libraries/z42.project/src/TargetSection.z42)）。

### 1.3 编译单元模型 —— **每个测试单元是一个独立的包**

[xtask_test_lib.z42:216](../../../../scripts/test/xtask_test_lib.z42#L216) 写死了合成 manifest 的形状：

```toml
[project]     name = "<lib>.test.<unit>"   kind = "exe"   version = "0.1.0"
[sources]     include = ["<abs>/<unit>/**/*.z42"]
[dependencies] = parent.[dependencies] ∪ parent.[tests.dependencies]
[build]       output_dir = "<work>/<lib>/<unit>"     # 强制隔离
```

即：**测试单元 = 一个新包，被测库是它的依赖**。

---

## 2. 与 Rust 的逐项对照

| 维度 | Rust / Cargo | z42 现状 | 差距 |
|---|---|---|---|
| **测试进不进产物** | `#[cfg(test)]` —— release 编译里那段代码**根本不存在** | 靠"测试放 `tests/`、`[sources]` 只收 `src/`"的**目录约定** | 🔴 `src/` 里写 `[Test]` 会照常编译进库 zpkg + 写 TIDX，**无警告** |
| **单元测试（看得见私有）** | `#[cfg(test)] mod tests` 与被测代码**同 crate** → 见 `private` | 测试是**独立包**、父库是依赖 → 只见 `public`（`internal` 跨包被 `enforce-crosspkg-internal-class` 拦） | 🔴 **不存在这一层** |
| **集成测试（只见公开面）** | `tests/*.rs` = 每文件一个独立 crate | `tests/` 目录 + 合成包 | ✅ 等价 |
| **测试编译粒度** | 一个 crate 的全部 unit test 编成**一个**测试二进制 | **一个测试文件一个 zpkg**，每单元一次完整 `z42.core` bootstrap | 🟡 编译时间 ∝ 测试文件数 |
| **dev-dependencies** | `[dev-dependencies]` | `[tests.dependencies]` + per-target `deps`，三层合并 | ✅ 等价（甚至更细） |
| **测试发现** | `#[test]` + 编译器生成 harness main；**rustc 无"测试 section"格式** | 编译器建 **TIDX section**（zbc 专属段 + v1→v2 版本史） | 🟡 测试语义渗进二进制格式 |
| **发现的判据** | 属性，编译期语义 | `File.ReadAllText(f).Contains("[Test]")` **子串扫描**（[xtask_test_lib_units.z42:99](../../../../scripts/test/xtask_test_lib_units.z42#L99)） | 🟡 注释里提一句就算数 |
| **签名约束** | `#[test] fn` 必须 `fn()` 或返回 `Termination`；**rustc 硬报错** | **无校验**（本来有，自举迁移丢了） | 🔴 P0 |
| **条件编译** | `cfg` 是通用机制（`target_os` / `feature` / `debug_assertions`…） | **完全没有** | 🔴 |
| **exe/bin 里的单元测试** | 允许（`cargo test` 编 bin 的 test 版） | 目前无此概念（测试一律另起包） | ✅ S3 后允许（§4.9） |
| **输出格式** | libtest：`--format json`、`--filter`、`--nocapture` | z42b：pretty + exit code（`--format json` 仅 bench 路径） | 🟢 已记录为 Deferred |

---

## 3. 问题清单

### P0（🔴）`[Test]` 无位置/签名校验

已单独立项：[enforce-test-attr-placement](../enforce-test-attr-placement/design.md)。摘要——`[Test]` 贴实例方法编译通过，
TestIndexBuilder 照写 TIDX entry，运行期撞 `expects 1 argument(s) (incl. receiver), got 0`。
这套校验（E0911/E0912/E0915）本来有，随 C# 编译器退休一起丢了，而 error-codes.md 至今写着"已启用"。
**同一失败模式两年内第二次**（`BenchmarkDesugar` 也是这么丢的、这么被发现的）。

### P1（🔴）没有条件编译——隔离靠纪律不靠机制

**实测**：全仓 `src/libraries/*/src/` + `src/compiler/*/src/` 下**零内联 `[Test]`**——约定被遵守。
但那是纪律：今天在库的 `src/` 里写一个 `[Test]`，编译器会照常收它、写 TIDX、打进发布 zpkg，
**没有任何警告**。z42 没有任何形式的条件编译（无 `cfg`、无 `#if`）。

Rust 的 `#[cfg(test)]` 是**编译期开关**：release 构建里那段代码不存在——不是"被过滤掉了"，是压根没编。

### P2（🔴）测试看不见 `internal`——而且这是 P1 的连带伤害

因为没有 cfg，唯一的隔离手段就是**另起一个包**（§1.3）。代价是：测试成了被测库的**跨包消费者**，
而 z42 的 `internal` 是包作用域且跨包强制（`enforce-crosspkg-internal-class`）。

⇒ **z42 今天无法对实现细节做单元测试**，只能测公开面。而 Rust 里 `#[cfg(test)] mod tests` 看得见
`private`——那是 Rust 最常用的测试形态。

### P3（🟡）测试发现靠源码子串扫描

```z42
// xtask_test_lib_units.z42:95
bool _dirHasTestMethods(string dir) {
    foreach (var f in Directory.Enumerate(dir)) {
        if (File.ReadAllText(Path.Join(dir, f)).Contains("[Test]")) { return true; }
    }
}
```

把目录下每个 `.z42` **全文读进内存**，只为判断有没有测试；判据是**子串**——注释里写一句 `[Test]` 就算数。

### P4（🟡）一个测试文件一个 zpkg

每单元一次完整 `z42.core` bootstrap。xtask 自己的注释承认这是主要成本
（*"parallelizing the dominant per-unit z42.core bootstrap across units"*），
靠并行批 + "第一次编译退出 0 但没产物就重试"来掩盖。Rust 把一个 crate 的全部 unit test 编成**一个**二进制。

### P5（🟡）测试知识渗入编译器

12 个文件写死 test 家族的名字/语义（TestIndexBuilder 19 处、ZbcReader 13、HandlerRegistry 11、
ZbcWriter 10、IrGen 9、IrModule 5、IncrementalDriver 4、…）+ **一个专属 zbc section（TIDX，v1→v2）**
+ 运行时 decoder。rustc 没有"测试 section"这种东西。

**终态已记录**（[HandlerRegistry.z42:15](../../../../src/compiler/z42c.semantics/src/HandlerRegistry.z42#L15)）：
*"TestIndexBuilder 的终态是 store-meta + 反射发现、TIDX 退休"*。

### P6（🟡）规则归属 + analyzer 纯 opt-in

`[Test]` 的规则理应归 z42.test（"谁的规则谁处理"），但今天它是编译器的（P5）。
而把校验做成 analyzer 又不行：[Main.z42:306](../../../../src/compiler/z42c.driver/src/Main.z42#L306)
是 `if (pm.AnalyzerCount > 0)`——**纯 opt-in，无内建/默认集**。实测：带 tests 目录的包 **28** 个，
声明 `[analyzers]` 的 **0** 个。

### P7（🟢）结构化输出退化

z42b 只产 pretty + 退出码；`--format json` 只在 bench 路径。TAP/JUnit/JSON/`--filter` 是被取代的
Rust runner 的能力。testing.md 自己记着这条。

---

## 4. 设计

### 4.1 核心：`[Cfg("...")]` directive —— 给声明一个编译期条件

**最小、且与 z42 既有机制严丝合缝**：`[Cfg]` 是一个 **directive**（名字识别、无 backing 类、
`KindOf → Directive`，同 `[Native]`/`[Suppress]`/`[Record]`/`[Deprecated]`）。
**零新 token、零新 AST 节点**——`AttributedDecl` 已经在那儿。

```z42
[Cfg("test")]
class StringSplitTests {
    [Test] public static void splits_on_empty() { ... }
}

[Cfg("debug")]  void _dumpState() { ... }
```

**语义**：cfg 未激活 → 该声明在 **AST 阶段整体剪除**，后续所有 pass 都看不见它
（不进符号表、不 typecheck、不 emit、不进 zbc）。这与 Rust 一致：**不是过滤产物，是根本没编**。

**落点**：`HandlerRegistry.RunAst` 里加一道 `CfgPrune.Run(cu)`，排在
`BenchmarkDesugar` / `AttributeSynth` **之前**（剪掉的东西不该被脱糖、更不该合成反射工厂）。

**cfg 从哪来**：`z42c --cfg test`（可重复）+ manifest。v1 只需三个内建名——
`test` / `bench` / `debug`；表达式（`all()`/`any()`/`not()`）留后续。

> **为什么不是 `#if` 预处理**：z42 的 `#` 通道已经有 `#suppress`/`#restore`（PR3c），是**语句/声明列表
> 边界的指令**，不是词法级预处理。做成声明级 directive 与既有模型一致，且天然只能整条声明地开关——
> 这正是 Rust `#[cfg]` 的粒度，避免了 C 预处理器那种"半个函数被 ifdef 掉"的病。

### 4.2 `[Test]` 隐含 `cfg(test)` —— 这一条就是你要的"默认不进 zpkg"

```
[Test] / [Benchmark] / [Setup] / [Teardown]  ⇒  隐含 [Cfg("test")]（[Benchmark] 隐含 test|bench）
```

于是：

| 构建 | `[Test]` 声明 | TIDX section | 产物里有测试吗 |
|---|---|---|---|
| `z42c build`（默认/release） | **剪除** | **不写** | ❌ 没有 |
| `z42c build --tests`（cfg=test） | 保留 | 写 | ✅ 有 |

**这同时解决 P1 和 P2**：

- **P1** —— 隔离从"目录纪律"变成"编译期机制"。在 `src/` 里写 `[Test]` **不再污染产物**。
- **P2** —— 既然写在 `src/` 里安全了，**单元测试就可以与被测代码同包**
  ⇒ **看得见 `internal`** ⇒ 补上 Rust `mod tests` 那一层。

### 4.3 两级测试，对齐 Rust

| 级 | 位置 | 编译方式 | 可见性 | 对标 |
|---|---|---|---|---|
| **单元测试** | `src/` 内，`[Cfg("test")]` / `[Test]` | 与库**同包**，`--tests` 时一起编成 `<lib>.test.zpkg` | **见 `internal`** | `#[cfg(test)] mod tests` |
| **集成测试** | `tests/` 目录 | 独立包，父库为依赖（**现状不变**） | 只见 `public` | `tests/*.rs` |

现有的 `[[test]]` / `[tests.dependencies]` / 约定发现**全部保留**——它们描述的是集成测试那一级，
本设计只是在其下**补上单元测试这一级**。

### 4.4 编译粒度：一个包一个测试产物（P4）

单元测试与库同包 ⇒ `z42c build --tests` **一次**编出 `<lib>.test.zpkg`，取代"每个测试文件一个 zpkg
+ 每个都从头 bootstrap 一遍 z42.core"。集成测试保持每单元一包（与 Rust `tests/*.rs` 相同）。

### 4.5 发现机制（P3）

`_dirHasTestMethods` 的全文子串扫描退休。判据换成**编译产物**：`--tests` 编出的 zpkg
有没有测试条目——编译器已经知道答案，不需要 xtask 再猜一遍。

### 4.6 TIDX 退休（P5）—— cfg 是它的前置

TIDX 今天存在的一半理由是"要在产物里标出哪些函数是测试"。cfg 剪枝之后：

- release 产物里**根本没有测试** → 不需要标；
- test 产物里全都是给 runner 用的 → `[Test]` 退化成普通 **store-meta attribute**，
  z42b 用**反射**发现即可（`Type.GetMethods()` + `GetAttribute(typeof(TestAttribute))`），
  与终态记录完全一致。

⇒ 编译器里那 12 个文件的 test 知识**整体撤出**，zbc 少一个 section。

### 4.7 规则归属（P6）

TIDX 退休后 `[Test]` 是 z42.test 的普通 attribute，校验自然应随 z42.test 走。要让它**自动生效**，
需要 **analyzer 随库携带**（对标 Roslyn analyzer 随 NuGet 包分发）——z42.test 已是每个测试包的依赖，
一旦支持即 28/28 覆盖、零 manifest 改动。这是独立前置变更。

**在此之前**，P0 的校验留在编译器（[enforce-test-attr-placement](../enforce-test-attr-placement/design.md) §4.5）——
`[Test]` 现在还要驱动 cfg 剪枝，更是编译器的事。

> **校验必须在剪枝之前**：否则 release 构建里 `[Test]` 被剪掉 → 写错的测试只有跑测试时才暴露。
> 顺序：`CfgPrune` **收集**待剪声明 → 校验（含被剪的）→ 实际剪除。

---

### 4.8 测试产物的 kind：**由 harness 决定，与父项目 kind 无关**（User 裁决 2026-09-11）

```
harness = true   →  lib   —— z42b `ModuleLoader.Load` 加载 + 反射跑 [Test]
harness = false  →  exe   —— 直接跑 entry Main，退出码即判定
```

**这不是新决定，是把既有惯例补齐。** manifest 声明的 `[[test]]` 目标**今天就是这么做的**：

```z42
// xtask_test_targets.z42:192   harness=true  → _compileTarget(..., "lib", ...)
// xtask_test_targets.z42:227   harness=false → _compileTarget(..., "exe", ...)
```

**但老的 per-unit 路径没跟上**——约定发现的 `tests/` 单元一律写
`kind = "exe"`（[xtask_test_lib.z42:216](../../../../scripts/test/xtask_test_lib.z42#L216)），
即便它们是 harness=true 的反射式测试。

⇒ **现存漂移**：同样是"z42b 反射跑 `[Test]`"，manifest 声明的产 lib、约定发现的产 exe。
两条路径都能跑（z42b 两种都加载得了），所以一直没人发现。S3 顺手统一到上面那条规则。

### 4.9 exe 项目**允许**写单元测试

**结论：允许。** 理由按份量：

1. **cfg 之后允许是零成本的** —— `[Test]` 隐含 `cfg(test)`（§4.2），默认就被剪除，
   发布的 exe 里根本没有它。禁止反而要新增一条规则 + 一个诊断 + 将来的逃生口，**是加机制不是减机制**。
2. **Rust 就允许** —— `src/main.rs` 里可以有 `#[cfg(test)] mod tests`，`cargo test` 会把 bin
   编成 test 版并跑它的单元测试。这是主流做法，不是边角。
3. **禁止会产生坏的次生行为** —— 要么把人逼着"为了可测性而人为拆一个库出来"，要么更糟：干脆不测。
   应用里的逻辑不比库里少。
4. **z42 已经有 exe 形态的测试概念** —— `[[test]] harness=false entry="..."`（自驱 Main、退出码判定）
   本来就是把测试写成一个 exe。

**机制成本 = 一条规则**（就是 §4.8 那条）：exe 项目跑 `z42c build --tests` 时，
按 harness=true 编成 **lib**——`[project].entry` 不烘，`Main` 退化成一个没人调的普通函数，
z42b 照常加载 + 反射跑其中的 `[Test]`。

> **反过来的那一半仍然成立**：exe 的**发布**产物里不允许有测试——但这由 §4.2 的 cfg 剪枝**自动保证**，
> 不需要单独立一条禁令。「不允许」和「默认剪掉」在结果上相同，而后者不用写规则。

## 5. 分期

| 期 | 内容 | 依赖 | 收益 |
|---|---|---|---|
| **S0** | `[Test]` 位置/签名强制（60 行，[enforce-test-attr-placement](../enforce-test-attr-placement/)） | — | 修 P0，**独立可交付** |
| **S1** | `[Cfg("...")]` directive + `--cfg` + AST 剪枝 | — | 通用条件编译能力 |
| **S2** | `[Test]` 家族隐含 `cfg(test)` + `z42c build --tests` | S1 | **修 P1**：默认不进 zpkg |
| **S3** | 单元测试层（`src/` 内测试，同包编译，见 `internal`）+ 发现机制换判据 + **kind 按 harness 统一**（§4.8，消除 lib/exe 漂移）+ **exe 项目的 `--tests` 走 lib**（§4.9） | S2 | **修 P2 + P3 + P4** + 消漂移 |
| **S4** | analyzer 随库携带 | — | 破 opt-in 死结（P6 前置） |
| **S5** | TIDX 退休：`[Test]` → store-meta + 反射发现；校验移交 z42.test | S2,S4 | **修 P5 + P6**；zbc 少一段 |
| **S6** | 结构化输出（`--format json` / `--filter`） | — | 修 P7，独立 |

**S0 现在就能做**，不依赖任何一项，且是唯一在修"今天会崩"的东西。
S1→S2→S3 是主线。S4/S6 独立。S5 收尾。

---

## 6. 明确不做（v1）

| 不做 | 理由 |
|---|---|
| cfg 表达式（`all()`/`any()`/`not()`/`feature = "x"`）| v1 只需 `test`/`bench`/`debug` 三个内建名；表达式是纯增量 |
| 语句级 / 表达式级 cfg | 声明级是 Rust `#[cfg]` 的主粒度，也避免"半个函数被 ifdef"的病 |
| `#[cfg_attr]` 等价物 | 无需求 |
| 把集成测试也搬进包内 | Rust 保留 `tests/` 两级是对的：集成测试**就该**只见公开面 |
| 测试并行执行模型改动 | 与本设计正交（现有 batch + jobs 模型不动） |

## 7. 开放问题

1. ~~**`--tests` 产物是 exe 还是 lib**~~ —— **已定：lib**（§4.8，User 2026-09-11）。
   顺带发现现存漂移：manifest 声明的 `[[test]] harness=true` 产 lib、约定发现的 per-unit 产 exe，
   S3 统一。z42b 对 lib 的加载已由 `[[test]]` 路径在产验证。
2. **`[Cfg]` 与 partial / generator 产物的交互**：generator 产出的声明带 `[Cfg]` 时在哪一轮剪。
   建议：剪枝在每轮 `RunAst` 都跑，保持幂等。
3. **cfg 是否进增量指纹**：必须进——`--cfg test` 与否编出的产物不同，缓存键要区分
   （对齐 `_handlerFingerprint` 的做法）。
4. **`internal` 可见性的实现细节**：同包单元测试天然可见，但要确认 `--tests` 构建不会把
   `[Cfg("test")]` 的 internal 泄进导出表（`ExportedTypeExtractor`）。
