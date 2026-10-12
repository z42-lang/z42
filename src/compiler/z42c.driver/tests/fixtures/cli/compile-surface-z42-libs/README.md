# 编译期依赖面只看 `Z42_LIBS`

z42c 解析被编译工程的 `[dependencies]` 只看 `Z42_LIBS`。z42c 自己运行期加载的库另有来处：VM 按 入口目录 →
`Z42_PROBING_PATHS` → `Z42_LIBS` 的顺序找依赖，自举时上一代 driver 要的那一代库就挂在 probing 上
（xtask `_z42cProcSeed`）——同名包运行期先取 probing 那份，编译期仍只解析 `Z42_LIBS`。

- ① 是守卫：依赖只在 probing 里时编译必须找不到它。probing 一旦漏进编译期面，自举时上一代的库就会和当前源新建的库
  同时出现在解析面上（同 FQN 来自两个包 ⇒ `E0606`，或静默解析到旧版）。
- ② 是正向对照：同一个依赖放进 `Z42_LIBS` 就编过。
- ③ workspace 路径单独测：workspace 构建自己拼解析域（成员 dist + 外部档），口径要与单包一致。
- ⚠️ `dep/` 必须在 `ws/` 之外：`members = ["*"]` 会把根下任何带清单的目录吞成成员，`cldep` 就从成员 dist 解析得到，③ 失去判别力。
