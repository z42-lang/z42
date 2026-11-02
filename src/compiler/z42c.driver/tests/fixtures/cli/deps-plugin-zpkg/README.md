# deps-plugin-zpkg — [dependencies] .zpkg path 分流（deps-plugin-routing）

与 plugin-zpkg-path-entry 用**同一个产物**，但引用写在 [dependencies]：z42c 读 zpkg 头的能力位
自述——是插件包 ⇒ 自动分流（加载进编译器进程、目录不并入 libsDirs、不链入产物、不参与 bundle）；
是普通库 ⇒ 照旧运行期依赖。断言：build 绿 + generator 产的 E2eOut.O.V() 可用（stdout = 42）。
