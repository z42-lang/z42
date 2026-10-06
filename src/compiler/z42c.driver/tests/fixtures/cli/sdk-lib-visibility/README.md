# SDK 库的可见性（编译期扩展的解析域）

同一份 generator 源码（实现 `ModuleGenerator`），三种清单：

| 目录 | kind | 声明 `z42c.semantics` | 期望 |
|---|---|---|---|
| `analyzer-undeclared` | analyzer | 否 | 编过（SDK 库对 analyzer 自动可见） |
| `lib-declared` | lib | 是 | 编过 |
| `lib-undeclared` | lib | 否 | 编不过，报错点名 `z42c.semantics` 并给出按名声明写法 |

- 只有前两格的门管不住「哪天有人图省事把 `z42c.semantics` 塞进 `libs/`」—— 那样它们照样绿，而隔离已经没了。第三格
  编过 = 未声明的 SDK 库漏给了普通工程；红了但不点名 = 门失去判别力。
- 每格**独立目录**：同目录只改清单会让增量缓存参与进来，把「解析得到吗」混进「缓存命中吗」。
- `Z42_LIBS` 必须是纯 stdlib flat（harness 默认）：若它混装了编译器包，它们对所有工程可见，第三格恒绿。
