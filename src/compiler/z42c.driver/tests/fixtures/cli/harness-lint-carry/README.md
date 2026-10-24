# harness 携带 lint（z42.harness.lint）自动生效

z42.harness 经 [plugins.export] 携带 z42.harness.lint（首个携带插件，harness-as-plugin PR 2）。
标准库包免声明、总是可见 ⇒ 它的携带对**每个**消费方生效（ambient 携带链）——本工程零
[plugins]、零 [dependencies]，H0002 照样报出（①）。插件 zpkg 平铺在 stdlib flat 同级 plugins/
（sibling 查找域），仓库自举（workspace 成员构建）不做携带、不受影响。

- H0002 抓的是 E0911–E0917 的真盲区：编译期校验按首个 kind 注解、运行期发现按末个覆盖——
  混贴（如 [Test][Setup]）先过编译、再静默失效。
- ② 是可关性断言：`[lints] "H0002" = "none"` 后构建输出不再含 H0002（携带与显式同权受管）。
