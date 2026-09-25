# Proposal: `FieldGet` 接受装箱值 struct —— 补齐唯一漏掉 `BoxedStruct` 的那条指令

> 状态：✅ 已实施（2026-09-25）。类型：**vm**（新增一条指令臂，无格式变更、无编译器改动）。
> User 口径「持续推进解决剩余问题」下落地；本刀刻意收窄到「不改任何约定」，
> 真正需要裁决的那条（④a + 返回位代换的整条链）留在「不做什么」里、未动手。

## Why

**同一个接收者形态，VM 的各条指令说法不一致。** 值 struct 经「擦除的返回位」流出泛型函数时，
运行期的值是 `Value::BoxedStruct`（带完整 `TypeDesc` + `struct_layout` 的堆盒）。今天：

| 指令 | 对 `BoxedStruct` | 落点 |
|---|---|---|
| `vcall` | ✅ 认（探自身槽位 → 候选查找） | `interp/vcall_resolve.rs:208` |
| `struct_fget_prim` / `struct_fset_prim` | ✅ 认（按 struct 布局读写字节） | `interp/exec_struct.rs:189,298` |
| `is` / `as_cast` | ✅ 认（拆箱进帧 arena） | `interp/exec_object_isa.rs:36,55` |
| 数组元素整读 | ✅ 认 | `interp/exec_array.rs:63` |
| 反射 `FieldInfo.GetValue` | ✅ 认（按名读，含嵌套/引用叶子） | `corelib/reflection/accessors.rs:139` |
| **`field_get`** | 🔴 **不认 —— 抛异常** | `interp/exec_object.rs:360`、`jit/helpers/object_field.rs:185` |

于是（2026-09-25 实测，main `fa7560e21` + 新建 SDK）：

```z42
struct Vec2 { public long X; public long Y;
    public Vec2(long x, long y) { this.X = x; this.Y = y; }
    public long Sum() { return this.X + this.Y; } }
T id<T>(T a) { return a; }

Vec2 v = new Vec2(7, 9);
Vec2 w = id(v);   Console.WriteLine(w.X);      // ✅ 7    —— 局部有具体静态类型 ⇒ 发 struct_fget_prim
Console.WriteLine(id(v).Sum());                // ✅ 16   —— vcall 认盒
Console.WriteLine(id(v).X);                    // 🔴 崩：FieldGet: ... got BoxedStruct(整屏堆转储)
Console.WriteLine(id<Vec2>(v).X);              // 🔴 同上（显式类型实参**并不能**绕开）
```

**为什么这条臂缺了会崩而不是走别的路**：调用点的静态类型是裸 `T`（`--dump-bound` 实测
`(call id … :T)`、成员类型 `:<unknown>`）⇒ `AccessEmitter._emitMember` 的 blob 分支
（`_isBlobStruct(m.Target.Type())`）判假 ⇒ 落通用 `field_get "X"`。而 `field_get` 是上表里
唯一没有 `BoxedStruct` 臂的一条。

**为什么不在编译器侧修（本刀刻意不做）**：让调用点的静态类型变成具体类型（把推断/显式的
类型实参代换进返回位）是正确方向，但实测它**立刻撞上调用约定**——见下「不做什么」。

不做的代价：用户拿到的是整屏 `ScriptObject { type_desc: TypeDesc { … } }` 堆转储，
且同一个表达式换成 `.Sum()` 就好、换成 `.X` 就崩，规律无法自解释。

## What Changes

给 `field_get` 补 `BoxedStruct` 臂（interp + JIT 两处），语义 = 反射那条已验证的路：
按名在盒的 struct 布局里定位叶子，基元 → `decode_prim`，引用 → `struct_refs` 侧表，
嵌套 struct → 拷出一份新盒（值语义）。

**复用而非新写**：`corelib/reflection/accessors.rs::boxed_struct_field_get` 正是这件事
（P4b `add-boxed-struct-identity` 为 `FieldInfo.GetValue` 写的，含 `validate_against`
布局对账），可见性从 `pub(super)` 放到 `pub(crate)` 即可。

- 无 zbc / zpkg 格式变更（不动指令编码，只加运行期一条接收者臂）
- 无编译器改动 ⇒ **零字节漂移、不需要两代自举**
- `field_set` **不在本刀**：写进「从擦除返回位流出的临时盒」是丢弃写（C# 直接拒绝
  `id(v).X = 5`），需要的是编译期诊断而不是运行期写通 ⇒ 独立裁决，见 Deferred

## 不做什么（以及为什么）

**不动「把类型实参代换进返回位」**。本刀调查中钉死了一条更深的缺口链，它需要独立裁决：

1. `MethodTypeArgSubst.ForExplicitTypeArgs` 用 `ms.TypeParamNames.Length` 当型参个数，
   而 `TypeParser._parseTypeParams` 里 `names = new string[4]` **不裁剪**就存进
   `TypeParamList.Names`（`Count` 另记）⇒ `n = 4 != TypeArgCount = 1` ⇒
   「arity 不符 → 原样返回」⇒ **凡本地声明的泛型方法/自由函数，显式 `<T>` 的签名代换从来没生效过**；
   跨包导入的因为 `ImportedSymbolLoader._tpNames` 产的是精确长度数组反而生效。
   判别性探针：声明**恰好 4 个**型参时代换立刻生效（`id4<Vec2,int,int,int>(v)` 的 bound 变
   `:Vec2`、实参不再装箱）。
2. 而代换一生效，`id4` 当场崩 `takes 1 physical argument(s), the call passes 2` ——
   调用方按具体返回类型（blob struct）加了 sret 隐藏实参，擦除的 callee 没有 sret。
   **这就是坑点 ④a（约束运算符派发 sret 不匹配）的同一个根**。

⇒ 依赖链是 **④a（泛型边界的物理约定统一）→ ④b（返回位代换）→ ⑤（单字段 struct 值语义）**，
与此前记录的方向相反。三条咬在一起、需要 `#814 add-iface-return-bridge` 那套桥接范式，
是独立且更大的一刀。本刀只把**崩溃**变成正确取值，不改任何约定。

## Deferred（登记，不在本刀）

- `substitute-generic-call-return-type`：上面 1+2 的整条链（含 `.Length`→声明个数 的一行修，
  必须与 sret 桥接同刀，单独修会把今天能跑的 `Vec2 w = id<Vec2>(v)` 打崩）。
- `reject-assign-to-erased-call-result`：`id(v).X = 5` 应报编译错误（今天行为待测）。
