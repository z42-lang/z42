# 入口自动检测：类里的静态 `Main()`

清单里没有入口键（`[project] entry` 报错），入口全靠自动检测。类方法的 IR 名带零参重载后缀
（`Scm.Program.Main$0`），检测要认它、并烤入这个**真实名字**：VM 按函数名精确匹配，烤成 `Scm.Program.Main`
运行期就是 `entry function not found`。

- 第 ① 段必须真的跑产物：只编不跑抓不到「烤入的名字 VM 找不到」。
- 第 ② 段：实例方法与静态方法 IR 名同形（都带 `$0`），只能按 `IsStatic` 区分；实例 `Main` 不是入口。
