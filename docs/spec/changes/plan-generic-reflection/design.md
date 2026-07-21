# Design: G0 泛型反射规划

## 模型基线（决定量级）

z42 泛型 = **C# 式具化类型擦除**：一份字节码 + per-instance `type_args`（具化）。关键既有机制：

| 机制 | 位置 | 作用 |
|------|------|------|
| `ScriptObject.type_args: Vec<String>` | `metadata/types.rs` | 实例携带的具体类型实参（具化）|
| `ObjNew{type_args}` 指令 | `interp/exec_object.rs` | `new Box<int>()` 填 `instance.type_args=["int"]`（**编译期常量**）|
| `make_constructed_type(name, args)` | `corelib/reflection.rs` | 运行期从"定义名 + 实参名"造 `Type`（挂 `__typeArgs` 槽）|
| `builtin_type_generic_args` / `IsGenericTypeDefinition` / `GetGenericTypeDefinition` | `corelib/reflection.rs` | 已能读构造型的 args、判定/取定义型 |
| `builtin_activator_create(Type)` | `corelib/reflection.rs` | 无参 ctor 造实例（**按定义 TypeDesc，不灌 type_args**）|
| `typeof(T)` 读 per-instance type_args | `interp/exec_address.rs` | 泛型方法内 `typeof(T)` 从 reg0 实例 `type_args[idx]` 取 |

**推论**：runtime instantiation ≠ codegen。三件套主要是"把编译期常量 type_args 的路径，接通运行期动态 type_args"。

## 三件套 → 复用 / 缺口 / 落点

### ① MakeGenericType(defType, Type[] argTypes) → constructed Type

- **复用**：`make_constructed_type(defName, argNames)` 直接产出 constructed Type。
- **缺口**：
  a. **接收者 = open-generic 定义型**（`typeof(Box<>)`）——需确认其运行期表示（Q1）。若 `GetGenericTypeDefinition` 已返回可用定义型句柄，则 MakeGenericType 可对称接收。
  b. arg 数校验（def 的 type_param_count vs argTypes.len）→ 不符抛 `Std.Exception`。
  c. **约束校验（Q3 必做，安全要求）**：对每个类型参数，读 def 的 `type_param_constraints`（`ConstraintBundle`：接口约束 / base-class / `class` / `struct` / `new()` / type-param-ref），校验对应 arg Type 满足——违背抛 `Std.Exception`（信息含参数名 + 违背的约束）。**复用现有 `IsAssignableFrom` / 接口成员判定**做接口/base 约束；`struct`/`class` 查 arg 的 value-type 位；`new()` 查无参 ctor 可达。
  d. stdlib：`Std.Type.MakeGenericType(params Type[])` + `[Native]` builtin `__type_make_generic`。
- **落点**：`reflection.rs` 新 builtin（读 def Type 名 + arg Type 名 → 校验约束 → `make_constructed_type`）；`Type.z42` 加方法。**纯 runtime**（约束元数据已在 TypeDesc.type_param_constraints）。
- **喂 serde**：`Deserialize<T>` 可先 `typeof(List<>).MakeGenericType(elemType)` 再 CreateInstance。

### ② 参数化 / 构造泛型 CreateInstance(Type) + CreateInstance<T>

- **复用**：`builtin_activator_create` 无参 ctor 路径。
- **缺口**：
  a. **constructed Type 造实例要灌 type_args**——现按定义 TypeDesc 造、`instance.type_args` 空 → 反射信息丢。改：从 Type 的 `__typeArgs` 槽读实参名，设进新实例的 `type_args`（镜像 ObjNew 填法）。
  b. **参数化 ctor**（`CreateInstance(Type, args)`）——ctor 重载决议 + 传参（现只无参）。这块与泛型正交，可并入或单列。
  c. **`CreateInstance<T>()`**——`T` 是**方法级**类型参数；调用点已知 T 的具体类型（编译期或经 per-instance）→ 降级为 `CreateInstance(typeof(T))`。需 Q2 的方法级 type_args 供给。
- **落点**：扩 `builtin_activator_create`（constructed 分支灌 type_args）；`Activator.z42` 加泛型/参数化重载。**纯 runtime**（除 Q2 的方法级 type_args 若需 IR 支持）。

### ③ 泛型方法 Invoke（MethodInfo.MakeGenericMethod + Invoke）——最硬

