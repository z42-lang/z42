# Tasks: complete-generic-instantiation

> 状态：🟡 P1 待 User 过 6.5 gate | 创建：2026-09-23
> 分支/worktree：`complete-generic-instantiation-p1` @ `wt-geninst` | 基于：origin/main `6232c87d1`
> 类型：`lang` + `ir` | 落地：**一条线、三个 PR 顺序落**（User 裁决 2026-09-23）

**变更说明：** 把 #774 显式留下的三条未覆盖项做完——跨包实例化（覆盖元组）、
泛型 class 独立身份、容器密集化。

## 进度概览

- [ ] **P1** 跨包闸门放宽到「成员全可合成」的导入泛型（🔴 正确性）
- [ ] **P2** 泛型 class 独立身份（🔴 正确性，含类型混淆）
- [ ] **P3** 容器密集化 + 删 P3a 装箱（⚪ 性能，不达标则不合）

---

## P1

### 0. 先钉风险（**在动任何发射代码之前**）

- [x] 0.1 🔴 **已定性（2026-09-23）：会炸，D2 因此修正。** 结论与取证见 design.md §D4。
      要点：① `struct_alloc` 不查歧义表，元组分配这条不受影响；② 类型描述符重复 →
      `registry.rs:83` warn + `note_ambiguous_type` + first-wins 丢弃第二份；
      🔴 ③ **合成 ctor 同名重复 → `note_ambiguous_function` → `exec_call.rs:235-239` 调用即抛**。
      碰撞形态比原先设想的广：**「一个库内部用了 `(int,string)`，主程序也用了」就已撞上**。
      编译期 E0601 不会误报（`PkgCheckFqn` 对实例化返回**定义**的 FQN）。

### 0b. D4-fix：加载器区分「合成实例化产物」与「用户声明」（P1 的先决条件）

- [x] 0b.1 `src/runtime/src/metadata/lazy_loader/registry.rs`：类型循环与函数循环各加一条——
      名字是实例化产物（含 `<`）且与表内那份**结构一致** ⇒ 静默跳过（不 warn、不记歧义）；
      结构不一致 ⇒ 保持今天的歧义行为（**不得静默吞**）
- [x] 0b.2 结构一致的判据：size + 字段数 + 逐项偏移（类型侧）
- [x] 0b.3 `src/runtime/src/metadata/lazy_loader_tests.rs`：单测覆盖三种情形
      （实例化名重复且一致 / 实例化名重复但不一致 / 普通用户类型重复）
- [x] 0b.4 **阴性对照**：`src/tests/cross-zpkg/dup_fqn_crosspkg` 仍报 E0601 ✅（e2e 71/71 全绿）

### 1. 元数据位（生产方 → 消费方）

- [ ] 1.1 `src/runtime/src/metadata/bytecode/class.rs`：加 `METHOD_FLAG_SYNTHESIZED: u8 = 1 << 4`
      + 注释说明 bit4–7 原本空闲、旧读端按 u8 读忽略不认位
- [ ] 1.2 z42c 侧：所有**编译器合成**的成员在写 SIGS 时打上该位（record 合成成员 / `Equals$1` /
      `[Record] ToString` / 主构造器）。**收敛到单一出口**，不要逐处打
- [ ] 1.3 `src/libraries/z42.ir/src/ExportedTypes.z42`：`ExportedMethodZ` 加 `IsSynthesized`；
      `ExportedClassZ` 加 `IsRecord`。**两者都不进 ctor 签名**（种子 ABI：默认值 + 构造后赋值）
- [ ] 1.4 `src/libraries/z42.ir/src/TsigReconcile.z42`：`ecz.IsRecord = (cd.Flags & 8) != 0`；
      `em.IsSynthesized = (f.MethodFlags & 16) != 0`
- [ ] 1.5 `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42`：`nct.IsRecord = cl.IsRecord`
      （`Z42ClassType.IsRecord` 已存在，今天只对本地类回填）；成员的 synthesized 标记随符号入表
- [ ] 1.6 验：`strings` 看一个 stdlib zpkg，确认 `ValueTuple2` 的成员确实全部带标记

### 2. 闸门与合成 decl

- [ ] 2.1 `src/compiler/z42c.semantics/src/ImportedGenericSynth.z42`（NEW）：
      判据 `_canSpecializeDef(def)` = `struct ∧ IsRecord ∧ 全部导出成员 IsSynthesized`
- [ ] 2.2 同文件：`SynthDecl(def)` —— 从有序字段表 + 型参名反造等价 `ClassDecl`
      （`[Record] struct X<T1..Tn>(F1 f1, …)`）
- [ ] 2.3 `src/compiler/z42c.semantics/src/ExprEmitter.z42`：`:554`（布局）与 `:604`（身份）
      两处闸门**都改调同一个判据函数**。⭐ #774 教训 6：同一判据散在多处 ⇒ 只改一处必漏
- [ ] 2.4 `src/compiler/z42c.semantics/src/IrGenTypeEmitter.z42`：`GenericDecls` 接纳合成 decl；
      `:68` 的 `rd is Decl` 兜底判据跟着放宽
