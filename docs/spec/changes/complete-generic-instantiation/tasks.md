# Tasks: complete-generic-instantiation（泛型实例化的单调化）

> 状态：🟡 S1 待 User 过 6.5 gate | 创建：2026-09-23 | **2026-09-24 按 A 方案重写**
> 分支/worktree：`complete-generic-instantiation-p1` @ `wt-geninst` | 基于：origin/main `2d4042a08`
> 类型：`lang` + `ir`

**变更说明：** #774 做的是**部分单调化**（特化了实例化类型，没特化操作它的泛型代码），
产生静默错值，**今天就在 main 上**。本 change 把特化做成**闭包**。

## 进度概览

- [x] **前置** D4-fix：合成实例化产物的重复到达不记为歧义（S2 先决条件）
- [x] **顺带修复** 导入 record 丢失 record 身份（跨包 `with` / 位置解构误拒）
- [x] **S1-a** 伪实例化不走特化路径（`new G<T,X>()` 不再硬崩）—— `71cbcf51f`
- [x] **S1-b** 泛型体单调化：自由函数 + **静态**泛型方法
- [x] **S1-c** **非虚**实例泛型方法（vcall 的解析链有「按名走基类链查 func_index」的回落 ⇒
      改派简单名即可命中，**无须动 vtable**；命中后照常装 IC，热路径不受影响）
- [x] **S1-d** **虚/override** 实例泛型方法（运行期「特化名不沿基类链继承」+ 覆写闭包）
- [x] **S1-e** 泛型体登记表提到**包级** —— 泛型声明在别的文件时也能改派
- [ ] 🔴 **S1-f** 跨 CU 的**泛型类型**实例化（#774 既有缺陷，单独立项，见下）
- [ ] **S2** 跨包模板投送 —— 覆盖元组与 `KeyValuePair`，需格式 bump
- [ ] **S3** 退役擦除名回落

---

## 已合入本分支的四个 commit

| commit | 内容 | 在 A 方案下是否仍成立 |
|---|---|---|
| `998b2e6c9` | 归档两个已完成的 change | ✅ 与方案无关 |
| `674d04057` | D4-fix（加载器区分合成产物与用户声明） | ✅ **S2 先决条件，保留** |
| `2d6b6d226` | 导入 record 的 `IsRecord` 接线（修跨包 `with`/位置解构误拒）| ✅ **独立成立的真 bug 修复，保留** |
| `f89ad960b` | `METHOD_FLAG_SHAPE_DERIVED` + 指纹 bump 11 | ⚠️ **待定**：它是「放宽闸门」那版判据的零件，A 方案下暂无消费方。S1 完成后若仍无用，连同指纹一并撤回 |

> 2026-09-24 已撤回：「放宽跨包闸门 + 反造 `ClassDecl`」那一版实现（未提交）。
> 撤回理由见 design.md §附录：其核心假设被实测推翻。撞到的六条障碍已全部写进
> design.md §S2，不必重新发现。

---

## S1：本包闭包

### 0. 驱动用例先行

- [x] 0.1 `src/tests/generics/generic_body_specialization.z42`（NEW）：main 上的静默错值
      ```z42
      [Record] struct Loc<A, B>(A Item1, B Item2);
      int ReadSecond<T>(Loc<T, int> p) { return p.Item2; }
      // Loc<P2,int>(a, 7) ⇒ ReadSecond<P2>(t) 今天读出 2，应为 7
      ```
      先确认它在**当前树**上判红（否则用例没有判别力），再动实现。
- [x] 0.2 同文件补：泛型**实例方法**形态、**闭包传递**形态（嵌套实例化）、
      **写**回形态（不止读）

### 1. 判据与工作表

- [x] 1.1 判据（design D1）：泛型体在该替换下**触碰的任一实例化**满足 `InstDiffersFromDef`
      ⇒ 需特化。复用既有 `InstDiffersFromDef`，只改作用对象
