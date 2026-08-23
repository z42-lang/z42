# Design: 泛型 Activator.CreateInstance<T>（G3）

## Architecture
```
用户代码  Activator.CreateInstance<Point>()
   │
   ▼  （z42.core，泛型方法薄壳）
public static T CreateInstance<T>() { return (T)Activator.CreateInstance(typeof(T)); }
   │                                          │
   │  typeof(T) → MethodTypeArgInsn           │  __activator_create(Type) builtin（既有）
   │  读 frame.method_type_args[0]            ▼
   ▼  make_type_from_name → Std.Type(handle)  reflection.rs::builtin_activator_create
                                              alloc + 跑无参 ctor
```

## Decisions

### Decision 1: 薄泛型壳，复用既有 native（唯一合理方案）
**问题：** 如何实现 `CreateInstance<T>()`？
**选项：**
- A **z42 泛型薄壳** `(T)CreateInstance(typeof(T))`——零 native / 零格式改动，复用 `__activator_create` + `typeof(T)`。
- B 新增专用 native `__activator_create_generic<T>`——多此一举（native 拿到的仍是 Type，与非泛型无异）。
**决定：** 选 **A**。C# 的 `Activator.CreateInstance<T>()` 语义就是 `CreateInstance(typeof(T))` 的便捷泛型形；
z42 已有 `typeof(T)`（方法级，#240）+ `CreateInstance(Type)` native（#249 起）→ 薄壳直接组合，无任何底座改动。
serde `Deserialize<T>` 内部已用同款 `(T)Activator.CreateInstance(typeof(T))` 并在 CI 全绿验证过该路径。

### Decision 2: 只做无参形，参数化留 Deferred
**问题：** 是否同时做 `CreateInstance<T>(args...)`？
**决定：** 否。参数化构造用户已可经 `ConstructorInfo.Invoke`（#249）达成；泛型无参形是 C# `Activator` 的
主力便捷入口，先收口三件套。参数化 + 值类型语义留 Deferred（design 无 Deferred 段，登记 roadmap Backlog）。

### Decision 3: 方法级形参转发 = `$mta:<idx>` 标记，运行期按调用方 frame 解析（实施期发现）
**问题（实施期暴露）：** `CreateInstance<T>()` 在**泛型方法内**被调（`Foo<T>() { CreateInstance<T>() }`）时，
调用点把类型实参 T 发成字面 "T"，被调方 `typeof(T)` → `make_type_from_name("T")` 落空 → 无 handle。这是
#240 method-generics 的**通用缺口**（任何 `Foo<T>()` 转发 T 给嵌套 `Bar<T>()` 都中招，非 Activator 特有）。
**根因：** `CallInsn.method_type_args` 是**编译期静态字符串**，无法表达「转发调用方运行期的 T」。
**选项：**
- A **转发标记 `$mta:<idx>`**：编译期若类型实参是外层方法级形参，发标记 `$mta:<idx>`（idx = 方法级下标）；
  运行期 `exec_call`/`exec_vcall` 在**拷入被调方 frame 前**，把标记按**调用方** frame.method_type_args[idx]
  替换成具体名。`method_type_args` 仍是 string[] → **零格式改动、零新 opcode**。
- B 调用约定改传 Type 值（Object）而非字符串——大改 calling convention，牵连 JIT。
**决定：** 选 **A**。最小、无格式改动、语义正确。嵌套性天然成立：每层调用在设置自己 callee frame 前解析，
故调用方 frame 的 slot 已是具体名（`Make<ActWidget>` → Make frame=["…ActWidget"] → `CreateInstance<$mta:0>`
解析为 "…ActWidget"）。**无标记的调用（具体实参 / 类级形参）产物字节不变**（`starts_with("$mta:")` 门控，
无标记不进解析、不 alloc）。
**限制：** 只做**顶层**类型实参转发（`Bar<T>`）；`Bar<List<T>>` 里嵌套的 T（`$mta` 落在尖括号内）需
`make_type_from_name` 角括号解析里嵌转发 → Deferred。

## Implementation Notes
- 返回类型 `T`，body `return (T)Activator.CreateInstance(typeof(T));`。`(T)` cast 在泛型方法内对 `object`
  结果收窄——与 serde `Deserialize<T>` 的 `(T)JsonBinder.FromJson(...)` 同构，已验证可行。
- **跨包 typeof(T) 短名 handle**：`CreateInstance<T>` 在 z42.core，用户跨包调用时 `typeof(T)` 的 method_type_arg
  可能是短名 → 依赖 `make_type_from_name` 无点短名唯一简单名兜底（add-json-serde 已落地，本 change 不再改 runtime）。
- 头注去掉「CreateInstance<T> deferred」措辞。

## Testing Strategy
- 单元 [Test]（`xtask test stdlib z42.core` 或 reflection.z42 所在 lib）：
  `CreateInstance<T>` 用户类无参 ctor 往返 + 类型正确（`as T != null`）+ ctor 副作用 + 泛型方法内转发。
- **无格式 bump → 本地即可完整 GREEN**（单代自举，`xtask test` 全 stage + self-host 不动点）。
