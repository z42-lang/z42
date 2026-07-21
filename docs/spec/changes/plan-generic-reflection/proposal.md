# Proposal: G0 —— 泛型反射规划（MakeGenericType / CreateInstance / 泛型方法 Invoke）

> 状态：🔴 DRAFT，待 User 确认规划方向。**这是 0.4.x G 流的 G0 设计规划**（不写实现代码，产出后续 G1–G3 的路线）。
> 子系统：主要 **runtime**（反射 builtin + 实例化）+ **stdlib**（Type/Activator/MethodInfo API）；**编译器基本不动**（见下「模型优势」）。

## Why

反射的只读 + 非泛型调用面已完整（0.3.12 收口：Invoke ✅ / IsEnum ✅ / 接口成员枚举 ✅）。**反射的下一个真价值点是泛型反射三件套**——`Type.MakeGenericType`、`Activator.CreateInstance<T>` / 参数化 CreateInstance、泛型方法 `MethodInfo.Invoke`。这是 0.4.x **L 流招牌 `Deserialize<T>` 完整泛型 serde 的直接前置**（roadmap G 流，2026-06-23 User 裁决"硬上"）。

roadmap 假设 G 流依赖"运行期泛型实例化"这个**重前置**。**本 G0 勘察证伪了这个"重"**——见下模型优势。

## 模型优势（勘察发现，重定 G 流的量级）

z42 泛型是 **C# 式具化类型擦除（reified erasure）**，不是单态化：
- **一份字节码服务所有实例化**（generics.md §"代码共享 + 具化"）——无 `Box<int>` / `Box<string>` 分身代码。
- 类型参数**具化为 per-instance `type_args`**（`ScriptObject.type_args`，`ObjNew` 指令携带、`obj.new` 填充）。
- **`make_constructed_type(name, type_args)` 已能在运行期从"定义名 + 实参类型名"构造 `Type` 对象**（挂 `__typeArgs` 槽）——`typeof(Box<int>)` / `new Box<int>().GetType()` 皆走它。

⇒ **"运行期泛型实例化"在 z42 里不需要 codegen / 单态化**：造 `Box<int>` = 跑（擦除的）ctor + 设 `instance.type_args=[int]`；`MakeGenericType` ≈ 复用 `make_constructed_type`。**G 流因此主要是 runtime 反射 builtin + stdlib API，编译器基本不动**——远轻于 roadmap 预估。

## What Changes（G0 = 规划，产出 G1–G3 路线）

- 产出**泛型反射三件套的设计 + 分阶段实现路线**（design.md），把 roadmap 的 G0–G3 落成可执行的 change 序列，标注每件**复用什么 / 新增什么 / 真缺口在哪**。
- 三件套与既有机制的映射：
  - **MakeGenericType(defType, argTypes[])**：复用 `make_constructed_type`；缺口 = open-generic 定义型 `typeof(Box<>)` 的表示 + `Type.MakeGenericType` stdlib API + arg 数校验。
  - **参数化 / 泛型 CreateInstance**：扩 `builtin_activator_create` —— constructed Type 造实例时从 Type 的 `__typeArgs` 灌 `instance.type_args`（现只按定义 TypeDesc 造、丢 type_args）；`CreateInstance<T>` = 方法级 T 解析 + 复用之。
  - **泛型方法 Invoke**：`MethodInfo.MakeGenericMethod(argTypes) + Invoke` —— **最硬**：需方法级 type_args 线程（现只有类级 per-instance type_args；方法自身泛型参数在擦除模型下运行期如何供给 `typeof(U)` 待定）。
- **不在 G0 写实现**：G0 只出设计 + 路线 + open questions，各件在 G1–G3 逐个开 change（本地环境受限——并发 WIP + stale 种子——G0 纯文档正好不受影响）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/spec/changes/plan-generic-reflection/*` | NEW | G0 规划（proposal + design + spec + tasks）|
| `docs/design/language/reflection.md` | MODIFY | 泛型反射 Deferred 段：补 G 流路线指针（不改行为）|
| `docs/roadmap.md` | MODIFY | 0.4.x G 流条目细化（G0 完成 + G1–G3 量级重估为"轻，无 codegen"）|

**只读引用**：
- `docs/design/language/generics.md`（具化擦除模型）
- `src/runtime/src/corelib/reflection.rs`（`make_constructed_type` / `builtin_activator_create` / `builtin_type_generic_args`）
- `src/runtime/src/interp/exec_object.rs`（`ObjNew` 填 `instance.type_args`）+ `exec_address.rs`（`typeof(T)` 读 type_args）

## Out of Scope

- **G0 不写任何实现**（G1–G3 各自开 change）。
- **纯 Java 式擦除 / Rust 式单态化改造**——模型已定（C# 具化擦除），不动。
- （原列此处的"约束运行期检查"已上移为 **G1 必做**——见 Q3 裁决）。
- **协变 / 逆变 / 关联类型**——generics.md 另有 Deferred，不属本线。

## Open Questions（G0 待与 User 敲定，决定 G1–G3 边界）

- [x] Q1（User「可以」认可）：复用 `GetGenericTypeDefinition` 返回的定义型句柄作 MakeGenericType 接收者；G1 实现期核对该句柄是否已足够表示 open-generic（不足则补最小"未绑定定义 Type"标记）。
- [x] Q2（User「可以」认可，后置 G2）：泛型方法级 type_args 供给通道留 G2 深究；G1/G2(参数化 CreateInstance) 不依赖它。
- [x] **Q3（User 裁决 2026-07-21：必须校验）**：**MakeGenericType / 泛型 CreateInstance 运行期必须校验类型约束**（`where T: IShape` / `: class` / `: struct` / base-class / `new()`）——违背抛 `Std.Exception`。理由：反射构造是**绕过编译期类型检查的唯一入口**，编译期约束保证在此失效，**语言安全要求运行期自查**。这**不与** generics.md「VM 不检查约束」冲突——后者指编译期已检查的正常泛型路径；反射是逃逸口，必须自我把关。**上调为 G1 必做**（约束元数据 `type_param_constraints` 已在 TYPE 段，可读 + 经 `IsAssignableFrom` 校验）。
- [x] Q4（User「可以」认可）：交付顺序 G1（MakeGenericType + constructed CreateInstance + **约束校验**）→ G2（参数化 CreateInstance + 泛型方法 Invoke）→ G3（CreateInstance\<T\> + serde 端到端）。
