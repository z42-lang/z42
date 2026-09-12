# dup_free_function_crosspkg — 跨包同名自由函数 + 静态调用位（E0601）

两个互不依赖的包在**同一个 ns** 各声明了自由函数 `helper()` 与静态类 `Util`。

## 为什么单独立一道门（类那道盖不住）

- **自由函数比类更糊**：`ImportedSymbols.Functions` 的键是**裸名**（`ExportedFuncZ.Name` 就是
  `md.Name`，从来没有 ns 前缀）⇒ 连 FQN 视图都没有，同名者无论同不同 ns 都撞一个键、first-wins；
  而 z42 的自由函数只有裸名一种调用形态，**输的那份没有任何写法能指到**。
- **静态调用不走 `_chkTypeRef`**：`Util.go()` 里的 `Util` 只经 `GetClass` 取类，于是
  `report-crosspkg-duplicate-type` 挂在 `_chkTypeRef` 上的 E0601 **漏掉了这条路径**——
  实测同 FQN 的 `Util` 在这里零诊断。本 change 一并补上。
- **运行期也指望不上**：输的那个包因惰性加载压根不会被载入 ⇒ 连
  `duplicate function … keeping first-loaded` 都不会打印。**编译期是唯一防线。**

负例 fixture 约定（`expected_build_error.txt`）见 `../README.md`。
