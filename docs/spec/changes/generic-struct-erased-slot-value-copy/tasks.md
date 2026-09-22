# Tasks: generic-struct-erased-slot-value-copy

> 状态：🟡 进行中（本包范围已完成并全绿，未开 PR）| 创建：2026-09-22

**变更说明：** 泛型类型的**实例化**拿到自己的布局（型参字段变真内联字节）并按该布局特化其成员。
三种值语义违反全部修复，其中形态 ② 是 **use-after-free**。

**原因：** `StructLayout._kindOf("A")`（型参名）落 `StructLeafKind.GcRef` 8 字节句柄——布局**按定义**算。
**那个槽物理上放不下字节**，所以「存入时复制」只是给错误表示打补丁。

## 实测（修前 → 修后）

| # | 形态 | 修前 | C# / 修后 |
|---|---|---|---|
| ① | `new Pair<P2,int>(inner,1)` 后改 `inner.Y` | 99 | 2 |
| ② | `new CBox<P2>(c)`（泛型 **class**）后改 `c.Y` | 77 | 2 |
| ③ | `var q = p3; q.First.Y = 9` | 9 | 2 |

形态 ②：栈帧作用域的 arena `StructRef` 存进 GC 堆对象字段槽，ctor 帧一弹 arena 截断 ⇒ 悬垂句柄。
`77` 只是最温和的表现。⇒ **健全性修复，不只是语义对齐。**

## User 裁决

1. **对齐 C#**（否决「只修③」「编译期禁止」「继续挂着」）
2. **否决装箱方案**——装箱是引用操作、引入引用身份、每次存入一次堆分配；C# 零装箱因为泛型具体化
3. **A3 分两步**：本 change 只做**字段槽**；容器密集化（删 P3a 装箱）另开 change
4. **实例化是独立类型**（对齐 C#）⇒ 身份与布局**解耦**

## 已完成（5 个 commit，各自全绿）

- [x] 1.1 `StructLayout` 按实例化算布局（代换表；`InstName`/`BaseNameOf`/`InstArgsOf` 带嵌套深度跟踪）
- [x] 1.2 合成类描述符**惰性投送**（没人查 ⇒ 行为零变化）
- [x] 1.3 B1 按布局特化成员 —— 形态 ①③ 修复
- [x] 1.4 类型身份与布局**解耦** —— 身份名 vs 特化名两个正交概念
- [x] 1.5 泛型 class 的 `T` 槽真内联 —— 形态 ② 修复
- [x] 1.6 测试：golden `src/tests/types/generic_inst_value_semantics.z42`
      （三形态 + 闸门 + 嵌套 + **带引用叶子** + struct 数组 + 值语义传参），
      **interp / jit 双通过**；**阴性对照**：停用特化 → 立刻判红
      （`struct ref leaf at byte offset 0 not in type layout`）
- [x] 1.7 全量 GREEN：`xtask test` 0 failed；`xtask test e2e --mode jit` 330/64/3 全过；
      自举不动点 gen1==gen2（含在 `test compiler`）

## 🔴 遗留项（必须在归档/PR 前让 User 知晓）

### R1. 普通泛型 class 不取独立身份（对裁决 4 的保留）

给一个实例化独立身份 ⇒ 必须发**完整**类描述符。blob struct 只需布局块；普通引用类要合成
**基类链 / 接口 / vtable / 静态字段**全套。故当前口径：

| 类别 | 独立身份 | 理由 |
|---|---|---|
| blob struct 实例化 | ✅ | 恒发描述符 |
| 有内联 struct 字段的泛型 class | ✅ | 发内联 + 对象布局块 |
| **普通泛型 class**（`MulticastException<bool>`） | ❌ 沿用擦除裸名 | 需完整类描述符，另开 change |

身份名已与「**确实会发描述符**」严格对齐——否则 ObjNew 会用一个运行期解析不到的类型名
（实测 `type Std.MulticastException<bool> could not be resolved`）。

### R2. 跨包实例化不特化

闸门之二：泛型定义须在**本包**。消费方编译只读依赖**签名**（`DepReconcile` 走 `ReadModuleSigs`），
拿不到生产方方法体、无法重发。两侧都不特化 ⇒ 行为与今天逐字不变（不是「修一半更坏」）。

⚠️ **`Std.ValueTuple` 在 z42.core ⇒ 用户写 `(P2,int)` 是跨包 ⇒ 本轮修不了。**
覆盖它需另开「泛型体随包投送 + 消费方重发」，量级 L。