- [ ] 2.5 ⭐ **放宽判据前先确认它在护什么下游**（#774 栽过一次）：逐一核对
      `LocalClasses` 这条闸门今天还顺带挡着哪些东西，不要只改判据不动下游

### 3. 用例

- [ ] 3.1 `src/tests/types/crosspkg_generic_inst_value_semantics.z42`（NEW）：
      形态 ①③ + 传参 + 嵌套 + 带引用叶子的实参
- [ ] 3.2 **阴性对照 a**：基元元组 `(int,string)` 复制/传参行为不回归
- [ ] 3.3 **阴性对照 b**：带用户方法体的跨包泛型 struct **不命中** ⇒ 产物逐字节不变
- [ ] 3.4 0.1 的双消费方包用例落成正式用例

### 4. GREEN

- [ ] 4.1 `xtask build stdlib` + `build compiler` + `test compiler`（自举字节不动点 gen1==gen2）
- [ ] 4.2 `xtask test all`
- [ ] 4.3 ⚠️ **`xtask test e2e --mode jit`**（`xtask test` 的 golden 只跑 interp；
      改 struct 访问路径必须显式跑 JIT。注意 `./xtask test e2e jit` 会因位置参数错静默 `rc=2`）
- [ ] 4.4 `cargo test --lib`（**debug，不加 `--release`**）
- [ ] 4.5 `xtask test bootstrap`（上一 nightly 仍能编当前源）
- [ ] 4.6 并入 origin/main 最新改动 + **在新基线上重跑完整 GREEN**
      （⭐ 只在旧基线绿过 = 测的不是要合的东西）

### 5. 文档 + PR

- [ ] 5.1 `docs/internals/src/runtime/struct-value-semantics.md` §收敛面与延后：遗留项 ② 状态更新
- [ ] 5.2 `docs/internals/src/compiler/source-compile.md`：跨包实例化特化机制
- [ ] 5.3 `docs/roadmap.md`：泛型擦除槽那行的「仍未覆盖」三条状态更新
- [ ] 5.4 PR（body 写跑 GREEN 时的 `base: <sha>`）

---

## P2（开工前回到阶段 3/4/5 补精确 Scope 与场景）

- [ ] 完整实例化类描述符（基类链 / 接口 / 静态字段；**vtable 不需要**）
- [ ] `is` / `as` 保留 `NamedType.Args`，走与身份名同一个规范名函数
- [ ] 静态字段按实例化分槽 —— ⚠️ **自举敏感，走分阶段引入**（User 裁决）
- [ ] 解析器：`(GBox<string>)o`
- [ ] 解析器：`GBox<int>.Count`

## P3（开工前定达标线）

- [ ] 类级类型实参送到 `List<T>` 内部 `new T[n]` 的分配点
- [ ] 删 P3a 装箱
- [ ] benchmark：分配次数 / RSS / 墙钟；**不达标则不合**

---

## 实测取证（2026-09-23，树 `wt-geninst` @ `6232c87d1`）

供种：CI run 35805871721 的 `toolchain-macos-26` + `cargo build --release` 重建 runtime
+ `z42 publish scripts/xtask.z42.toml` 重建门禁。interp 与 `--mode jit` 逐字相同。

| 探针 | 实测 | C# |
|---|---|---|
| `(P2,int) t=(a,7); a.Y=99` → `t.Item1.Y` | 99 | 2 |
| `(P2,int) t2=t; t2.Item1.Y=55` → `t.Item1.Y` | 55 | 2 |
| `(int,string)` 复制/传参 | 正确 | 正确 |
| `List<P2>` 值语义 | 正确（装箱顺带给的） | 正确 |
| `GBox<int>`/`GBox<string>` 静态计数 | 4 / 4 | 2 / 2 |
| `is GBox<string>`（收者 `GBox<int>`） | true | False |
| `as GBox<string>`（同上） | 放行 → `VCall: expected object, got I64(42)` | null |

**阴性对照（同一次编译、同一文件）**：

```
本包  [Record] struct Loc<A,B>(A Item1, B Item2)  → struct_alloc Demo.Loc<P2,int> [24B] + struct_fget_prim @8
跨包  [Record] struct ValueTuple2<T1,T2>(…)       → obj_new Demo.<unknown> + field_get %14.Item1
```

## 踩过的坑（本轮）

- 🔴 **`Z42_JIT=1` 不是旋钮**，正确的是 `--mode jit` / `Z42_MODE=jit`。我先用错的跑了一轮，
  「JIT 与 interp 一致」当时是个空结论。
- `xtask` 的 apphost stub 会用 `.z42/bin/z42vm`；供种后那份是旧的 ⇒
  `zpkg minor 49 not supported (writer is at 0.43)`。修法：`xtask build sdk` 后
  用 `artifacts/.z42` 整个替换根 `.z42`。
- 所有 xtask / cargo 命令前须 `RUSTUP_TOOLCHAIN=1.98.1`（本机默认 1.88 < MSRV 1.95）。
- 单文件 `--emit-zbc` 出的 zbc 没有烘焙入口，跑它要给 **`Demo.Main`**（带命名空间）。
