# Proposal: 泛型擦除槽的值复制 —— 按实例化算布局（部分具体化）

> Status: **DRAFT · 范围评估已完成，等 User 确认后进 spec**
> 分类：ir（TYPE 段内容变化）+ vm（值语义）→ **规范先行**；子系统：compiler 为主，runtime/wire 近乎零改动
> 方向：2026-09-22 User 裁决「对齐 C#」；同日 **User 否决装箱方案**，改走**按实例化算布局**
> 承接 [fix-generic-struct-chain-access](../../archive/2026-09-15-fix-generic-struct-chain-access/)（修链式偏移时实测发现本缺陷并登记 Deferred）

## Why

struct 值存进**声明类型为型参 `T` 的字段**时按句柄存、不复制；复制外层泛型 struct 也只浅拷句柄。
2026-09-22 在 main `134c7d962` 实测，三种形态全部违反值语义：

| # | 形态 | z42 | C# |
|---|---|---|---|
| ① | `P2 inner; var pp = new Pair<P2,int>(inner,1); inner.Y = 99;` → `pp.First.Y` | **99** | 2 |
| ② | `P2 c; var cb = new CBox<P2>(c); c.Y = 77;` → `cb.Item.Y` | **77** | 2 |
| ③ | `var q = p3; q.First.Y = 9;` → `p3.First.Y` | **9** | 2 |

**形态 ② 还是 use-after-free**：`new CBox<P2>(c)`（泛型 **class**）把**栈帧作用域的 arena `StructRef`**
存进 GC 堆对象字段槽（`AccessEmitter.z42:140` → `FieldSetInstr`，不经复制/装箱）。ctx 帧一弹，
arena 按 LIFO 截断（`struct_arena.rs:71-75`），`cb.Item` 成悬垂句柄。看到的 `77` 是最温和的表现。
⇒ **本 change 是健全性修复，不只是语义对齐。**

## 根因：那个槽物理上放不下字节

`StructLayout._kindOf("A")`（`StructLayout.z42:498-505`）对型参名判断——不是 struct、不是 prim、
不是 string ⇒ 落 `StructLeafKind.GcRef`，**8 字节句柄**。布局是**按定义**算的：`Pair` 只有一份布局，
它不知道 `A` 将来是 `P2`。

**所以「存入时复制」是给一个错误的表示打补丁。** 正解是让那个槽真的装得下字节。

## 已否决：装箱（`__box_struct`）

2026-09-22 User 质疑「复制 struct 为什么要装箱」，成立，**撤回**。理由：

- 装箱是**引用操作**——每次存入一次堆分配，并引入**引用身份**，而值语义的要求恰恰是「没有身份」；
- C# **零装箱**，因为 C# 泛型具体化，`P2` 的字节就内联在 `Pair<P2,int>` 里；
- 拿引用机制模拟值语义，形状错，且把「取出」也一并拖成拆箱问题（`struct_copy_val` 硬拒非 `StructRef`）。

> 一处修正：装箱方案**不会**让每次元组构造都装箱（判据是「传入值是不是 struct」，`(1,2)` 存 int
> 不装箱）。代价没有一开始说的那么夸张，但形状仍然是错的。

## 方案：按实例化算布局（部分具体化，仅 struct 型参实参）

`Pair<P2,int>` 拿到**自己的**布局，`First` 是 **8 字节内联**（P2 的两个 int）而非 8 字节句柄。
一旦如此，**①②③ 由构造消失**，不需要任何「存入复制」特殊逻辑——它会走普通 struct 字段那条路：
`FieldIsStruct` 为真 → `_copyRegion` 自动深拷 → 链式偏移自动累加 → 外层 `StructCopy` 自动带上字节。

**只在型参实参是 blob struct 时生效**；`Pair<string,int>` 等一律保持今天的句柄表示、**字节不变**。
这是把爆炸面关住的唯一闸门。

## 范围评估（2026-09-22 实测，本节是 go 的依据）

### ✅ runtime：近乎零改动

`resolve_layout(ctx, type_name, size)`（`exec_struct.rs:51-62`）**按原样字符串名** `try_lookup_type`，
拿到 `td.struct_layout()` 就用，查不到则回落「只有 size 的纯基元布局」。**这条路径没有泛型名擦除。**
⇒ wire 送来一个名叫 `Pair<P2,int>` 且带 `struct_layout` 的 TypeDesc，运行期**一行都不用改**就会用上它。

全仓只有两处泛型名擦除：`exec_array.rs:33`（数组元素类型按裸名查 TypeDesc）与
`constraints.rs:82`。前者在有了按实例化描述符之后可以改为「先试全名、miss 再剥」，**向后兼容**。

### ✅ wire：零格式 bump

TYPE 段 = `count:u32` + `count` × 类描述（`ZbcWriter.BuildType`，`ZbcWriter.z42:265-268`），
reader 按 count 循环。**多送几条合成描述符只是列表变长，不是格式变化。**
内联布局块本身也已有既成 shape（`CLASS_FLAG_HAS_INLINE_STRUCT` bit7 gated，随 TYPE 尾部 emit）。
⇒ 不进 bootstrap-seed 的两代自举流程。

### ✅ 实例化类型在发射点本来就在手边

`FunctionEmitter._blobStructNameT`（`:494-501`）：
```z42
else if (t is Z42InstantiatedType) { ct = (t as Z42InstantiatedType).Def; }   // ← 主动丢实参
return ct.Name();                                                            // ← 裸名查布局
```
**是 `.Def` 这一步把实参丢了**，不是拿不到。加上布局缓存本来就是按名字字符串的 `StrMap`
（`_cache` / `_inlineCache` / `_objectCache` / `_objStructCache`，`StructLayout.z42:97-112`）
⇒ 编译器侧**不存在架构障碍**。

