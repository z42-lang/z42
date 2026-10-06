# analyzer-generator

**为什么必须在发布态验**：这条链在开发树里也有夹具（z42c 命令行夹具的 `sdk-lib-visibility` / `analyzer-path-entry`），
但那里 `Z42_LIBS` 指向 build-tree 的扁平 dist，契约包靠编译器域探测序的**最后一档**（自 `Z42_LIBS` 上溯到
`artifacts/build/`）命中。发布态走的是完全不同的一档（`Z42_HOME/programs/z42c`），开发树那格证明不了它 ——
插件作者在发布态拿到的会是一句位置在别处的 `E0443: undefined type`，没有任何东西诊断它。

按用户的实际动作排（SDK 库的可见性规则：analyzer 免声明可见；exe / lib 按名声明才可见）：

1. analyzer 工程（`kind = "analyzer"`，不声明契约包）用包里的 z42c 编得过；
2. 普通工程 `[analyzers]` 挂上它，编得过，且 generator 生成的代码真能跑（输出 42）；
3. 同一份源码改成 `kind = "lib"`、不声明 ⇒ **必须编不过**，且报错点名契约包并给出声明写法。少了这一步，
   第 1 步的「编过了」可能只是因为契约包对所有工程都可见（那样隔离就没了）；
4. `kind = "lib"` 按名声明 `"z42c.semantics" = "0.1.0"` ⇒ 编得过。

`aslib/` / `aslibdecl/` 只放清单，源码由 `copy` 从 `gen/` 拿同一份。
