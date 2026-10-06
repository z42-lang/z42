# probing-paths：声明的运行期搜索目录 + 侧车跟随

守 `[profile.<n>.runtime] probing-paths` 能不能让一个**既不在 exe 目录、也不在 `libs/`** 的依赖在运行期被找到
（「配置了就不复制过去」那条路的后半段；前半段「构建期不复制」见 `deploy-framework-not-copied`）。

- ① 是 ② 的对照：不配就必须跑不起来，否则 ② 的「跑通」可能只是依赖碰巧还能从别处解析到。
- ③ 不是凑数：`[profile.*.runtime]` **不进源 hash**（它不影响编译产物），于是「只改运行时配置」恰好全命中增量缓存
  ⇒ 走 preserved 早退 ⇒ 侧车留在上一次的值。实测踩过：改了 probing-paths 重建，构建报成功而侧车纹丝不动——
  用户改了旋钮，产物里还是旧配置。
- `${Z42_HOME}` 占位符见 `probing-paths-z42-home`，数组写法见 `probing-paths-array`。
