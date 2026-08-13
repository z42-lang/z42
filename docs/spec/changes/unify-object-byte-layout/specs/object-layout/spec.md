# Spec: 统一对象 byte-offset 布局 + 8B 引用

## ADDED Requirements

### Requirement: class 直接字段统一为 byte-offset 布局

#### Scenario: 基元字段 byte-pack
- **WHEN** 定义 `class C { int x; long y; bool b }` 并访问 `c.x`/`c.y`/`c.b`
- **THEN** 三字段以 C 顺序布局在对象字节区（`x`@0 4B、`y`@8 8B、`b`@16 1B，含对齐），值语义读写正确；对象不再为每字段分配 24B `Value` 槽

#### Scenario: 引用字段 8B 内联
- **WHEN** 定义 `class Node { Node next; string name }` 并读写 `n.next`/`n.name`
- **THEN** `next`/`name` 各占对象字节区中的 **8B** 裸指针位置；读写得到正确对象/字符串；对象级引用位图标记这两个 offset 为引用

#### Scenario: 内联 struct 字段扁平嵌入（无死槽）
- **WHEN** 定义 `class C { Point pt; int tag }`（`Point` 为多字段 struct）
- **THEN** `pt` 的叶子扁平嵌入对象字节区，`tag` 紧随其后；对象**不**为 `pt` 保留任何未用的 `slots` 格（消除 P3b 双存储）

#### Scenario: 继承字段 offset 稳定
- **WHEN** `class B : A`，A 声明字段在前、B 追加
- **THEN** A 的字段在 B 实例里的 byte offset 与在 A 实例里一致（基→派生单调）

### Requirement: 引用压到 8B（路 A 标记指针，非移动 GC）

#### Scenario: GcRef 8B + generation 校验
- **WHEN** 分配对象得到句柄，正常访问
- **THEN** 句柄为 8B（48 位地址 + 16 位窄 generation）；deref mask 掉高位得真地址；generation 与 entry 不符（槽复用）时报明确 use-after-free 错误，不静默读脏

#### Scenario: Value enum = 16B
- **WHEN** 运行时以 `Value` 存寄存器 / 数组 boxed 元素
- **THEN** `size_of::<Value>()` == 16（tag 8 + payload 8）

### Requirement: 字符串 8B 细指针

#### Scenario: 字符串引用 8B
- **WHEN** `string s = "hello"; int n = s.Length`
- **THEN** `s` 引用为 8B 细指针，`Length` 从堆对象头读 len 正确；字符串内容/拼接/比较行为不变

### Requirement: GC 按对象级引用位图精确扫描

#### Scenario: 内联 8B 引用被正确 trace
- **WHEN** 对象字节区内联了引用字段，触发 GC mark
- **THEN** GC 按 `TypeDesc` 引用位图读每个引用 offset 的 8B 指针、按 kind 重建句柄并 mark；存活对象不被误回收，死对象被回收

#### Scenario: 写屏障按 byte-offset
- **WHEN** 向对象引用字段写入堆引用（generational 模式）
- **THEN** `write_barrier_field` 以 owner + byte-offset dirty 卡；collector 观察到写

### Requirement: 跨包 / 反射 / JIT 一致

#### Scenario: 跨包对象布局一致
- **WHEN** 消费方包引用生产方包定义的 class 并访问字段
- **THEN** 消费方按 zbc 布局表重算的 offset 与生产方逐字节一致

#### Scenario: 反射读写内联字段
- **WHEN** `FieldInfo.GetValue/SetValue` 读写基元/引用/内联 struct 字段
- **THEN** 走 byte-offset 访问，结果与直接字段访问一致

#### Scenario: JIT 与 interp 同结果
- **WHEN** 同一含对象字段访问的程序 `--mode jit` 与 interp 运行
- **THEN** 输出一致；JIT 按 16B Value STRIDE + byte-offset 字段访问

## MODIFIED Requirements

### Requirement: 对象 payload 表示（object-abi §3）
**Before:** 普通 ref 对象 payload = `slots: Value[]`，GC 逐 slot 看 tag。
**After:** payload = `bytes` C 顺序字节布局，基元自然宽度、引用 8B 内联、内联 struct 扁平嵌入；GC 按对象级引用位图精确扫。

## IR Mapping
- 对象直接字段访问：`FieldGet/FieldSet`（按名/slot）→ 统一到 `StructFieldGetPrim/SetPrim`（对象基址 + 编译期烘焙 byte-offset）；引用叶子内联 8B。
- zbc TYPE section：class 完整字段布局表（size + (offset,kind)×n 引用位图），取代/扩展 1.32 内联字段表。

## Pipeline Steps
- [x] Lexer —— 无
- [x] Parser / AST —— 无
- [ ] TypeChecker / 布局：`StructLayout` 对象全字段布局合成
- [ ] IR Codegen：`ExprEmitter` 字段访问统一 byte-offset
- [ ] 格式：zbc/zpkg writer/reader
- [ ] VM interp：字段访问 + GC 位图扫 + GcRef 8B + 字符串细指针 + Value 16B
- [ ] JIT：STRIDE 16 + byte-offset
