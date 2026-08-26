# Tasks: optimize-parser-char-array

> 状态：🟢 已完成 | 创建：2026-08-27
> 类型：perf / refactor（最小化模式）；属「stdlib 性能改善程序」PR2

**变更说明：** YAML / JSON / TOML 解析器与 Regex 匹配引擎的源串一次 `ToCharArray()`
物化为 `char[] _chars`，把逐字符游标读取从 `CharAt` builtin 调用换成 O(1) 数组索引
（ArrayGet）。`_src` / `_input` 保留供 `.Length` / `.Substring`。
**原因：** `CharAt` 虽 O(1) 摊还（VM 有字符元数据缓存），但每次调用仍走 builtin 派发 +
thread-local 缓存查找，远重于一条数组索引指令。热解析器逐字节扫描时这个常数因子显著。
**文档影响：** 无——纯内部机制替换，逐字符行为字节等价，由现有解析器测试套件守住。

## Design 决策
- **保留原始 string 字段**（`_src`/`_input`）：`.Length` / `.Substring` 仍走 string（都 O(1) /
  高效），只把热点 `CharAt(i)` 读取替换为 `_chars[i]`。避免把 Substring 改写成
  `FromChars(slice)` 这类更大、更易错的改动。
- **只换源游标读取**：局部串（YAML 的 `s`、Regex 的 `pattern`/`replacement` 等）的 `CharAt`
  不在本次范围——它们是次要路径且转换更 invasive。
- **Regex 每次 match run 物化**：`_input` 在 `ResetState` 每次赋值，`_chars` 同处物化，
  与 `_input` 始终一致；backtracking 引擎重复读同位置从中获益最大。

## 进度
- [x] 1.1 YamlParser：加 `_chars` 字段 + ctor 物化；96 处 `this._src.CharAt(x)`→`this._chars[x]`
- [x] 1.2 JsonParser：同上（4 处）
- [x] 1.3 TomlParser：同上（11 处）
- [x] 1.4 Regex：加 `_chars` + ResetState 物化；6 处 `this._input.CharAt(x)`→`this._chars[x]`
- [x] 2.1 校验：无残留源串 CharAt、`_chars` 均声明+初始化、Regex 读取都在 ResetState 后
- [ ] 3.1 GREEN 交 PR CI（本机 z42vm wedge）；字节等价由 yaml/toml/json/regex 现有测试守住

## 备注
- 零格式 bump（纯 stdlib 源码）。行为字节等价（无新增测试；现有解析器测试套件覆盖）。
