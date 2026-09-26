# Spec: 泛型实例化作为运行期真正的类型

## MODIFIED Requirements

### Requirement: 实例化的类型测试区分类型实参

**Before:** `_bindIsExpr` / `_bindAsExpr` 取 `(NamedType).Name`，**丢弃类型实参**；运行期按
名字符串比。⇒ `o as GBox<string>` 在 `GBox<int>` 上**放行**。

**After:** 类型测试携类型实参，与实例化身份名走同一个规范名出口。

#### Scenario: 错误实例化的 as 返回 null
- **WHEN** `object o = new GBox<int>(42); GBox<string> b = o as GBox<string>;`
- **THEN** `b == null`（今天放行，随后 `VCall: expected object, got I64(42)`，不可 catch）

#### Scenario: 正确实例化的 is 为真
- **WHEN** `object o = new GBox<int>(42);`
- **THEN** `o is GBox<int>` 为真

#### Scenario: 身份成立后擦除名不得静默变假（阴性对照）
- **WHEN** 源码写 `x is GBox<int>`（实参齐全）
- **THEN** 结果为真 —— **不得**因为运行期类型变成 `GBox<int>` 而编成擦除名 `GBox` 后比失败

### Requirement: 实例化的继承字段带代换后的类型

**Before:** 泛型基类名被剥成裸名，运行期从擦除定义合并字段 ⇒ 型参字段的默认值是 `null`。

#### Scenario: 继承字段的零值
- **WHEN** `class GBox<T> { public T V; }` `class DInt : GBox<int> { }` `new DInt().V`
- **THEN** `0`（今天 `null`）

### Requirement: 静态字段按实例化分槽

#### Scenario: 两个实例化各自计数
- **WHEN** `GBox<int>` 构造两次、`GBox<string>` 构造两次，ctor 里 `Count = Count + 1`
- **THEN** 各自为 `2`（今天两边都读到 `4`）

### Requirement: 实例化的方法签名按实参代换

**Before:** `g.Get()` 的静态类型是未代换的 `T` ⇒ 调用方与 callee 都判不出返回 blob struct
⇒ 都不走 sret ⇒ 返回**已死帧的 arena 句柄**。

#### Scenario: 返回型参值的方法
- **WHEN** `class G<T>{ public T V; public T Get(){return this.V;} }`，`G<P2> g = new G<P2>(p);`
- **THEN** `g.Get().X == 42`（今天 `struct-value handle used after its creating frame exited`）

#### Scenario: 形参位同样代换
- **WHEN** 泛型方法以型参为形参类型，实参是 blob struct
- **THEN** 传参按值语义，callee 修改不影响调用方

### Requirement: 闭合泛型可在源码层书写

#### Scenario: 转换到闭合泛型
- **WHEN** 源码写 `(GBox<string>)o`
- **THEN** 解析通过（今天 `E0202: expected ')'`）

#### Scenario: 闭合泛型上的静态成员
- **WHEN** 源码写 `GBox<int>.Count`
- **THEN** 解析通过（今天 `E0202` + 级联 `E0401`）

## IR Mapping

不新增 IR 指令。变化在**元数据与名字**：

- TYPE 段：泛型 class 的实例化获得**完整**描述符（基类链 / 接口 / 代换后的字段 / 静态字段）。
  ⭐ **vtable 不在 TYPE 段**（运行期从 `own_methods` + 基链 merge），不需要合成。
- `is_instance` / `as_cast` 的目标名：擦除名 → **实例化规范名**。
- 静态字段键：`<裸名>.<字段>` → 带实例化的键。⚠️ **自举敏感**，走分阶段引入。
- 返回 blob struct 的方法：两侧改走既有 **sret** 约定（`METHOD_FLAG_SRET`），无新指令。

## Pipeline Steps

- [x] Parser / AST — cast 与成员访问两处前瞻改回溯式
- [x] TypeChecker — `is`/`as` 保留 `NamedType.Args`；`_substGenericSig` 铺到一般实例方法调用
- [x] IR Codegen — 完整实例化描述符；静态字段键；sret 两侧对齐
- [ ] VM interp — 预期零改动（运行期本就按原样字符串名解析）；若 `is`/`as` 需要新回落再评估
