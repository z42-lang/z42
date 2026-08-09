# Spec: struct 值语义（内联/栈布局，选项 B）

> 状态：🟡 DRAFT — 选项 B + 3a。场景按最终目标语义列全；右侧 **[Pn]** 标该场景在分阶段程序里的
> 落地阶段（P1 局部/传参/返回；P3 对象内联/数组；P4 跨包/装箱）。

## ADDED Requirements

### Requirement: struct 内联存储、无独立堆身份

#### Scenario: 局部 struct 不产生堆对象 [P1]
- **WHEN** 声明并使用一个未逃逸的 struct 局部（不装箱、不存入引用容器）
- **THEN** 不分配 GC 托管对象（无 `Value::Object` 身份）；数据内联在栈帧寄存器区间，随帧退出释放

#### Scenario: struct 数组字节扁平内联 [P3]
- **WHEN** `struct[]` 长度 n、struct 字节 size s
- **THEN** backing 为 `n*s` 字节扁平区（无每元素堆对象、无每叶子 24B Value 开销）；
  `arr[i].f` 走字节 `i*s + byte_offset(f)`；内存密度逼近 C#

### Requirement: struct 赋值复制

#### Scenario: 局部整体赋值独立 [P1]
- **WHEN** `var a=P(); a.x=1; var b=a; b.x=99;`
- **THEN** `a.x==1`（`b` 独立副本）

#### Scenario: 引用类型字段随 struct 复制共享引用 [P1]
- **WHEN** struct 含 class 字段 `c`，`var b=a;`
- **THEN** `b.c===a.c`（引用叶子复制引用），但值叶子独立——与 C# 字段级复制一致

### Requirement: struct 传参 / 返回复制

#### Scenario: 传参是副本 [P1]
- **WHEN** `fn mut(s:P){ s.x=99; }`，`var a=P(); a.x=1; mut(a);`
- **THEN** `a.x==1`（形参副本，改动不回传）

#### Scenario: 返回是副本 [P1]
- **WHEN** `fn make():P{ var s=P(); s.x=7; return s; }`
- **THEN** 每次 `make()` 结果独立，改一个不影响另一次

### Requirement: struct 存入 / 取出容器复制

#### Scenario: 存入 class 字段是副本 [P3]
- **WHEN** `obj.pt=s; s.x=99;`（pt 是 struct 字段）
- **THEN** `obj.pt.x==1`

#### Scenario: 存入数组元素是副本 [P3]
- **WHEN** `arr[0]=s; s.x=99;`
- **THEN** `arr[0].x==1`

#### Scenario: 从容器取出是副本 [P3]
- **WHEN** `var t=obj.pt; t.x=99;`
- **THEN** `obj.pt.x` 不变

### Requirement: 穿过 struct 字段的原地修改（3a）

#### Scenario: 局部嵌套 struct 字段原地改 [P1]
- **WHEN** `struct Line{var a:P; var b:P;}`，`line.a.x=3;`
- **THEN** `line.a.x==3`，`line.b` 不受影响（叶子地址直写）

#### Scenario: class 的 struct 字段原地改 [P3]
- **WHEN** `obj.pt.x=5;`
- **THEN** `obj.pt.x==5`（原地写父对象 slots）；`obj2.pt.x` 不受影响

#### Scenario: 数组元素的 struct 字段原地改 [P3]
- **WHEN** `arr[i].x=5;`
- **THEN** `arr[i].x==5`；`arr[j].x`（j≠i）不受影响

### Requirement: struct 默认值

#### Scenario: 未初始化 struct 局部为全字段默认（非 null） [P1]
- **WHEN** `var s:P;` 后读 `s.x`
- **THEN** `s.x==0`（内联区间零初始化，各叶子默认），非 null 解引用错误

#### Scenario: 自含值字段的 struct 编译期报错 [P1]
- **WHEN** struct 直接或间接含自身**值**字段（无限大小）
- **THEN** 编译期报错（类 C# CS0523），非运行时栈溢出

