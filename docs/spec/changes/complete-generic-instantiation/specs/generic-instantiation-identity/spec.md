# Spec: 泛型实例化的身份与跨包特化

## MODIFIED Requirements

### Requirement: 跨包泛型实例化的值语义

**Before:** 泛型定义在依赖包里时，实例化一律不特化、不取独立身份。型参槽按 8 字节句柄存，
外层复制只浅拷句柄 ⇒ 存进去的 struct 与源变量共享同一块 blob。

**After:** 定义是**成员全部由编译器合成的 `[Record] struct`** 时，消费方按实例化布局特化该
实例化（型参字段变真内联字节）并合成其成员。其余跨包实例化**退回原表示，逐字不变**。

#### Scenario: 元组存入后改源变量（形态 ①）
- **WHEN** `P2 a = new P2(1,2L); (P2,int) t = (a,7); a.Y = 99L;`
- **THEN** `t.Item1.Y == 2L`（今天读到 99）

#### Scenario: 复制元组后经副本写穿（形态 ③）
- **WHEN** `(P2,int) t2 = t; t2.Item1.Y = 55L;`
- **THEN** `t.Item1.Y` 不变，`t2.Item1.Y == 55L`

#### Scenario: 元组作实参传递
- **WHEN** 把 `(P2,int)` 传给一个会改 `v.Item1.Y` 的函数
- **THEN** 调用方的原值不受影响

#### Scenario: 基元元组行为不回归（阴性对照）
- **WHEN** `(int,string) t = (1,"a"); (int,string) u = t; u.Item1 = 99;`
- **THEN** `t.Item1 == 1`（今天已正确，改后仍正确）

#### Scenario: 带用户方法体的跨包泛型不命中（闸门阴性对照）
- **WHEN** 依赖包导出一个带用户方法的泛型 struct，消费方实例化它
- **THEN** 不特化、不取独立身份，编译产物与改动前**逐字节相同**

### Requirement: 合成实例化产物的重复到达不是歧义

同一个实例化会被**每个用到它的包**各合成一份描述符与成员。两份是 `(定义, 类型实参)` 的
确定性函数，逐字节相同 ⇒ 第二份到达时应被静默跳过，而非记为「两个包声明了同一个名字」。

#### Scenario: 库与主程序各用一次同一元组
- **WHEN** 库 `X` 内部使用 `(int,string)`，主程序也使用 `(int,string)`
- **THEN** 程序正常运行；构造与调用均不抛；stderr 无 `duplicate type` / `duplicate function` 告警

#### Scenario: 两个互不依赖的库各用一次同一元组
- **WHEN** 包 X 与包 Y 各自使用 `(int,string)`，同一程序同时依赖两者
- **THEN** 同上

#### Scenario: 真正的用户声明重复仍然报（阴性对照）
- **WHEN** 两个包各声明同一个 FQN 的**非泛型**类型，消费方引用它
- **THEN** `E0601` 照报、运行期歧义行为照旧 —— D4-fix 不得放宽这条

### Requirement: 泛型 class 实例化的独立身份

> P2 范围。开工前回到阶段 4 补齐场景。

**Before:** `GBox<int>` 与 `GBox<string>` 在运行期是同一类型——静态字段共享一槽、
`is`/`as` 跨实例化为真、`GetType().Name` 都报 `GBox`。

**After:** 每个具体实例化是独立类型（对齐 C#）。

#### Scenario: 静态字段按实例化分离
- **WHEN** `GBox<int>` 构造两次、`GBox<string>` 构造两次，构造器里 `Count = Count + 1`
- **THEN** `GBox<int>.Count == 2` 且 `GBox<string>.Count == 2`（今天两边都读到 4）

#### Scenario: 类型测试区分实例化
- **WHEN** `object o = new GBox<int>(42);`
- **THEN** `o is GBox<int>` 为真，`o is GBox<string>` 为**假**（今天为真）

#### Scenario: 错误实例化的转换被拒
- **WHEN** `object o = new GBox<int>(42); GBox<string> b = o as GBox<string>;`
- **THEN** `b == null`（今天放行原值，随后崩在不可 catch 的 `VCall: expected object, got I64(42)`）

#### Scenario: 转换到闭合泛型可被书写
- **WHEN** 源码写 `(GBox<string>)o`
- **THEN** 解析通过（今天 `E0202: expected ')'`），语义等同 C# 的硬转换

#### Scenario: 闭合泛型上的静态成员可被书写
- **WHEN** 源码写 `GBox<int>.Count`
- **THEN** 解析通过（今天 `E0202: expected ')'` + 级联 `E0401`）

## IR Mapping

P1 不新增任何 IR 指令。命中闸门的跨包实例化，发射从

```
obj_new <擦除名>          →   struct_alloc <实例化 FQ 名> [N B]
field_get %o.Item1        →   struct_fget_prim %s @<实例化偏移>
```

即改为走与本包实例化**完全相同**的既有指令。

**元数据位**：新增 `METHOD_FLAG_SYNTHESIZED = 1 << 4`（SIGS 的 `method_flags: u8`，
bit4–7 本就空闲）。**零 zbc / zpkg 格式 bump**——旧读端按 u8 读、忽略不认的位；
旧写端打 0 ⇒ 消费方判否 ⇒ 退回原行为。

## Pipeline Steps

受影响的 pipeline 阶段：

- [ ] Lexer — 无
- [x] Parser / AST — **仅 P2**（cast 与成员访问两处前瞻改回溯式）
- [x] TypeChecker — P1：导入类携 `IsRecord`；P2：`is`/`as` 保留 `NamedType.Args`
- [x] IR Codegen — P1：闸门判据 + 反造合成 decl；P2：完整实例化描述符 + 静态字段键
- [x] VM interp — P1：**惰性加载器的重复登记判定**（D4-fix，见 design.md）。
      解析路径本身零改动（本就按原样字符串名）。P2 待定
