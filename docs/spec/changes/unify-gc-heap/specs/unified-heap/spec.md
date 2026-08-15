# Spec: 统一 GC 堆模型（变长 GC 分配器 A'）

## ADDED Requirements

### Requirement: 变长 GC 块分配器（A'）

#### Scenario: 分配任意字节大小的 GC 块
- **WHEN** 运行时请求分配 N 字节 payload 的 GC 块（string 字节 / array 元素 / closure 数据）
- **THEN** 分配器返回一个 8B `GcRef` 指向单个变长块 `{GcBlockHeader, inline payload[N]}`，**单次分配、数据内联**（不是头一处+数据另一处的两次分配）；块头对齐 8

#### Scenario: 块头携带 GC 元数据 + 自描述
- **WHEN** 一个变长块被分配
- **THEN** 其 `GcBlockHeader` 至少含 mark 位、alive 位、generation、payload size、type_tag（Str/Array/Closure），使 sweep 能推进/回收、trace 能按 type_tag 决定如何扫 payload；**不含** `Mutex`/finalizer/soft_ref 等定长 `RegionEntry` 的重字段（叶子块不需要）

#### Scenario: 块地址稳定供身份
- **WHEN** 块被分配后、未被 sweep tombstone 前
- **THEN** 块头地址稳定不变（字节 chunk Box-owned 不搬迁），`GcRef` 身份等价（`ptr_eq`）成立；A' **不做移动/压缩**（对象不搬迁）

#### Scenario: free-list 按 size-class 复用
- **WHEN** 一个变长块被 sweep tombstone 后，随后请求分配同 size-class 的块
- **THEN** 分配器优先复用被回收的槽（size-class free-list），控制碎片；generation 在 tombstone 时 bump，旧 `GcRef` 的窄 generation 快照与之不符 → 拒绝（ABA 防护，同定长 region）

#### Scenario: mark/sweep 集成变长块
- **WHEN** GC 周期运行
- **THEN** mark 阶段从 roots BFS，遇 `GcRef` 指变长块按 type_tag 扫（array<Value>/closure 递归子引用；string/array<prim> 叶子终止）；sweep 回收未 mark 块（tombstone + 入 size-class free-list）；变长 region 与定长 `region_object`/`region_array` **并存**、统一由同一 GC 驱动

### Requirement: string 收进 GC 堆

#### Scenario: Value::Str 迁 GcRef、删 Arc 计数
- **WHEN** 程序创建/传递/丢弃字符串
- **THEN** `Value::Str` 承载 GC 细指针（仍 8B）指向 GC string 块（`{GcBlockHeader, len, UTF-8 bytes}`）；string 的生死由 GC 管（不再原子引用计数即时释放）；`len()`/内容/`==`（按内容）语义不变

#### Scenario: interned 串池是 GC roots
- **WHEN** 加载模块的 interned 常量串在整个进程存活
- **THEN** interned 串池注册为 GC roots，不被回收；const-str 访问是 `GcRef` clone（O(1)，比 Arc fetch_add 更省）

#### Scenario: string 不可变叶子无引用边
- **WHEN** GC trace 到一个 string 块
- **THEN** 按 type_tag=Str 识别为叶子、不递归（string 从不引用其它堆对象）

### Requirement: delegate/closure 收进 GC 堆

#### Scenario: Closure 本体进 GC
- **WHEN** 创建一个逃逸的 closure（`Value::Closure`）
- **THEN** `ClosureData`（含 `fn_name`、`env`）进 GC 变长块，`Value::Closure` 承载 `GcRef`；trace 扫 `env` 边（已 GC）+ `fn_name`（GC string）；`Box<ClosureData>` 外部分配被消除

#### Scenario: StackClosure 仍栈分配
- **WHEN** 逃逸分析判定 closure 不逃逸
- **THEN** 它仍是 `StackClosure`（帧作用域栈 arena），**不进 GC**——与布局程序 `StackObject`/`StackArray` 同规则

### Requirement: array backing 收进 GC 堆

#### Scenario: 引用数组元素在 GC
- **WHEN** 分配 `Value[]`（`ArrayBacking::Boxed`）
- **THEN** 元素缓冲区进 GC 变长块（不再外部 `Vec`），GC 直接逐元素 trace

#### Scenario: packed 基元数组是叶子块
- **WHEN** 分配 `int[]`/`byte[]`/`double[]` 等 packed 基元数组
- **THEN** 元素字节进 GC 变长块、type_tag 标叶子，GC 扫时跳过（无引用边）；值语义/长度不变

#### Scenario: struct[] 内联元素按引用位图扫
- **WHEN** 分配 `struct[]`（`StructBytes` backing）
- **THEN** 内联 struct 字节进 GC 变长块，引用叶子按 struct 引用位图 trace，基元叶子跳过

### Requirement: 单一堆收敛

#### Scenario: 无 Arc/Box/外部 Vec 双路径残留
- **WHEN** 程序运行任意 workload
- **THEN** 所有托管变长数据（string/closure/array 元素）都经同一 GC 分配器；不存在「一半 GC、一半 Arc/Box/Vec」的双重管理；mark/sweep/write-barrier/内存统计口径统一

#### Scenario: 运行时表示变、序列化格式不变
- **WHEN** 编译产物 zbc/zpkg 加载
- **THEN** string/closure/array 的**运行时表示**改变不影响 zbc/zpkg **序列化格式**（无 version bump、无 fixture 重生、自举字节不动）

## Non-Goals（本 spec 不覆盖）

- **移动/压缩 GC**：块地址稳定、不搬迁；forwarding/card-table 压缩是后续独立程序。
- **B 方向**（废类型化 region、全变长单一堆）：A' 达成统一堆实质后另议。
- **string 去重/interning 优化**、FFI marshal 对 GC string/array 的直传：后续。