### Requirement: struct 装箱与反射一致

#### Scenario: struct 装箱到 object 拷进堆并保留精确类型 [P4]
- **WHEN** `var o:object = s;`（s 是 P）
- **THEN** `o` 是堆 boxing 副本，`o is P` 真、`o.GetType()` 返回 `P`、`Type.IsValueType` 真；
  改 `s` 不影响 `o`（装箱是复制）

### Requirement: struct 引用类型叶子的 GC 正确性（读写屏障 + 根扫描）

#### Scenario: 局部 struct 里的引用字段保持存活 [P1]
- **WHEN** 局部 struct 含引用类型叶子（如 `struct Box { var items: List; }`），`items` 指向堆对象，
  之后触发 GC
- **THEN** 该堆对象**不被误回收**——GC 借引用位图定位 blob 内 `items` 叶子并当根扫描

#### Scenario: blob 复制含引用叶子时发写屏障 [P1]
- **WHEN** `var b = a;`（a 是含引用叶子的 struct）→ StructCopy 复制 blob
- **THEN** 目标 blob 的每个引用叶子写都过写屏障（分代/并发后端不漏写）；复制后 `b.items === a.items`

#### Scenario: 向 struct 引用叶子写引用发写屏障 [P1]
- **WHEN** `s.items = newList;`（s 是局部 struct，items 是引用叶子）
- **THEN** 原地写 blob 引用叶子 + 触发写屏障

#### Scenario: 无引用叶子的 struct 走纯 memcpy 快路径 [P1]
- **WHEN** struct 全为基元叶子（引用位图为空），复制/传参
- **THEN** 走纯字节 memcpy（不发屏障），性能不被屏障拖累

### Requirement: struct 值相等

#### Scenario: 逐字段值相等 [P1]
- **WHEN** 两 struct 各叶子值相同，`a==b`
- **THEN** 真（逐字段值相等，非引用相等）；任一叶子不同则假

## MODIFIED Requirements

### Requirement: struct 实例的运行时表示

**Before:** struct 与 class 同构，均 `Value::Object(GcRef<ScriptObject>)`，堆分配 + 引用语义，
`var b=a` 共享句柄。

**After:** struct 为**内联值类型**——未装箱时以容器内**连续槽区间**存在（栈帧寄存器 / 父对象 slots /
数组扁平 backing），无独立堆身份、无 GC 托管；赋值/传参/返回/存容器 = 区间复制（字段级）；可寻址位置
支持原地可变；`struct→object` 在装箱点拷进堆 `ScriptObject`（CLR boxing）。class 仍为引用语义不变。

## IR Mapping

- **StructLayout**（编译期）：每 struct 类型 → `{size, align, field_layout(byte_offset/size/kind), 引用位图}`，
  嵌套递归展平 + 对齐。
- **新 struct-aware 指令**（Decision α）：字节区间复制 `StructCopy`、字段字节区间/基元
  `StructFieldGet/Set(Prim)`、box/unbox。→ **zbc minor bump**（version-bumping.md）。
- **zpkg TypeDesc**：承载 struct 字节布局 + 引用位图供跨包消费（P4）→ **zpkg minor bump**。
- 两阶段 nightly 纪律（bootstrap-seed.md）：support 先行，z42c/stdlib 源晚一 nightly 才使用。

## Pipeline Steps

- [ ] Lexer —— 不涉（`struct` 已存在）
- [ ] Parser / AST —— 不涉
- [x] TypeChecker —— 值语义规则、自含值字段报错、原地可变合法性（可寻址 lvalue）
- [x] IR Codegen —— StructLayout、区间分配、区间复制、lvalue 叶子地址、box/unbox
- [x] VM interp —— struct 指令 dispatch、区间复制/搬运、内联对象/数组布局、装箱、默认值、相等、GC 覆盖
- [ ] JIT —— interp 全绿后再评估（interp 优先）