### ⚠️ 工作量全在编译器侧（M–L）

1. `StructLayout` 能为实例化名算布局：`_kindOf` 要带上代换表（`A` → `P2` → Struct kind + P2 的 size）。
2. 发射端停止塌到 `.Def`：`_blobStructNameT` / `_blobStructName`（`ExprEmitter.z42:506`）
   / `_receiverClassType` 在存在按实例化布局时给出实例化名。
3. **枚举用到了哪些实例化**（单态化集合问题）——本仓今天没有这个 pass。要覆盖：本包用例 +
   导入签名里出现的 + 嵌套（`Pair<Pair<P2,int>,int>`）。
4. 实例化名的**规范拼法**须编译器 / wire / 运行期三方一致。数组路径已有既成约定可参照：
   `_qualifyElemName`（`ExprEmitter.z42:513-519`）= 限定基名 + 原样实参串。
5. 跨包：消费方用自己的 struct 实例化生产方的泛型 struct 时，两侧必须算出**逐字节一致**的布局。
   先例是 `add-crosspkg-struct-value-semantics`（消费方重算，与生产方逐字节一致）。

## 🔴 三个最大风险

1. **自举字节漂移 + TYPE 段变长**：`Std.ValueTuple2..8` 的字段全是型参，凡是用 struct 实参实例化过的
   元组都会多一条描述符。`xtask test bootstrap` 要 gen1 == gen2，必须预留重新供种的一轮。
   **闸门（只在实参是 blob struct 时生效）就是控制这条的关键**——`(int,string)` 一律不动。
2. **与 P3a 的相互作用**：擦除槽变内联后，泛型**容器**边界的 P3a 装箱可能变成冗余甚至打架。
   `src/tests/types/struct_generic_container.z42` 是要盯的 golden（`:56-59` 已在验箱→拆箱）。
   需要判定：容器 API 实参装箱这条路留不留。
3. **名字里带 `<` 会不会打到按名字做事的东西**：ns 认领（NSPC）、`QualifyClass`、惰性加载器都按名字
   索引，而 `_qualifyElemName` 之所以要在 `<` 处切开，正是因为 imported ns 解析走裸键。
   合成描述符叫 `Ns.Pair<Ns2.P2,int>` 会不会被这些逻辑误判，**需要在 spec 阶段先做探针**，
   不能假定没事。

## 不做

- 不引入 `StructLeafKind.Erased`（改 zbc TYPE 段 `ref_kinds` 字节 ⇒ 真格式 bump，且会撞
  `cross-zpkg/struct_nested_layout_cross_pkg` 的逐字节一致要求）。
- 不改 D1-a（引用叶子进侧表、不裸内联进字节区）——`struct-value-semantics.md:325-335` 已否决 D1-b。
- 不给 struct 加 base/vtable（`:167-172`）。
- 不做全量单态化（不为每个实例化生成代码），**只做布局层的具体化**。
- 不做「读出即复制 + 禁止写穿」：它**修不了 ①②**（源变量在存入之后被改，读时拷的是已被改过的那块 blob）。

## ✅ 已裁决（2026-09-22，User）

1. 走**按实例化算布局**，闸门＝**只在型参实参是 blob struct 时**生效。否决装箱方案。
2. P3a 容器装箱**最终删除，但分两步**（A3）：本 change 只做**字段槽**；
   **紧接一条 change 做容器密集化**，届时才删 P3a 装箱。

### 为什么容器不能在本 change 里一起删

**按实例化算布局管不到容器 backing。** 本 change 修的是**固定槽位**（`Pair<P2,int>.First`、
`CBox<P2>.Item`）——布局在实例化时算定即可。而容器的 backing 是 `List<T>` **内部**的
`new T[n]`，那段代码**只编一份**，发射时元素类型名就是字面的 `"T"`
（`ExprTyper.z42:348` `PrimModel.SurfaceName(elem.Name())`）⇒ 运行期 `array_new("T")`
不是 struct ⇒ **引用背衬数组**。

**P3a 的装箱在那里是承重的**：它正是让 struct 元素能活在引用数组里的东西。
若本 change 直接删它而 backing 仍是引用数组 ⇒ 裸 `StructRef`（栈帧句柄）重新被存进 GC 堆数组
⇒ **原样放回我们正在修的那个 use-after-free**。

容器密集化可行，但需要**另一套机制**：把**类级**类型实参送到 `new T[n]` 那个分配点。
地基已存在（`TypeDescCold.type_args`、`ObjNew` 指令携带 `type_args`、
`frame.method_type_args` + `exec_support.rs:53-64` 的标记回填），但那是具体化类型实参，
不是布局具体化的副产品——正是 P3b 文档所说的「密度」终局。**单开 change，单独评估。**

## 本 change 的边界（写死，防后来者误解）

| 形态 | 本 change | 备注 |
|---|---|---|
| 泛型 **struct** 的 `T` 字段槽（`Pair<P2,int>.First`）| ✅ 修 | 形态 ①③ |
| 泛型 **class** 的 `T` 字段槽（`CBox<P2>.Item`）| ✅ 修 | 形态 ②，含 use-after-free |
| 泛型**容器 backing**（`List<P2>` 的 `T[]`）| ❌ 不动 | P3a 装箱继续承重，下一条 change 处理 |
| 型参实参**不是** blob struct（`Pair<string,int>`）| ❌ 不动 | 闸门；字节必须逐字不变 |