- [x] 1.2 工作表（design D2）：`IrGen.Generate` 尾部的不动点循环，工作项从
      「实例化类型名」扩成 {实例化类型, 泛型体实例 `f<A>`} 两类
- [x] 1.3 ⚠️ **防挂死**（实测逼出：S1 实施后递归泛型让编译器 100% CPU 不返回，45 秒未结束；
      main 在同一源码上秒级报 E0402）。两道防线：① 本 CU 有类型错误 ⇒ 完全不做特化
      （`IrGen.HasTypeErrors`，三条 Generate 入口都要设 —— `_compileCu` 与 `IrDump` 的两条
      单文件路径，我первый次只设了包路径，`--emit-zbc` 照样挂）；② 工作表上限
      `SpecializationCap`（每个特化要发一整个函数体，故取 2000 而非两万，秒级失败）。
      门：`shape_derived_flag_tests.test_recursive_generic_does_not_hang`

### 2. 特化名的单一出口

- [x] 2.1 泛型体特化名建**单一出口**，调用点与发射端**都调它**
      ⭐ #774 教训 6：同一判据散在多处 ⇒ 只改一处必漏（当时漏了属性那处，
      症状是 `MissingSymbolException`）
- [x] 2.2 核对：调用点拼名与发射端逐字节一致（含 arity-mangle / 嵌套实例化）

### 3. 阴性对照

- [x] 3.1 布局相同的实参组合**不特化** ⇒ 产物逐字节不变
- [x] 3.2 不含泛型实例化的程序 ⇒ 产物逐字节不变
- [x] 3.3 代码膨胀观测：记录 stdlib + z42c 自举产物的体积变化

### 4. GREEN

- [ ] 4.1 `xtask build stdlib` + `build compiler` + `test compiler`（自举不动点 gen1==gen2）
- [ ] 4.2 `xtask test all`
- [ ] 4.3 ⚠️ **`xtask test e2e --mode jit`**
- [ ] 4.4 `cargo test --lib`（**debug，不加 `--release`**）
- [ ] 4.5 `xtask test bootstrap`
- [ ] 4.6 并入 origin/main 最新改动 + 在新基线上重跑完整 GREEN

### 5. 文档 + PR

- [ ] 5.1 `docs/internals/src/runtime/struct-value-semantics.md`：单调化闭包这条不变式
- [ ] 5.2 `docs/internals/src/compiler/source-compile.md`：工作表与判据
- [ ] 5.3 `docs/roadmap.md`
- [ ] 5.4 `f89ad960b` 的去留裁决（见上表）
- [ ] 5.5 PR（body 写跑 GREEN 时的 `base: <sha>`）

---

## S1-d：虚/override 实例泛型方法（已做）

虚派发从**接收者的运行期类型**起走基类链，故每个覆写都要各特化一份。

**实测三方对照**（`Base b = new Derived(); b.Bump<P2>(Loc<P2,int>(a,7))`）：

| | 结果 |
|---|---|
| main（#774 现状）| **102** —— 静默错值（读到 @8 的 `P2.Y`=2，+100） |
| 本分支 S1-c 后 | `struct ref leaf at byte offset 8 not in type layout` —— 大声报错 |
| 本分支 S1-d 后 | **107** ✅ |

**安全性靠两条，缺一不可：**

1. **运行期：特化名不沿基类链继承**（`vcall_resolve`）。一份特化按**某一个声明**所在类型的
   实例化布局烘焙了偏移，落到基类的特化体就是静默调错实现。判据是「名字含 `:`」——
   `:` 在标识符与类型名里都不可能出现，故精确。miss 时交给擦除名回落（大声报错）。
2. **编译期：本 CU 内把闭包扩到该方法的全部覆写**（`_noteSpecOverrides`）。过近似安全
   （多特化几份只是体积），漏特化才是危险。

