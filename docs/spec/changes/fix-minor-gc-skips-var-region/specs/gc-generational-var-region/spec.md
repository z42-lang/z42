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

### Requirement: 变长区的跨代写屏障

老变长块（如一个已晋升的 `ArrayValue`）写入一个年轻对象的引用时，必须留下记录，
否则该年轻对象在 minor 中无根可达 → 被误回收。

#### Scenario: 老变长块写入年轻引用会脏卡
- **WHEN** 一个 `gen_age >= PROMOTION_THRESHOLD` 的 `ArrayValue` 块的某个元素
  被写入一个 `gen_age == 0` 的对象引用
- **THEN** 该块所在 `VarRegion` chunk 的卡位被置脏

#### Scenario: 脏卡是 minor 的根
- **WHEN** 上一场景之后触发 minor GC
- **THEN** 脏 chunk 里的存活块被当作额外根扫描，那个年轻对象被标记存活、不被回收

#### Scenario: 叶子块不需要卡
- **WHEN** 写入的 owner 是 `Str` 或 `ArrayPrim` 块
- **THEN** 不脏卡（这两类没有出边，物理上不可能持有跨代引用）

#### Scenario: major 清卡
- **WHEN** 一次 major GC 结束
- **THEN** `region_var` 的卡表被清空（与 `region_object` / `region_array` 同步），
  下一轮 minor 从干净的脏集合开始

## Pipeline Steps

不涉及编译期 pipeline（纯 VM 运行期变更）：

- [ ] Lexer
- [ ] Parser / AST
- [ ] TypeChecker
- [ ] IR Codegen
- [x] VM runtime（`gc/var_region*` + `gc/arc_heap/generational.rs`）
