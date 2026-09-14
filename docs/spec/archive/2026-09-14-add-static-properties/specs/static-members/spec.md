# Spec: 静态属性、类内裸名静态成员、属性初始化器

> Capability：`static-members`。语义对标 C#。所有正例 interp 与 JIT 结果一致。

## ADDED Requirements

### Requirement R1: 静态 auto 属性

`static T P { get; set; }` / `static T P { get; }` 合成**静态**后备存储 `__prop_P`，访问器为 0 参 / 1 参静态函数。

#### Scenario: 读写
- **WHEN** `class St { public static int A { get; set; } }`，执行 `St.A = 7; Print(St.A);`
- **THEN** 输出 `7`

#### Scenario: 未赋值读默认值
- **WHEN** `public static string S { get; set; }`，首次读 `St.S`
- **THEN** 得 `null`（int 类型得 `0`）——与同类型静态字段一致

#### Scenario: 复合赋值与自增
- **WHEN** `St.A = 1; St.A += 5; St.A++; ++St.A; St.A--;`
- **THEN** `St.A == 7`；`St.A++` 表达式值为旧值、`++St.A` 为新值

### Requirement R2: 静态计算属性

#### Scenario: 三种写法等价
- **WHEN** `static int K => 5;`、`static int G { get { return 6; } }`、`static int H { get => 7; }`
- **THEN** `St.K == 5`、`St.G == 6`、`St.H == 7`

#### Scenario: 每次读取都重新计算
- **WHEN** `static int cnt = 0; static int Next => cnt + 1;`，`cnt = 3` 后读 `St.Next`
- **THEN** 得 `4`

#### Scenario: getter 体是静态上下文
- **WHEN** 静态计算 getter 体内引用 `this` 或实例字段
- **THEN** 编译报错（与静态方法体内同样写法的诊断一致），不再当实例上下文静默绑定

### Requirement R3: 属性初始化器

#### Scenario: 实例 auto 属性初始化器（有显式 ctor / 无 ctor / 主构造）
- **WHEN** `class C { public int P { get; set; } = 12; public string N { get; } = "nm"; }`，`new C()`
- **THEN** `P == 12`、`N == "nm"`；三种构造形态结果相同

#### Scenario: 与字段初始化器按声明序执行
- **WHEN** `int a = Log(1); int P { get; set; } = Log(2); int b = Log(3);`
- **THEN** 构造时副作用顺序为 `1, 2, 3`，且都早于用户 ctor 体

#### Scenario: 静态 auto 属性初始化器
- **WHEN** `static int A { get; set; } = 3;`（有 / 无静态 ctor 两种类）
- **THEN** 首次使用前 `St.A == 3`；有静态 ctor 时初始化器先于静态 ctor 体执行（静态 ctor 可覆写之）

#### Scenario: `this(...)` 委托 ctor 不重复执行初始化器
- **WHEN** `C() : this(0) { }` 与 `C(int x) { }`，类有属性初始化器
- **THEN** 初始化器副作用只发生一次（与字段初始化器同规则）

### Requirement R4: 只读与计算属性的赋值边界（E0452）

#### Scenario: 静态 get-only auto 属性在本类静态 ctor 内赋值（合法）
- **WHEN** `class St { static int R { get; } static St() { R = 9; } }`
- **THEN** 编译通过，`St.R == 9`

#### Scenario: 静态 get-only auto 属性在其他位置赋值（违规）
- **WHEN** 在静态方法 / 实例方法 / 他类中写 `St.R = 1` 或裸名 `R = 1`
- **THEN** 报 `E0452`，消息含 ``get-only property `R` ``

#### Scenario: 静态计算属性赋值（违规）
- **WHEN** `St.K = 1`（`K` 为计算属性）
- **THEN** 报 `E0452`，消息含 ``property `K`: it has no setter``

### Requirement R5: 类内裸名静态成员

在类的任何成员体内（实例/静态方法、实例/静态 ctor、属性 getter、索引器访问器），裸名 `x` 若不是局部 / 形参，
且是**本类**的静态字段或静态属性，则等价于 `C.x`。

#### Scenario: 实例方法读写静态字段
- **WHEN** `class St { static int cnt = 40; int G() { cnt = cnt + 1; return cnt; } }`，`new St().G()`
- **THEN** 得 `41`，且之后 `St.cnt == 41`（今天：读回 Null 抛运行期异常）

#### Scenario: 静态方法读写静态字段与属性
- **WHEN** `static int F() { cnt++; A += 2; return cnt + K + A; }`
- **THEN** 结果按 C# 语义计算（今天：`E0401`）

#### Scenario: 局部 / 形参遮蔽
- **WHEN** `static int cnt = 40; static int F(int cnt) { return cnt; }`，`St.F(1)`
- **THEN** 得 `1`

#### Scenario: const 字段裸名（实例方法 / 静态方法）
- **WHEN** `const int M = 3; int G() { return M; } static int F() { return M; }`
- **THEN** 两者都得 `3`，且发射为常量（与 `C.M` 同一折叠路径）
  （今天：实例方法读回 Null → `__box_prim: expected integer value, got Null`；静态方法 `E0401`）

### Requirement R6: 限定静态成员的复合赋值

#### Scenario: 静态字段 `+=`
- **WHEN** `St.cnt = 40; St.cnt += 2;`
- **THEN** `St.cnt == 42`（今天：`E0401 undefined: St`）

#### Scenario: 静态 event 语义不受影响
- **WHEN** 实例 `obj.E += h`
- **THEN** 仍绑定 `add_E`，行为与改动前一致

### Requirement R7: 跨包静态属性

#### Scenario: 读写另一个 zpkg 中类的静态属性
- **WHEN** 包 `target` 声明 `public class Cfg { public static int Level { get; set; } = 2; public static string Name => "cfg"; }`，
  包 `main` 执行 `Cfg.Level = 5; Print(Cfg.Level); Print(Cfg.Name);`
- **THEN** 输出 `5` 与 `cfg`

#### Scenario: 跨包只读静态属性赋值
- **WHEN** 包 `main` 写 `Cfg.Name = "x"`
- **THEN** 编译报 `E0452`

## 非目标（明确不支持，保持现有 loud 诊断）

- `Derived.baseStatic` 与派生类内裸名引用基类静态成员（`E0401`）
- 静态索引器、静态 event、计算 setter