> ⚠️ **更正（2026-09-25 实测）**：S1-e 把**登记表**提到了包级，但跨文件的覆写特化**仍未打通**
> ——真正的阻塞是 `SemanticModel` 也按 CU 建（见下方 S1-f）。原先写的「覆写闭包也看得见别的
> 文件里的声明」是**未经验证的断言**，据实更正。

## S1-e：登记表提到包级（实测逼出来的）

`IrGen` 由 `CuCompile._compileCu` **按编译单元**创建。登记表只扫本 CU 时，**泛型声明在别的
文件**的调用点判不出该改派 ⇒ 落到擦除体 ⇒ 运行期 `struct ref leaf at byte offset 8 not in
type layout`。这不是边角，是主路径：任何跨文件的泛型调用都中招。

实测（`a.z42` 声明 + 用，`b.z42` 用）：修前 `UseInB` 报错，修后 `A=7 B=9`。

修法 = 在 `IrDump.BuildPackageCus` 扫全包 CU 建表（与 `layouts` 同为并行段只读共享），
`Generate` 仅在未注入时按本 CU 自扫（单文件 dump / 测试路径）。两条路**共用**同一个
`IrGen.ScanGenericBodies`，判据只有一份。

**顺带验证**：两个 CU 都用到 `ReadSecond<P2>` ⇒ 各发一份同名特化体。实测把该包当依赖加载
**不会**被记成歧义函数（调用未抛、无告警）—— 调用在各自模块的 `func_index` 里就地命中。
固化为 `cross-zpkg/crosscu_generic_specialization`。

## S1-f：跨 CU 的泛型类型实例化（#774 既有缺陷，单独立项）

**症状**：泛型 struct 声明在一个文件、实例化在**另一个文件** ⇒ 运行期
`MissingSymbolException: undefined function Demo.Loc<P2,int>.Loc`。
**main 上逐字复现**，与本线无关。

```z42
a.z42: [Record] struct Loc<A, B>(A Item1, B Item2);
c.z42: new Loc<P2, int>(a, 7)      → undefined function Demo.Loc<P2,int>.Loc
```

**根因是两层，只修第一层不够**（实测探针：`ZP declFound pkgHas=true cuHas=false bodyLoc.Loc=false`）：

| 层 | 说明 |
|---|---|
| 泛型**声明**表按 CU 建 | 闸门 `LocalClasses` 是**包级**的（跨文件放行），`GenericDecls` 却按 CU ⇒ 判据与数据源不同步。可修（已试通） |
| 🔴 **绑定后的体**（`SemanticModel`）也按 CU 建 | `TypeChecker.Infer(cu, …)` 每 CU 一份；`EmitMethod` 要 `model.GetBody(...)`。**单独修声明表无效** |

> 与 design.md §S2「障碍 1」是**同一件事**，只是粒度从跨包降到跨文件。

**三条路（User 已裁决：都不在 S1 的 PR 里做，单独立项走甲）**：

| | 做法 | 代价 / 性质 |
|---|---|---|
| ✅ **甲** | **两阶段包级流水线**：先 Infer 全部 CU，再带所有 model 做 codegen | 要动 `BuildPackageCus` / `CompileCuTask` / `_compileCu` —— 并行 + 诊断 + 缓存都在这条路上。真修 |
| ❌ 乙 | 闸门收窄到同一 CU | 崩 → 退回 #774 的**别名**（静默错值）。**loud 换 silent，与 z42 取向相反** |
| ❌ 丙 | 跨 CU 实例化时编译期报错 | 诚实且响，但会让今天「只在运行期崩」的构建编不过 |

⚠️ **A2（跨文件虚覆写）被这条挡住，至今未验成** —— 修完 S1-f 才测得到。

## S2 / S3（开工前回到阶段 3/4/5 补精确 Scope）

- [ ] S2 模板段载荷形态 + 格式 bump + 两代自举安排
- [ ] S2 六条已实测障碍：见 design.md §S2（**先读它再动手**）
- [ ] S3 退役 `vcall_resolve` 的擦除名回落

