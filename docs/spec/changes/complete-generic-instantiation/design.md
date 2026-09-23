# Design: 补完泛型实例化模型

> 本文件 P1 段已定稿；P2 / P3 段是方向记述，各自开工前回到阶段 5 定稿。

## 支点：两个正交概念（承自 #774，不得混用）

- **身份名**：描述符 / `StructAlloc` / `ObjNew` / 数组元素名。**所有**具体实例化都有。
- **特化名**：布局查询 / 成员派发。**仅布局与定义不同**者才有。

布局相同的实例化有独立身份但**共享定义的成员体**——共享体按定义偏移烘焙，布局既相同则偏移对得上。

⚠️ **身份名必须与「确实会发描述符」严格对齐**，否则 `ObjNew` 会用一个运行期解析不到的类型名
（#774 实测 `type Std.MulticastException<bool> could not be resolved`）。运行期两条解析源
（`module.type_registry` / `ctx.try_lookup_type`）**都按名字符串**。

---

## P1：跨包闸门放宽到「成员全可合成」的导入泛型

### 判据（D1）——为什么不按名字判

需要回答的是「消费方重新合成这个类型的全部成员，会不会丢掉用户写的东西」。三个候选：

| 方案 | 代价 | 否决理由 |
|---|---|---|
| ❌ 按导出方法**名**判「⊆ 合成集」 | 零改动 | 用户在 `[Record] struct` 上手写一个 `ToString()`，名与签名都与合成版**一模一样** ⇒ 消费方拿合成版顶替，**静默丢用户实现**。z42 不接受静默错。 |
| ❌ 新增 `CLASS_FLAG_*` 位 | **格式 bump** | `class_flags: u8` 的 bit0–7 **已全占满**（ABSTRACT/SEALED/STRUCT/RECORD/INTERFACE/ENUM/DELEGATE/HAS_INLINE_STRUCT），加位须加宽字段 ⇒ zbc minor bump ⇒ 拖进两代自举。 |
| ✅ **新增 `METHOD_FLAG_SYNTHESIZED = 1 << 4`** | 零格式 bump | `method_flags: u8` 只用了 bit0–3（VIRTUAL/ABSTRACT/SEALED/SRET），**bit4–7 空闲**。 |

**判据定稿**：

```
可跨包特化(inst) ⟺ 定义是 struct
                 ∧ 定义带 CLASS_FLAG_RECORD
                 ∧ 定义的每一个导出成员都带 METHOD_FLAG_SYNTHESIZED
```

三条缺一不可：`struct` 决定它走 blob 布局；`RECORD` 决定消费方**知道怎么合成**（主构造器
+ 值相等/哈希/ToString 的规则）；全员 synthesized 保证**没有任何用户实现会被顶替**。

**为什么这条判据自带向后兼容**：旧种子 z42c 不打 `METHOD_FLAG_SYNTHESIZED` ⇒ 消费方读到 0
⇒ 判否 ⇒ 退回今天的 `obj_new` 表示、逐字不变。旧 VM 读到 bit4 置位的 SIGS ⇒ 它按 u8 读、
不认的位本就忽略。**两个方向都优雅降级，无需分阶段引入。**

> 对照 P2 的静态字段换键——那条**不是**优雅降级（新旧键互不相认），所以 User 裁决它走
> 分阶段引入。两者的差别正在于此，不要把 P1 的结论套到 P2。

### 机制（D2）——反造合成 decl，复用 #774 的整条通道

消费方持有导入类的**有序字段表**（`ExportedClassZ.Fields` → `ImportedSymbolLoader._fillClass`
→ `ct.AddOwnField`，"G18-import：实例字段入 OwnField* **有序**元数据"）。对一个
`[Record] struct X<T1..Tn>(F1 f1, …)`，这份字段表**完全确定**了它的声明。

⇒ 不新写一套「从元数据合成成员」的发射器（那会与 AST 那套各判各的，正是 #774 九个连环缺陷
的共同形状）。改为**反造一个与本地声明等价的 `ClassDecl`**，`Put` 进 `IrGen.GenericDecls`，
其余一行不改地走 `IrGenTypeEmitter.EmitInstantiation`。

```
ImportedSymbolLoader  ──(字段表 + 型参名)──▶  ImportedGenericSynth.SynthDecl()
                                                      │
                                                      ▼
                                          IrGen.GenericDecls.Put(名, 合成 Decl)
                                                      │
                            ExprEmitter 闸门命中 ──────┤
                                                      ▼
                                   IrGenTypeEmitter.EmitInstantiation（#774 既有通道）
```

