# 用户工程显式引用编译器域库

隔离不是禁止，是「不隐式可见」。用户写 linter / 格式化器 / 代码生成，就在 `[dependencies]` 里**按名声明** SDK 库
（`"z42c.syntax" = "*"`）；引用到的 zpkg 要被拷进他的输出目录（拷出去就能跑，不依赖 SDK 在不在）。

- 不能直接拿开发树的 stdlib flat 当「用户的 libs」：flat 里可能躺着破环预建写进去的编译器域包，① 在那里天然不成立。
  所以用例自己拼 `shipped/`（flat 减去编译器域）与 `cdomain/`（编译器域包，经 `Z42_COMPILER_LIBS` 交给 z42c）。
- ② 的「不过度复制」：真实 SDK 的 `programs/z42c/` 是 driver 的自包含闭包（含 stdlib 副本），解析域若排在框架之前，
  整套 stdlib 会被拷进用户产物。
- ③ 旧写法必须当场说清怎么改，而不是落成一句离原因很远的「文件不存在: …/${compiler_libs}/…」。
