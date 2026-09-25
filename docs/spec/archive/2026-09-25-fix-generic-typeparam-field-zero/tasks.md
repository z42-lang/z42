# Tasks: 泛型型参字段的零值按实例化取

> 状态：🟢 已完成 | 创建：2026-09-25 | 完成：2026-09-25
> 分支/worktree：`close-value-type-non-null` @ `wt-refnull` | 基于：origin/main `47dc5e1a7` (#813)
> 类型：`fix`（**vm** —— 运行期分配点；编译器与格式都不动）
> 授权：User「那请你修复」

## 现象（修前，interp / jit 双验，当前 main）

```z42
class GBox<T> { public T V; public GBox() { } }
GBox<int> g = new GBox<int>();

object o = g.V;     // → null           （C#：0）
g.V == 0            // → interp false / jit true   ⇐ tier-up 前后结论相反
int y = g.V;        // → int 局部里装着 Null       ⇐ 「值类型槽永不含 Null」被破
g.V + 1             // → 内部错误 `type mismatch in arithmetic: Null vs I64(1)`（catch 抓不到）
```

`T = bool` 时：`== true` 与 `== false` **同时为 false**，`if (g.V)` 崩
`BrCond expects bool, got Null`。

## 根因

`alloc_object` 按 composed layout 分配「零字节区 + `Null` 引用区」——**这是对的**。
问题在于型参字段 `T V` 的槽被分类成**引用槽**（布局按定义算，型参名不是基元），
于是它的零值是 `Null` 而不是 `T` 实例化后的零值。

**同一个坑数组早就修过**：`ArrayNew`（`interp/exec_array.rs`）会按
`type_param_kind` / `type_param_index` 从 `frame.method_type_args` 或收者的
`type_args()` 解析出具体类型，再取 `default_value_for(concrete)`；
只有**对象字段槽**没做这件事。⇒ 本变更把同一条路接到 ObjNew 上。

## 为什么不从编译器改布局

让型参字段变真内联字节是 `complete-generic-instantiation` /
`generic-struct-erased-slot-value-copy` 的内容（「单调化闭包」），那条线在飞
（`wt-geninst` 有未提交改动），且会动 `StructLayout._kindOf` 同一处判据。
本变更只在**分配点**按实例化写零值，**不动布局、不动编译器、不动格式**
—— 与那条线正交，它落地后本变更自然变成冗余的一层。

## 口径：只管基元值型参（与 `ArrayNew` 同一条窄口径）

`resolved` 是 struct / 引用类型时**一律不介入**，保持现行路径。理由照抄
`exec_array.rs` 的注释：把解析出的 **struct** 硬推成 struct-backing 会打坏
按引用存 struct 的泛型容器（`struct_generic_container: VCall: expected object,
got StructRefHeap`）。引用类型的零值本来就是 `Null`，无事可做。

## 进度概览

- [x] 1 共享判据 + 单测
- [x] 2 interp ObjNew（堆 + 栈两支）
- [x] 3 JIT ObjNew
- [x] 4 边界摸底 —— **两格修不了，根因在编译期，见下**
- [x] 5 e2e 用例（interp + jit）+ 阴性对照
- [x] 6 全量 GREEN + 指纹判定 + PR

## 1 共享判据
- [x] 1.1 `metadata/types/field.rs`：`generic_field_zero_overrides(td, type_args) -> Vec<(usize, Value)>`
      —— 字段 `type_tag` 命中 `td.type_params()` 的某个名字 ⇒ 按下标取 `type_args`，
      `default_value_for(concrete)` 非 `Null` 才产出一条覆写
- [x] 1.2 Rust 单测（7 条，`cargo test --lib generic_field_zero` 全过）：基元型参出零值 / 引用型参不产出 / 非泛型不产出 / 下标越界不 panic

## 2 interp
- [x] 2.1 `interp/exec_object.rs` 堆分支：`set_type_args` 之后应用覆写
- [x] 2.2 同文件 `stack_alloc` 分支：入 arena 前应用覆写（**两支必须同一步改**，
      否则 `Z42_STACKALLOC` 开关一拨行为就变）

## 3 JIT
- [x] 3.1 `jit/helpers/object.rs::jit_obj_new`：`set_type_args` 之后应用覆写
      （**与 2.1 绑同一提交** —— 只改一份 = tier-up 前后分叉，正是本 bug 现在的症状之一）

## 4 边界摸底（实测结果）

| 形态 | 修后 | 说明 |
|---|---|---|
| `GBox<int>` / `<double>` 直接实例化 | ✅ 零值 | 本变更 |
| `GBox<bool>`（`==true` / `==false` / `if`） | ✅ 三格全对 | 本变更 |
| `Multi<int,string>` 多型参 | ✅ `X`=0、`Y`=null | 按**名字**映射下标 |
| `Mixed<T>`（具体字段 + 型参字段） | ✅ 两者都对 | |
| `GBox<GBox<int>>` 嵌套 | ✅ null（正确） | 外层实参是引用类型 ⇒ 不介入 |
| `int[]` / `T[]` / stdlib 容器 | ✅ 不变 | 本就走 `ArrayNew` 的正确路径 |
| **interp vs jit** | ✅ **分叉消失** | 修前 `gi.V == 0` 一边 false 一边 true |
| 🔴 `class DInt : GBox<int> {}` 继承字段 | ❌ 仍是 Null | **运行期拿不到信息**，见下 |
| 🔴 `struct GS<T> { T F; }` | ❌ 仍是 Null | **运行期拿不到信息**，见下 |

- [x] 4.1 继承字段 —— **分配点修不了**。实测 `DInt` 的 TypeDesc：
      `base=Some("GBox")`、`tparams=[]`、ObjNew 的 `type_args=[]`、继承来的字段
      `V` 的 tag 仍是 `"T"`。⇒ **基类实参 `int` 在运行期元数据里根本不存在**
      （`base_name` 不带实参），没有任何可解析的来源。必须由编译器在派生类型上**代换**
      字段 tag（或让基类实参进元数据）。
- [x] 4.2 泛型 struct —— **分配点同样修不了**。实测 `new GS<int>()` 走
      `obj_new GS<int>`，解析到的是**特化后**的 TypeDesc `GS<int>`，但它
      `fields=[]`（存储走 struct blob 的叶子，不是对象槽），而
      `StructTypeLayout` 只有 `size` / `ref_offsets` / `ref_kinds`（u8）——
      **没有叶子的声明类型名** ⇒ 分不出「这个 ref 叶子是型参字段」。
      必须改 `StructLayout._kindOf`（让型参叶子变真字节），正是
      `generic-struct-erased-slot-value-copy` 自述的那条根因。
- [x] 4.3 嵌套实例化 —— ✅ 不介入（实参 `GBox<int>` 是引用类型）
- [x] 4.4 方法型参 / `default(T)` —— 本变更不碰该路径，全量 GREEN 复核
- [x] 4.5 两格如实记录，**未写成「已修」**；已回填到
      `archive/2026-09-25-enforce-value-type-non-null/` 那条洞

> ⚠️ **不变式的准确说法**：本变更把「值类型的存储槽永不含 `Value::Null`」从
> 「非泛型成立」推进到「**非泛型 + 直接实例化的泛型类字段**成立」。
> **继承链上的型参字段与泛型 struct 的型参字段仍不成立** —— 两者都要等
> 编译期单调化，不能靠运行期补。

## 5 用例
- [x] 5.1 扩 `src/tests/types/value_field_zero/`：补泛型格（int / bool / double + 引用对照）
      —— 这个用例此前**只有非泛型**，正是漏掉这一格的原因
- [x] 5.2 bool 三格都断言（`== true` / `== false` / `if`）—— bool 是这类 bug 的头号探针
- [x] 5.3 阴性对照 —— 取的是**更强的一种**：把 `generic_field_zero_overrides` 临时改成
      恒返回空（等于撤回修复本身，而不只是改个期望值），重建后用例
      **interp + jit 双双变红**（`VCall: expected object, got Null`）⇒ 证明它抓的是这个 bug，
      不是恰好绿。随后原样恢复。
- [x] 5.4 interp + jit 双模式

## 热路径代价

非泛型 ObjNew（绝大多数）在 `type_args.is_empty()` 一行就返回，**一次比较、零分配**
（`Vec::new()` 在没有 push 之前不分配）。泛型 ObjNew 多付「字段数 × 型参数」次字符串比较
（实际都是 1~2 × 1~2），只有真产出覆写时才分配一个极小的 Vec。
⇒ 未做进一步微优化：按本仓的规矩，**没有测量就不优化**（这条线上已有四次性能误判的记录）。

## 6 收尾
- [x] 6.1 `xtask test all` **GREEN**（8m51s / 15 stage 全过）+ `cargo test -p z42 --lib`
      （debug）**1380 + 21 passed / 0 failed**
- [x] 6.2 指纹判定：**不 bump**。`CompilerFingerprint` 管的是「同一份源码 + 同样格式，
      编出的**字节**却变了」（codegen / 优化 / typecheck / lowering）。本变更**一行编译器代码
      都没改**，发出的 IR 与 zbc 完全相同，变的只是 VM 在 ObjNew 时往槽里写什么 ⇒
      既不存在「哈希不变而发码变」的缓存条目，也没有格式变化。
      （对照：#746 同理不 bump；而 #791 要 bump 是因为**诊断**变而源码哈希不变。）
- [x] 6.3 回填 `archive/2026-09-25-enforce-value-type-non-null/tasks.md` 那条洞的状态
- [x] 6.4 归档（阶段 9，在本 PR 内）→ `archive/2026-09-25-fix-generic-typeparam-field-zero/`
