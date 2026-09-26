# tasks: fix-nested-generic-field-layout

> 类型：**fix**（最小化模式）｜ 创建：2026-09-27
> 出身：[结构审计 2026-09](../../../internals/src/compiler/generics.md) 的止血项 U-4。

## Why

`StructLayout` 的字段类型代换**只做整名匹配**，于是「泛型 struct 的字段类型本身是一个实例化」
这一格代换不到：

```z42
struct P2   { int X; long Y; }
struct Loc<A, B> { A a; B b; }
struct Wrap<T>   { Loc<T, long> inner; }   // ← 字段类型是实例化，不是裸型参
```

`Wrap<P2>` 时 `subst = {T→P2}`，而字段类型串是 `"Loc<T,long>"` —— 整名不在表里 ⇒ 不代换
⇒ 布局层按**擦除**的 `Loc`（`a` 当 8B 句柄 + `b` 8B）算出 `Wrap<P2>` = **16B**；
而访问侧的静态类型是语义层**结构递归**出来的 `Loc<P2,long>`（`a` 内联 16B + `b` @16）= **24B**。

两侧对同一批字节的理解不一致。实测（release 与 jit 均如此）：

```
Error: uncaught exception: struct field write out of blob bounds (off=16, w=8, len=16)
```

分配端给 16 字节、访问端按 off=16 写 —— 越界。**这是审计里 U-4 那条，但症状是崩溃而不是
静默错值**（原推断说「静默错值且闸门保证永不暴露」，两点都不对：它立刻崩，只是无人测过）。

这正是审计 R2 记下的那条裂缝的具体形态：**同一个「型参代换」概念，语义层做结构递归、
布局层只做整名匹配** —— 低层那份语义更弱。

## What Changes

新增 `StructLayout._substFieldTypeName(ftype, subst)`：整名匹配**外加递归进实例化实参**，
重组时用 `InstName`（那是「编译器 / wire / 运行期三方必须用的同一份拼法」——自己拼一份会让
描述符名对不上，而运行期 `resolve_layout` 查不到是**静默落兜底布局**）。

`_compute` 与 `_computeObjFields` 两处都改调它（两处必须同口径，否则类里的内联 struct 字段
与裸 struct 的同名布局会分叉）。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/StructLayout.z42`
- `src/tests/types/generic_struct_inst_field.z42`（新 e2e）
- `docs/internals/src/compiler/generics.md`

## Tasks

- [x] `_substFieldTypeName`：递归代换 + 用 `InstName` 重组
- [x] `_compute` / `_computeObjFields` 两处改调它
- [x] e2e `generic_struct_inst_field.z42`：读 / 写 / 相邻叶子不串 / 两层嵌套 / 值语义拷贝
      + **阳性对照**（引用型实参那格布局与擦除相同，不得被本修复打坏）
- [x] 实测 interp + **jit 双验**（struct blob 读写路径，记忆里的铁律）：2 passed / 0 failed
- [x] 阴性对照（种子旧 driver 编同一 fixture）：崩在 `struct field write out of blob bounds`
- [ ] `docs/internals/` 记下「布局层的代换必须与语义层同口径」
- [ ] GREEN：`xtask test compiler`（含自举不动点）+ `xtask test e2e` 本地过；CI 全矩阵绿

## 与 `generic_struct_chain` 的区别（两格，缺一格照样崩）

| | 字段声明 | 存储 |
|---|---|---|
| `generic_struct_chain`（既有）| 裸型参 `A First` | 擦除成**引用叶子**，存另一块 blob 的句柄 |
| 本 change | 实例化 `Loc<T,long> inner` | 它是 struct ⇒ 应当**内联** ⇒ 两端必须对大小达成一致 |

## 不做（Out of Scope）

- **不把 `StructLayout` 的入口改成吃 `Z42Type[]` 实参**（审计建议的根治）。那要给布局层引入
  对 `Z42Type` 的依赖（它今天刻意只吃 `StructFieldsDef` + 字符串），是中刀、需单独评估。
  本 change 在字符串层把语义补齐，先消掉崩溃。
- **不动跨包那一格**：跨包实例化两侧都不特化、自洽（见 generics.md 的混合模型一节）。

## 验证

- 无格式 bump：不改 wire 布局。
- **指纹**：`InstDiffersFromDef` 的答案对「字段类型是实例化」的泛型 struct 会改变 ⇒ 这类
  实例化从「不特化」变成「特化」⇒ **同一份源码的编译结果会变**。按 version-bumping 的判据
  需 `CompilerFingerprint++`；号按合并时的 main 现查（审计 D-1 那条规范冲突未裁决前，
  取「同一份源码的编译结果是否会变」这一口径）。
- 自举字节不动点：stdlib / z42c 里没有「字段类型是实例化的泛型 struct」⇒ 预期产物不变，
  由 `test compiler` 的 gen1==gen2 与 stdlib 逐包比对确认。
