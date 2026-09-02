# Spec: 泛型数组值类型零初始化（方案 C）

## MODIFIED Requirements

### Requirement: 泛型 `new T[n]` 按元素类型零初始化

**Before:** 泛型方法/类里 `new T[n]`，当 T 绑**基元值类型**（int/bool/char/double/…）时，未写入槽为
`Value::Null`；读该槽（触发装箱）抛 `__box_prim: expected integer value, got Null`。

**After:** 未写入槽为该基元值类型的零值（int→0，bool→false，char→'\0'，double→0.0）。读未写槽不再报错。

**范围（narrow）：** 只影响**基元值类型**类型参数——它们才是 `__box_prim` bug 现场。**值 struct / 引用类型**
类型参数**保持原擦除路径不变**（reference-backed、Null 默认）：泛型容器按引用/装箱存 struct（见
`struct_generic_container`：`Dictionary<P,int>`/`List<P>`），若把已解析的 struct 强制走 struct-backing 会
`VCall: expected object, got StructRefHeap`。数组 backing 与 `element_type` 保持擦除不变，仅基元槽默认值改变
（复刻已验证的 `default(T)` 赋值行为）。

#### Scenario: 泛型方法返回值类型数组，读未写槽
- **WHEN** `T[] make<T>(int n) => new T[n];`，调 `make<int>(3)`，读 `result[0]`（未写）传给取 `object` 形参的方法（触发装箱）
- **THEN** 得 `0`，不抛 `__box_prim`

#### Scenario: 各值类型零值
- **WHEN** `make<bool>(2)[0]` / `make<char>(2)[0]` / `make<double>(2)[0]`
- **THEN** 分别 `false` / `'\0'` / `0.0`

#### Scenario: 泛型类字段 `new T[n]`（类级类型参数）
- **WHEN** 泛型类 `Box<T>` 内 `T[] buf = new T[n];`，`Box<int>` 读未写 `buf[0]`
- **THEN** 得 `0`（操作数 kind=2 → 接收者 `Object.type_args[idx]`）
- **注**：若首版仅落 method 级（design Decision 5），本 scenario 归后续；格式已预留 kind=2。

#### Scenario: 引用类型元素不回归
- **WHEN** `make<string>(2)`，读未写 `result[0]`
- **THEN** 得 `null`

#### Scenario: 值 struct 泛型容器不回归
- **WHEN** `Dictionary<P,int>` / `List<P>`（P 为值 struct）的存取 / 遍历 / 按值查找（`struct_generic_container`）
- **THEN** 与修改前逐一致（struct 仍 reference-backed / 装箱，不被强制 struct-backing）

### Requirement: 非泛型数组编码/行为不变（自举字节安全）

#### Scenario: `new int[n]` / `new C[n]` 逐字节不变
- **WHEN** 元素为具体类型或非泛型擦除（`TypeParamKind=0`）
- **THEN** array_new 走原路径；zbc ArrayNew 编码除新增 `kind=0 + index=-1` 尾字段外语义不变；执行结果不变

### Requirement: Array.Resize 去绕过后仍正确

#### Scenario: Resize 扩容尾部为零值
- **WHEN** `Array.Resize<int>(new int[]{1,2,3}, 5)`（内部去掉显式填尾后）
- **THEN** `result[3]==0 && result[4]==0`

## IR / Format Mapping

- `ArrayNewInstr` 新增 `TypeParamKind: u8`（0=none/1=method/2=class）+ `TypeParamIndex`（kind=0 时 -1）。
  泛型形参元素 emit (kind, idx)；非泛型元素 emit (0, -1)。`ElemTag` 仍 Unknown（VM 以操作数为准）。
- zbc writer/reader 对称：ArrayNew opcode 尾部加 `kind(u8)` + `index(varint)`。**zbc 1.36→1.37 / zpkg 0.41→0.42**。
- Rust strict-pin：`ZBC_VERSION_MINOR 36→37` / `ZPKG_VERSION_MINOR 41→42`。
- 格式 bump 走 ci-bootstrap 两代自举自动吸收（回归已修，#383/#385）。

## Pipeline Steps

- [ ] Parser / AST — 不涉及（语法不变）
- [x] TypeChecker — `ExprTyper._bindArrayNew`：泛型形参解析 (kind, ParamIndex)
- [x] IR Codegen — `ExprEmitter` ArrayNew emit 新操作数
- [x] zbc writer/reader — 对称加字段 + 版本 bump
- [x] VM interp — `exec_array.rs array_new`：kind!=0 → type_args → 具体零值+backing+element_type
