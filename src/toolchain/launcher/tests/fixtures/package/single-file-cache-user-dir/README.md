# single-file-cache-user-dir

缓存不许落进 SDK 安装目录：安装位常是只读 / 需提权的，多用户共享时还会互相串。干净的包里没有 `cache/`，
跑一次 `z42 run` 就不能多出来。

判据写两段，少任何一段都不够：

1. SDK 根下不得出现 `cache/`（`absent = ["{sdk}/cache"]`）；
2. 用户级目录下**真的**有 `cache/run/`（Windows：`%LOCALAPPDATA%/z42/cache/run`；其余：`~/.cache/z42/run`）——
   否则「SDK 干净」也可能是因为缓存整个坏掉了没写成。
