# deps-plugin-by-name — [dependencies] 名字条目的插件分流（deps-plugin-routing）

插件 zpkg 与普通库**物理分开**：独立分发的插件放插件目录（Z42_PLUGINS / <sdk>/plugins），
消费方在 [dependencies] 里按名声明——z42c 读 zpkg 头的能力位自述分流：加载进编译器进程
（generator 产的符号可用），但**不**链入产物、目录不进运行期解析域。判据是构造保证的：
能力位只在 kind=plugin 构建时写头，lib/exe 恒 0。

- ① 阴性：插件目录还没有那份 zpkg ⇒ 普通依赖的「未找到」错误（与拼错包名同款，位置在用户熟悉处）。
- ② 编出插件产物、放进插件目录。
- ③ 阳性：Z42_PLUGINS 指向它 → 分流加载、generator 产的符号可用（stdout = 42）。
- 插件 zpkg 误放进 libs/ ⇒ 硬错（查找域分离不变量），错误信息指路插件目录。
