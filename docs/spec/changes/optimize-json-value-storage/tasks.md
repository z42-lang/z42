# Tasks: optimize-json-value-storage

> 状态：🟢 已完成 | 创建：2026-08-27
> 类型：perf（最小化模式）；属「stdlib 性能改善程序」PR3

**变更说明：** JsonValue 对象存储加 `Dictionary<string,int>` 名字→slot 索引，令
ContainsKey/Get/Set 从 O(n) 线性扫描降到 O(1) 平均——构建/读取 n 键对象从 O(n²) 降到
O(n)。JsonWriter.QuoteString 改为「普通字符成段 Substring 批量 flush」，消除每字符
char[1]+FromChars 分配。
**原因：** JSON 是热 API；parse（Set per key）与 serialize（WriteObject 里 Get per key）
双双踩 O(n²)。QuoteString 对每个普通字符分配一个临时字符串串。
**文档影响：** 无对外行为变更（键序、输出字节均不变）；内部数据结构变更，设计决策见下。

## Design 决策（内部数据结构）
- **并行数组仍是 SoT**：`_objectKeys`/`_objectValues` 保持插入顺序（JSON 输出必须序稳定），
  `_objectIndex` 只做加速查找的旁路索引，不改变遍历顺序（Keys() 仍按数组序）。
- **为何不用纯 Dictionary 存对象**：Dictionary 迭代序不稳定（hash 分布相关），会破坏
  JSON 输出的键序稳定性。故「有序数组 + 旁路 hash 索引」而非「换成 Dictionary」。
- **索引仅对 kind==6 存在**：非对象值 `_objectIndex` 为 null；ContainsKey 先判 kind，
  Get/Set 在非对象上先抛异常，都不会触到 null 索引。
- **无 Remove**：JsonValue 对象无删除 API，故索引不会与数组产生 desync。

## 进度
- [x] 1.1 `JsonValue.z42`：加 `_objectIndex` 字段 + OfObject 初始化 + ContainsKey/Get/Set 改用索引（append 分支同步注册）
- [x] 1.2 `JsonWriter.z42`：QuoteString 成段 flush，删除死方法 CharString
- [x] 2.1 bench：`json_bench.z42` 加 parse_large_object_200 + stringify_large_object_200（KB 级）
- [x] 2.2 测试：`tests/object_index.z42`（get/set/contains、覆盖保序、多键 grow、round-trip 键序、QuoteString 转义+成段）
- [ ] 3.1 GREEN 交 PR CI（本机 z42vm wedge）

## 备注
- 零格式 bump（纯 stdlib 源码）。键序与输出字节保持不变（round-trip 测试守住）。
