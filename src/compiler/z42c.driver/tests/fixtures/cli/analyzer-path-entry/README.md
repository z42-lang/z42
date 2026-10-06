# `[analyzers]` 的 path 条目

用户自己的 generator 工程按路径挂上去：z42c 定位它、校验它确是编译期扩展、**代建**、把产物喂给 generator 引擎，
不需要手工把 zpkg 拷进 SDK `libs/`。

- ② 仍打印 42 = handler 指纹没接上 path 条目：「改了扩展却编出旧结果」的静默错值。
- ③ analyzer 永不链入产物，写进 `[dependencies]` 就是让运行期代码引用一个不会到场的包。
- ④ 普通库挂进 `[analyzers]` 的失败模式是「加载成功、发现 0 个 handler、什么都不做」的静默空转。断言的是**那一句**诊断
  （`不是 "analyzer"`），不是「含 analyzer 字样即可」：kind 校验若被删，kind=lib 的工程会在代建阶段因解析不到契约包而失败，
  那条报错也含 analyzer 字样 —— 宽断言会把「校验没了」判成绿。
- ③ 排在 ④ 前面：④ 把 `pathgen` 改成了 lib，之后它就能合法地当运行期依赖了。