**单一出口**：闸门判据收敛到一个函数（`_canSpecializeDef`），`ExprEmitter.z42:554` 与 `:604`
**都调它**。#774 的教训 6 就是「同一判据散在 9 处 ⇒ 只改一处必漏」。

### 已知连带影响（D3）

跨包元组今天是 `obj_new` **堆对象** + 按名 `field_get`；命中后变成 `struct_alloc` **栈 blob**
+ 偏移烘焙。这对**所有**元组生效（不止 struct 元素那些）——`(int,string)` 的实例化布局
与定义布局也不同（引用位图：定义 2 个引用叶子 vs 实例化 1 个）。

- **正确性**：今天基元元组**已经是对的**（实测 `(int,string)` 复制/传参均不串），改后仍对。
  真正被修的是型参槽里装 blob struct 的那些。
- **性能**：顺带去掉每个元组字面量的一次堆分配。
- **字节 diff**：编译器与 stdlib **实际不使用元组值**（`grep Item1|Item2` 的 14 处命中全是
  注释与解析器代码）⇒ 自举字节 diff 风险低，diff 局限在测试与用户代码。

### 必须早验的风险（D4）

🔴 **同名描述符跨模块重复**：两个消费方包若都用 `(int,string)`，各自会发一条名为
`Std.ValueTuple2<Int32,String>` 的描述符。两条内容**逐字相同**，但运行期
`build_type_registry` 是**按模块**建、`try_lookup_type` 是**全局惰性**的。仓库有
`fix-package-identity-gate` / `fix-crosspkg-static-ns-collision` 的历史，说明同名冲突是被
管制的。⇒ **tasks 第一项就是写一个双消费方包的用例把这件事钉死**，不要等实施到后面才撞。

---

## P2：泛型 class 独立身份（方向记述，开工前定稿）

四块，缺一块模型就自相矛盾：

1. **完整实例化类描述符**：`_instClassDesc` 今天硬编码 `base = "Std.Object"`、无接口、无静态
   字段、且 `il.FieldCount <= 0` 直接返回 null。需按定义的基类链 / 接口表 / 静态字段表合成。
   （**vtable 不需要**——运行期 `build_type_registry` 从 `own_methods` + 基链 merge。）
2. **`is` / `as` 带上实参**：`_bindIsExpr` / `_bindAsExpr` 今天 `tn = (ix.Type as NamedType).Name`，
   把 `NamedType.Args` 扔掉。改为走与 `_instIdentityName` **同一个**规范名函数。
3. **静态字段按实例化分槽**：键从 `QualifyClass(裸名) + "." + 字段` 改为带实例化名。
   ⚠️ **自举敏感**（换键 ⇒ 旧种子打旧键、当前源打新键）。User 已裁决走 `bootstrap-seed.md`
   的分阶段引入：support 先行、晚一个 nightly 再 use。
4. **两处解析器缺口**：`(GBox<string>)o` 与 `GBox<int>.Count`。两处今天都是**定长 token
   前瞻**（`_peekAt(2) == RParen` / `<…>` 后须紧跟 `(`），需改成回溯式「试解析一个类型再看闭括号」。
   `_parseType()` 本身早已支持闭合泛型（`as` 走的就是它），瓶颈纯在前瞻形状。

---

## P3：容器密集化（方向记述，开工前定稿）

把**类级**类型实参送到 `List<T>` 内部 `new T[n]` 的分配点。地基已有：`TypeDescCold.type_args`、
`ObjNew` 携 `type_args`、`frame.method_type_args` + `exec_support.rs` 的标记回填。

⚪ **这条不修 bug**（实测 `List<P2>` 值语义已正确，装箱顺带给了拷贝语义）。收益＝密度与分配
次数，须由 benchmark 说话，**不达标则不合**。达标线开工前定。

---

## 验证纪律（三个 PR 共用）

- ⚠️ **`xtask test` 全绿 ≠ JIT 验过**：golden 阶段只跑 interp。改 struct 访问路径**必须**
  显式 `xtask test e2e --mode jit`。（`./xtask test e2e jit` 会因位置参数错静默 `rc=2`。）
- **阴性对照只变一个变量**：不命中闸门的实例化，编译产物必须**逐字节不变**。
- 自举字节不动点 `gen1 == gen2`。
