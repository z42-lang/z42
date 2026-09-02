# Tasks: List/Dictionary 查找族补齐

> 状态：🟢 已完成 | 创建：2026-09-02 | 完成：2026-09-02

## 进度概览
- [x] 阶段 1: List 查询族（partial 拆分）
- [x] 阶段 2: Dictionary 查找族
- [x] 阶段 3: 测试 + 文档 + 验证

## 阶段 1: List 查询族
- [x] 1.1 `List.z42`：`class`→`partial class` + 拆分说明注释
- [x] 1.2 新建 `List.Query.z42`：Find/FindLast/FindIndex/FindLastIndex/FindAll/
      Exists/TrueForAll/RemoveAll/LastIndexOf/GetRange/BinarySearch

## 阶段 2: Dictionary 查找族
- [x] 2.1 `Dictionary.z42`：TryAdd / GetValueOrDefault(key) / GetValueOrDefault(key, default)

## 阶段 3: 测试 + 文档 + 验证
- [x] 3.1 `tests/list_query.z42`（10 [Test]，含 default/边界/string）
- [x] 3.2 `tests/dictionary_lookup.z42`（5 [Test]，含 grow/无幽灵插入）
- [x] 3.3 README 功能索引 + List 尺寸例外 + Deferred 段
- [x] 3.4 build stdlib z42.core —— 编译通过（partial + 重复约束 OK）
- [x] 3.5 test stdlib z42.core —— 42/42 文件通过
- [x] 3.6 完整 GREEN（xtask test 全 stage）—— 全绿(self-host 3/3 + cross-zpkg + stdlib 311/20 + compiler 23)

## 备注
- 用户裁决：List 满 200 行类型限 → partial 拆文件 + 记录在案的对标尺寸例外；
  Dictionary TryGetValue 跳过（依赖 out，待 out→tuple 迁移）。
- Deferred：ConvertAll / AsReadOnly / TryGetValue / ContainsValue（见 README）。
