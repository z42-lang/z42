# `deploy = "sdk"`：不复制，运行期经 `${Z42_HOME}` 从所在 SDK 解析

- 产物里**没有** SDK 库副本，侧车的 probing-paths 带 `${Z42_HOME}/programs/z42c`；以一个只含
  `programs/z42c/{z42c.pipeline,z42c.semantics}` 的目录当 `Z42_HOME` 跑得起来 = 确实是从「SDK」解析到的。
- 选 `z42c.pipeline` 的理由同 `sdk-lib-exe-copy`：开发树 stdlib flat 里可能预建着 `z42.build` 等，它们在那里是「框架」。