> 因此**不新增跨包一致性用例**：该路径本轮无行为变化，写个用例只会给人「已覆盖」的错觉。

### R3. 容器密集化 + 删 P3a 装箱（A3 第二步）

`List<T>` 内部 `new T[n]` 只编一份、元素名是字面 `"T"` ⇒ 引用背衬 ⇒ P3a 装箱在那里**承重**。
本 change 不动它。需要把**类级**类型实参送到 `array_new` 分配点（地基已有：`TypeDescCold.type_args`、
`ObjNew` 携 `type_args`、`frame.method_type_args` + 标记回填）。

## ⭐ 九个连环缺陷（全部实测逼出，无一设计时预见）

**共同形状：凡是把 owner 布局烘焙进指令的地方，都得跟着特化。** 每修一处都以为齐了。

1. 调用点派发：`StructAlloc` 携实例化 FQ 名 + ctor 指向特化那份
2. 特化体内 `this` 的身份：其静态类型是**定义**，不映射过去就按定义布局判字段种类
3. 规范名必须**递归**：`Z42Type.Name()` 对嵌套实例化给 `Pair<Int32, Int64>`（wrapper 名 **+ 空格**）
4. 🔴 合成描述符必须 **FQ 名**：短名**静默 miss** → 回落「只有 size、空引用位图」兜底布局 ⇒
   **零引用叶子的实例化恰好照常工作**，有引用叶子的才炸。**「能跑」掩盖了它两轮。**
5. 数组元素名也要规范名，否则 `try_struct_backed` 全名 miss ⇒ 按定义布局建数组（越界）
6. 成员 IR 名的 owner 前缀在 `IrGenMemberEmitter` 里有 **9 处** ⇒ 收敛到单一出口 `_irOwner`
7. 属性 getter 调用点用定义名，没派发到特化版
8. 合成成员（`Equals$1` / `[Record] ToString`）只在 `EmitClass` 发了，特化通道漏
9. 合成 `Equals` 的 `other is <类型>` 用定义名 ⇒ 装箱值类型是实例化 ⇒ **值相等静默误返 false**

## ⭐⭐ 两条通用教训（同一天各栽一次）

- **移除闸门前，先确认它在挡几件事。** 布局闸门还顺带挡着「实参仍是型参」的**伪实例化**
  （`ListEnumerator<T>`）；解耦身份拿掉它后，z42.core 导出了一个字面叫 `ListEnumerator<T>` 的类，
  污染导入侧类型表。现显式化为 `_isConcreteTypeArg`。
- **放宽判据前，先确认它在护什么下游。** `_isBlobStruct(m.Type())` 护着 `_blobStructName` 不被喂
  型参；只放宽判据不动下游 ⇒ 编译器自身崩。正解是连同下游的**信息来源**一起换
  （字段 struct 名改从 `InlineFieldTypeName` 取）。

## ⭐ 闸门判据改过两次（两次都判错）

- ❌ v1「有没有 blob struct 实参」：定义把**所有**型参字段当 8B 引用叶子，故 `Pair<int,long>`
  虽无 struct 实参布局也不同（零引用叶子 vs 两个）。
- ✅ v2 `InstDiffersFromDef`（size / 引用位图 / 字段偏移与种类逐项比）。**自带闸门性质**：
  布局相同 ⇒ 特化是 no-op ⇒ 跳过即字节不变。

## ⭐ 「实例化是独立类型」的两个硬后果

1. 合成 `Equals` 的 `is` 检查要用实例化名，否则**值相等静默误返 false**
2. **VCall 按运行期类型名派发** ⇒ 身份一变，`MyList<int>.Add` 找不到（19 条测试红）。
   必须配**擦除名回落**（miss 后剥实参重试）——只在 miss 路径跑，热派发不受影响。

## 零格式 bump

TYPE 段 = `count + count×描述符`，追加条目 reader 按 count 循环照读。运行期 `resolve_layout`
本就按**原样字符串名** `try_lookup_type`。唯一运行期改动：`exec_array.rs` 的 `try_struct_backed`
先试全名、miss 再剥（向后兼容）+ `vcall_resolve.rs` 的擦除名回落。

## 下一步

- [ ] 文档同步（`struct-value-semantics.md` 的 Deferred 条目 → ✅；`docs/roadmap.md:395`）
- [ ] 归档 + PR