- **现状**：`MethodInfo.Invoke`（非泛型）已落地（复用 `exec_function`）。
- **缺口（真前置）**：泛型方法 `T Foo<U>(U x)` 的 `<U>` 在**擦除模型**下——
  - 方法体内 `typeof(U)` / `new U()` / `default(U)` 需要 U 的具体类型；
  - 类级 type_args 走 per-instance 槽（reg0.type_args），**方法级 type_args 无载体**（方法不是实例）。
  - ⇒ 需要一个**方法级 type_args 供给通道**：invoke 时把 `MakeGenericMethod(argTypes)` 的实参传入执行上下文，方法体内 `typeof(U)` 从该上下文取。这是三件套唯一**可能触及 IR / interp 执行帧**的点（Q2）。
- **落点**：待 Q2 定案——最小方案可能是"invoke 时把 method type_args 放进 Frame 的一个槽，`GenericParamAddr` 类指令按 method-level index 读"。**这件排最后（G2/G3），前两件先交付喂 serde。**

## 分阶段实现路线（G0 产出）

| 阶段 | 内容 | 量级 | 依赖 |
|------|------|------|------|
| **G1** | MakeGenericType（①）+ constructed CreateInstance 灌 type_args（②a）| 纯 runtime，轻 | Q1（open-generic 表示）|
| **G2** | 参数化 CreateInstance（②b）+ 泛型方法 Invoke（③）| 中——③ 触 Q2 方法级 type_args | Q2 定案 |
| **G3** | `CreateInstance<T>()` 语法糖（②c）+ serde 端到端串联 | 轻 | G1+G2 |

> 与 roadmap 对齐：roadmap G1「运行期泛型实例化」→ 本 G1（MakeGenericType + 灌 type_args，**发现无需 codegen**）；G2「泛型方法 Invoke + MakeGenericType」→ MakeGenericType 提到 G1、泛型方法 Invoke 归 G2；G3「Activator.CreateInstance\<T\>」→ 本 G3。**量级整体下修**（擦除模型红利）。

## Decisions（G0 层）

### Decision 1: 复用具化擦除机制，不引入单态化 / 运行期 codegen
**理由**：generics.md 已定 C# 具化擦除；`make_constructed_type` + per-instance `type_args` 已是运行期具化的完整地基。三件套是"接通动态 type_args 路径"，非新执行模型。

### Decision 2: 交付顺序 G1（喂 serde 基本盘）→ G2（硬点方法 Invoke）→ G3（糖 + 串联）
**理由**：`Deserialize<T>` 招牌的**基本盘**（造 `List<T>` / `Dictionary<K,V>` / 用户类实例并填字段）只需 MakeGenericType + constructed CreateInstance（G1）——不需要泛型方法 Invoke（③）。先交付 G1 即可解锁 serde 主路径，把最硬的 ③ 后置、单独攻坚。

### Decision 3: 运行期**必须**校验泛型约束（User 裁决 2026-07-21，安全要求）
**决定**：MakeGenericType / 泛型 CreateInstance **必须**在运行期校验 `where` 约束，违背抛 `Std.Exception`。
**理由**：反射构造是**绕过编译期类型检查的唯一入口**——正常泛型路径编译期已保证约束，但反射让用户在运行期任意组合定义型 + 实参，编译期保证在此**失效**。语言安全（z42 设计目标）要求这个逃逸口自我把关，否则 `List<>.MakeGenericType(SomeInvalidType)` 会造出违反约束的类型、后续崩在意想不到的地方。
**与 generics.md 不冲突**：generics.md「VM 不检查约束」指**编译期已检查的正常路径**（VM 不重复检查）；反射是编译期未覆盖的新入口，必须补检查。
**可行性**：约束元数据 `type_param_constraints`（`ConstraintBundle`）已随 TYPE 段持久化、已被 loader 读入 TypeDesc；校验复用现成 `IsAssignableFrom` / 接口判定 + value-type 位 + ctor 可达性。**零新元数据、纯 runtime。**

## Testing Strategy（G1–G3 各自）

- G1：`typeof(List<>).MakeGenericType(typeof(int))` == `typeof(List<int>)`（名/GetGenericArguments 一致）；`Activator.CreateInstance` on constructed → 实例 `GetType().GetGenericArguments()` 带实参。
- G2：泛型方法 `Foo<U>` MakeGenericMethod+Invoke，方法体 `typeof(U)` 正确。
- G3：`CreateInstance<T>()`；`Deserialize<T>` 端到端（喂 L 流招牌）。
- 每阶段 GREEN 以 CI 为准（本地环境受限；纯 runtime 改动零编译器/无格式 bump → 自举天然稳，除非 ③ 触 IR）。
