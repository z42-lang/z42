# deps-plugin-zpkg — [dependencies] .zpkg path 分流（deps-plugin-routing）

引用写在 [dependencies]：z42c 读 zpkg 头自述——是插件包 ⇒ 自动分流（加载进编译器进程、
目录不并入 libsDirs、不链入产物、不参与 bundle）；是普通库 ⇒ 照旧运行期依赖。

- 断言①②：build 绿 + generator 产的 E2eOut.O.V() 可用（stdout = 42）。
- 断言③（指纹面）：只改 generator 源（42 → 7）重编插件产物；消费方源码不动、引用路径不动 ⇒
  仍必须重编出新值（stdout = 7）——否则「换了插件版本却编出旧结果」的静默错值。