---

## 实测取证（树 `wt-geninst`，interp 与 `--mode jit` 逐字相同）

供种：CI run 35805871721 的 `toolchain-macos-26` + `cargo build --release` 重建 runtime
+ `z42 publish scripts/xtask.z42.toml` 重建门禁。

### 单调化闭包（本 change 的靶子）

| 探针 | 实测 | 应为 | 备注 |
|---|---|---|---|
| `ReadSecond<P2>(Loc<P2,int>)` | **2** | 7 | **本包**；用 main 的编译器复验同样是 2 |
| `ReadSecond<P2>((P2,int))` | **2** | 7 | 元组同形 ⇒ 白名单方案被推翻 |
| `dict_iter` 的 `sum3`（特化开启时）| **0** | 6 | 生产方按擦除布局建 `KeyValuePair<K,V>[]` |

### 原 P2 的取证（泛型 class 无独立身份；**本 change Out of Scope，保留备查**）

| 探针 | 实测 | C# |
|---|---|---|
| `GBox<int>`/`GBox<string>` 静态计数 | 4 / 4 | 2 / 2 |
| `is GBox<string>`（收者 `GBox<int>`）| true | False |
| `as GBox<string>`（同上）| 放行 → `VCall: expected object, got I64(42)` | null |
| `(GBox<string>)o` / `GBox<int>.Count` | 解析失败 `E0202` | 合法 |

根因（agent 核实）：`_instClassDesc` 合不出完整描述符（基类写死 `Std.Object`、无接口、
无静态字段）；`_bindIsExpr`/`_bindAsExpr` 把 `NamedType.Args` 直接扔掉；静态字段键是
`QualifyClass(裸名) + "." + 字段`。⭐ **vtable 不在 TYPE 段**（运行期从 `own_methods` +
基链 merge），比记忆里记的少一块。

### 原 P3 的取证（容器密集化；**Out of Scope，保留备查**）

`List<P2>` 的值语义**是对的**（P3a 每元素一个堆 box，装箱顺带给了拷贝语义）⇒
那条不是正确性缺口，只是密度/分配。

---

## 踩过的坑（本轮）

- 🔴 **`Z42_JIT=1` 不是旋钮**，正确的是 `--mode jit` / `Z42_MODE=jit`。我先用错的跑了一轮，
  「JIT 与 interp 一致」当时是个空结论。
- 🔴 **跨包的事一律走 harness 判定**：手工 `z42c build` 与单文件 `--dump-ir` **不加载依赖
  TSIG**（元组显示成 `Demo.<unknown>`）。我为此误判过两次——一次以为 fixture 坏了，
  一次以为闸门没生效。`--emit-zbc` 走的是另一条（带依赖）路径。
- 🔴 **阴性对照要能分辨两种失败**：跨包 record 探针里加一个**普通 class** 才分得清
  「fixture 接线坏了」与「record 被误拒」——两者症状一模一样。
- `xtask` 的 apphost stub 用 `.z42/bin/z42vm`；供种后那份是旧的 ⇒
  `zpkg minor 49 not supported (writer is at 0.43)`。修法：`xtask build sdk` 后
  用 `artifacts/.z42` 整个替换根 `.z42`。**且改完编译器后 `artifacts/.z42/bin/z42c` 不会自动
  跟新**——我拿它验过一轮，测的是旧编译器。
- 所有 xtask / cargo 命令前须 `RUSTUP_TOOLCHAIN=1.98.1`（本机默认 1.88 < MSRV 1.95）。
- 单文件 `--emit-zbc` 出的 zbc 没有烘焙入口，跑它要给**带命名空间**的 `Demo.Main`。
- `strings` 找不到函数全名**不能证明没发射**：全名不是单条池字符串（本包特化的
  `Demo.Loc<P2,int>.Loc` 同样查不到，而它能跑）。
