# Spec: 变长区参与分代回收

## MODIFIED Requirements

### Requirement: minor GC 的回收范围

**Before:** minor GC 只扫 `region_object` / `region_array` 的 young 条目；`region_var`
的块无论年龄，一律要等到 major 才被回收。

**After:** minor GC 同时扫三个 region 的 young 成员。变长块与定长槽走同一套
「标记 → 存活则 `gen_age + 1`（达阈值则移出 young 集合）→ 未标记则终结 + tombstone」流程。

#### Scenario: 年轻的变长块在 minor 里被回收
- **WHEN** 分配一个 `Str` 块，随后它变为不可达，触发一次 minor GC
- **THEN** 该块被 tombstone，其槽进入对应 size class 的 free list，
  且 `region_var` 的 `used_bytes` 相应下降

#### Scenario: 存活的变长块在 minor 里晋升而非回收
- **WHEN** 一个变长块被根引用着，连续经历 `PROMOTION_THRESHOLD` 次 minor GC
- **THEN** 每次 minor 后它仍然存活、`gen_age` 递增；达到阈值时移出 young 集合，
  此后的 minor 不再访问它

#### Scenario: 老的变长块不被 minor 访问
- **WHEN** 一个已晋升的变长块在后续 minor 中变为不可达
- **THEN** 该 minor **不**回收它（这是分代的定义），它在下一次 major 被回收

#### Scenario: 数组头与其元素存储在同一次 minor 中一起回收
- **WHEN** 一个年轻的 `Value[]` 变为不可达并触发 minor
- **THEN** `region_array` 的头槽与 `region_var` 的 `ArrayValue` 元素块**在同一次 minor**
  内都被回收（今天只有头被回收，元素块留到 major）

### Requirement: minor GC 的 freed_bytes 口径

**Before:** minor 回收数组头时把 `array_size_estimate`（含 `elem_storage_bytes()`）
计入 `freed_bytes`，但元素存储所在的变长块并未被这一轮回收——账面回收量高于实际。

**After:** `freed_bytes` 只计入本轮**实际**回收的字节。元素存储的字节由变长区自己的
sweep 在同一轮里如实计入，不再由数组头代记。

#### Scenario: 账面回收量不超过实际内存下降
- **WHEN** 任意一次 minor GC 结束
- **THEN** 该次上报的 `freed_bytes` ≤ (回收前 `used_bytes` − 回收后 `used_bytes`)

#### Scenario: 预算闸门读到真实值
- **WHEN** 设了 `Z42_GC_MAX_BYTES` 且分代模式下连续触发多次 minor
- **THEN** `used_bytes` 的实测下降与累计 `freed_bytes` 一致，
  自动回收不会因为虚高的回收量而推迟下一次触发

## ADDED Requirements

### Requirement: 变长块携带 gen_age

#### Scenario: 块头大小不变
- **WHEN** 编译运行时
- **THEN** `size_of::<GcBlockHeader>() == 16` 的静态断言仍然成立
  （`gen_age` 打包进 `type_tag` 的空闲高位，不新增字段）

#### Scenario: 新分配的块是年轻的
- **WHEN** 通过任一路径分配变长块（`VarRegion::alloc` 的 bump / free-list 复用路径、
  `VarChunkClaim::fill` 的 TLAB 路径、`VarGcRef::alloc_leaked` / `leak_block_for_test`）
- **THEN** 该块的 `gen_age == 0`，且被登记进 young 集合

#### Scenario: block_type 与 gen_age 互不干扰
- **WHEN** 任意 `BlockType` 变体与任意 `gen_age`（0..=PROMOTION_THRESHOLD）组合
- **THEN** 两者都能无损读回；未知位模式仍走 `BlockType::from_u8` 的损坏保护路径

### Requirement: minor 不得在变长块上留下陈旧的 mark 位

一个块若带着上一轮 minor 的 mark 位进入下一轮，它的 `mark()` CAS 会失败，
追踪逻辑据此判定「已访问」而**跳过它的子节点**。对 `Closure`（唯一自身是变长块又有出边的类型）
这意味着它引用的对象不再被标记 —— 会被当作垃圾回收掉。

#### Scenario: 闭包的 env 连续多轮 minor 后仍存活
- **WHEN** 一个 `env` 数组**只**经由闭包块可达，闭包被 pin，连续触发
  `PROMOTION_THRESHOLD + 2` 次 minor GC
- **THEN** 该 `env` 数组每一轮之后都仍然存活
  （修复前在**第 2 轮**被释放 —— 第 1 轮设下的 mark 位在第 2 轮抑制了追踪）

#### Scenario: 存活的变长块在 sweep 后 mark 位被清
- **WHEN** 一个年轻变长块被标记并熬过一次 `sweep_young`
- **THEN** 对它重新发起 mark CAS 必须能成功（说明位已被清）

### Requirement: gen_age_of 认得变长块的年龄

**Before:** `Value::Str` / `Value::FuncRef` 落到 `_ => 0`，恒被当作年轻；
`Value::Closure` 报的是它 `env` 数组的年龄，而不是闭包块自己的。

**After:** 三者都报所属变长块的真实 `gen_age`。

#### Scenario: 老字符串不再被 minor 反复重标
- **WHEN** 一个被 pin 的字符串熬过 `PROMOTION_THRESHOLD` 次 minor 后再触发一次 minor
- **THEN** 它的 `gen_age` 停在 `PROMOTION_THRESHOLD` 不再增长（minor 已不访问它）

> **不做卡表**：变长块不产生跨代写 —— `Str` / `ArrayPrim` 是叶子，
> `ArrayValue` / `ArrayStruct` 只经 `Value::Array` owner 写入（已被 `region_array` 的卡覆盖），
> `ClosureData` 创建后不可变。老数组头经脏卡重新入根后，
> `trace_children` 的 `mark_backing()` 会标记其元素块，覆盖已经完整。

## Pipeline Steps

不涉及编译期 pipeline（纯 VM 运行期变更）：

- [ ] Lexer
- [ ] Parser / AST
- [ ] TypeChecker
- [ ] IR Codegen
- [x] VM runtime（`gc/var_region*` + `gc/arc_heap/generational.rs`）
