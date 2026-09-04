# Spec: 类初始化时机（class-initialization）

本变更的 delta。归档时上浮到 `docs/book/src/runtime/`。

## ADDED: 按需类初始化

### Scenario: 未被引用的包，其初始化器不执行

- **GIVEN** libs 目录下存在包 `z42.json`，其含 `Std.Json.<CU>.__static_init__`
- **AND** 程序 `hello.z42` 只调用 `Console.WriteLine`，不引用 `Std.Json` 的任何符号
- **WHEN** 运行该程序
- **THEN** `z42.json.zpkg` 不被加载
- **AND** `Std.Json.<CU>.__static_init__` 不被执行
- **AND** 程序输出与变更前一致

### Scenario: 首次调用包内函数时触发该包初始化

- **GIVEN** 程序引用 `Std.Regex.Regex.Compile`
- **WHEN** 执行到该调用
- **THEN** `z42.regex.zpkg` 在此时被加载
- **AND** 该包的全部 `*.__static_init__` 在该调用**返回之前**执行完毕
- **AND** 调用结果与变更前一致

### Scenario: 静态字段引用触发所属包初始化

- **GIVEN** 包 `DemoGTarget` 定义 `Store.items`，由其 `__static_init__` 赋值为空列表
- **AND** 主模块读取 `Store.items` 但不调用该包任何函数
- **WHEN** 运行主模块
- **THEN** 读取 `Store.items` 得到初始化后的空列表（**不是 `null`**）
- **AND** 对应 cross-zpkg golden `generic_field_carry` 输出不变

### Scenario: 初始化器互相引用不死锁

- **GIVEN** 包 A 的初始化器读取包 B 的静态字段，包 B 的初始化器读取包 A 的静态字段
- **WHEN** 首次触达 A
- **THEN** 不死锁、不栈溢出
- **AND** B 的初始化器观察到 A 的部分初始化状态（与 CLR 循环类型初始化器一致）

### Scenario: 每个包最多初始化一次

- **GIVEN** 某包的初始化器带可观察副作用（写静态计数器）
- **WHEN** 程序多次触达该包的不同符号
- **THEN** 该初始化器只执行一次

### Scenario: 并发首次触达

- **GIVEN** 两个 `VmContext` 线程同时首次调用同一未加载包的函数
- **WHEN** 两者并发进入解析
- **THEN** 该包的初始化器只执行一次
- **AND** 两个线程都在初始化完成之后才观察到该包的静态字段

## MODIFIED: 启动阶段行为

### Scenario: 启动不再加载全部候选包

- **GIVEN** 一个只用到 `Console.WriteLine` 的程序
- **WHEN** 运行
- **THEN** 加载的 zpkg 数量 ≤ 2（`z42.core` + 入口自身）
- **AND** 启动墙钟相对变更前至少快 1.8×（同机 hyperfine ≥ 50 runs）
- **AND** peak RSS 相对变更前至少降低 40%

### Scenario: 根命名空间不参与候选路由

- **GIVEN** 入口的 IMPT 含 `["Std", "Std.IO", "Std.IO.Console"]`
- **WHEN** 构建候选包集合
- **THEN** 单段的根命名空间 `Std` 不产生候选
- **AND** 候选来自 `Std.IO` / `Std.IO.Console` 的精确匹配

### Scenario: 原生类型名不触发全量扫描

- **GIVEN** 运行时解析类型名 `int`
- **WHEN** 该名字不在 type registry 中
- **THEN** 直接返回未找到，不加载任何 zpkg

## MODIFIED: REPL

### Scenario: REPL 跨轮引用新包

- **GIVEN** REPL 已启动，`z42.json` 尚未加载
- **WHEN** 用户输入一个引用 `Std.Json` 的表达式
- **THEN** 该轮求值成功，结果与变更前一致
- **AND** `z42.json` 的初始化器在该轮求值中执行

### Scenario: REPL 跨轮读取未加载包的静态字段

- **GIVEN** REPL 已启动，某包尚未加载
- **WHEN** 用户输入读取该包静态字段的表达式
- **THEN** 读到初始化后的值，不是 `null`

### Scenario: REPL 静态状态跨轮延续

- **GIVEN** 用户在第 N 轮修改了某静态字段的值
- **WHEN** 第 N+k 轮触发了其它包的首次加载与初始化
- **THEN** 第 N 轮改过的静态值不被重置
