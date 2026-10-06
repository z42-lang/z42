# 清单身份进包级缓存键

清单自身的身份（name / version / kind / entry / 声明依赖）由 `z42c.driver/src/Main.z42` 以 `|mf:` 折进 `depsId`。
「只改 toml、源码一字不动」时它若不在键里，probe 全命中 ⇒ `no changes; preserved` 早退 —— 既能发出旧产物，也能吞掉本该判红的诊断。

**两格判据，第二格是第一格换不来的**：

- ① 改 `[project].version` ⇒ 产物字节必须变（它进 zpkg 的 META 段），改回必须回到原字节；
- ② 加一条**不存在**的 `[dependencies]` ⇒ 构建必须判红，且错因是那条依赖（只判退出码会把「夹具本身编不过」也当成通过）。
  「声明了却不存在」的检查在 `Main.z42` 的 declaredDeps 循环里，位置在 preserved 早退**之后** ⇒ 名单不在键里时
  全命中、早退、这条检查根本跑不到，红被吞掉。

把声明依赖名单从键里去掉，① 照样绿 —— 所以必须有 ②。
