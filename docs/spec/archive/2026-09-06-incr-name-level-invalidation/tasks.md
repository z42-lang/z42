# Tasks: 增量失效判据降到「名字」粒度

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06 | 类型：fix（最小化模式）

**变更说明：** 把文件级增量的失效判据从「文件的声明面变没变」降到「**我引用到的那个名字**变没变」。
每个 cache 条目改存 `名字 → 该名字自己的声明面指纹`（meta v4→v5），闭包据此判定。

**原因：** `fix-z42c-incremental-closure` 的闸门只掐住「改注释 / 改函数体」。**只要真改了声明**，
边仍是文件粒度的 —— 而 token 边的属主表把**成员名**也算属主，于是 `IosBackend` 定义了 `Name()`，
全包凡是提到 `Name` 的 60 个文件就都连着它。实测 xtask（64 文件）：**新增一个类型 / 新增一个函数 /
给某个类加一个方法，一律 `cached: 0/64`**——增量对「写新代码」这个最常见的动作完全无效。

**文档影响：** `docs/book/src/dev/build.md`（增量编译节）、`z42c.pipeline/README.md`、
`z42c.driver/README.md`。

## 设计

**判据两层：**

1. `seedChanged` = **种子失效文件**里「指纹变了 / 新增 / 删除」的名字集合（逐名字 diff meta）。
2. 文件 i 失效 ⟺ i 的标识符 token 集（**含方法体内**）碰到 `seedChanged` 里任一名字。

**传播也在名字粒度上**：文件 i 失效后，**只有当它的声明面（体已剥）提到了「别人的」已变名字**，
才把自己的全部定义名并入已变集、把波传下去。

- 必需 —— `A: class Foo : Bar`，`Bar` 变了则 `Foo` 的布局/vtable 跟着变，用 `Foo` 的 C 必须重编。
  A 的源没动、`Foo` 的指纹不足以反映这一点，靠这条规则接上。
- 足够 —— 若 A 与 `Bar` 的唯一关联在某个**方法体**里，A 的任何声明都没变，A 的消费方看到的 A 仍是
  原来那个 A（跨文件依赖只经声明面），不必重编。
- **必须排除「自己定义的名字」**：种子文件的声明面天然提到它刚改的名字，不排除的话种子第一轮就把
  自己全部成员名并进已变集，粒度当场退回文件级（实测复现过）。

**名字口径**（与增量边一致）：类型名 / enum 名 / 自由函数名 / 成员方法名 / 成员字段名 / enum 成员名。
类型名的指纹覆盖**整个类型声明的签名面**（修饰符、基类、类型参数、全部成员签名、字段声明含初值）。

**每个名字的指纹都带「文件头」前缀**（`namespace` + 全部 `using`）：它们不属于任何名字，却决定该
文件里的类型名解析到谁。

## 任务

- [x] 1.1 `z42c.driver/src/SurfaceHash.z42`：整文件指纹 → `NameSurfacesZ`（名字级指纹 + 声明面标识符集）；
      token 切片按「上一个声明的收尾符」定界
- [x] 1.2 `z42c.pipeline/src/CacheStore.z42`：meta v4→v5，`surface` 单行 → `nsurf <name> <hash>` 多行 + `sident` 行
- [x] 1.3 `z42c.pipeline/src/IncrementalBuild.z42`：`Close` 重写为「已变名字集 + 传播」；
      删掉 N² 边矩阵与 `OwnerNodeZ`（判据改为 StrMap 命中，比前身更快）；`IncrFilePlan.Prev` 留存上次条目
- [x] 1.4 `z42c.driver/src/IncrementalDriver.z42`：`_seedChangedNames` 逐名字 diff；三份闭包输入
      （tokens / surfIdents / names）cached 行一律取自 meta（零重算）；删掉 `_definedOf` / `_partialNamesOf`
- [x] 1.5 `z42c.driver/src/Main.z42`：`metaSurfaces` 类型跟着换
- [x] 1.6 `scripts/test/xtask_test_incremental.z42`：对账器加「改声明」轮
      —— 逐文件追加一个新自由函数 → 增量 dist 与全量逐字节对账
- [x] 1.7 文档同步（book 增量编译节 + pipeline/driver 两个 README）
- [x] 1.8 GREEN：`xtask test` 全绿（2m55s，exit 0）+ `xtask test incremental` 全绿（exit 0）
      —— 三轮对账：注释轮 demo 5/5 + xtask 65/65；**改声明轮 demo 5/5 + xtask 65/65**；
      dist-wiped 轮 demo 6/6 + xtask 66/66，全部与 `--no-incremental` 全量逐字节相等
- [x] 1.9 pipeline 增量单测 15 个全 PASS（新增三条：链式传播 / 只在函数体引用则波到此为止 /
      同名但未变的成员不被牵连）

## 实测（xtask，64 文件，release）

| 改动 | 修前 | 修后 |
|---|---|---|
| 新增类型 | `cached: 0/64` | **63/64** |
| 新增自由函数 | `0/64` | **63/64** |
| 新增成员函数 | `0/64` | **62/64**（多的 1 个提到了 `IosBackend`——加方法确实改了该类型的面） |
| 改成员函数签名 | `0/64` | **56/64** |
| 改成员可见性 `public→private` | `0/64` | **56/64**（debug 显示只归到 `Name` + `IosBackend`） |
| 只改注释 / 只改函数体 | 63/64 | 63/64（不回退） |

## 实施期踩到的四个坑（都会让粒度悄悄退回文件级，勿重犯）

1. **`IntBoxZ` 重名**：`IncrementalDriver.z42` 里早有一个同名同 ns 的类；新建第二份 → `as` 转型拿到
   null → `ArrayGet index got Null`。同命名空间下不要重复定义小工具类。
2. **种子自己触发传播**：见上「必须排除自己定义的名字」。
3. **切片停止集漏了 `{`**：类的第一个成员回退时穿过类头，成员切片与类型切片重叠（两者算出同一个指纹）。
4. **文件头没有右边界 + `Eof` token 进了切片**：文件头是每个名字指纹的前缀，最后一个 `using` 的区间
   一路吞到文件末尾 → 在文件尾追加任何东西都会让全文件所有名字的指纹漂移；`Eof` 同理，它永远落在
   最后一个声明的切片里。

## 备注

- **本 change 不动 `private`**：把私有成员排除出声明面能让「改私有方法签名」不牵连引用该类型的文件，
  但访问检查发生在成员解析**之后**且只发诊断（`AccessChecker` / `docs/book/src/compiler/access-control.md`），
  故新增一个 private 重载可能把别的文件里原本合法的调用抢过去 → 那个文件应当报 `E0404`。排除 private
  会让增量**漏报**这条错误，与全量构建对「这次构建红不红」产生分歧。另外 `private` **字段**对 struct
  有字节意义（`StructLayout` 算字节精确布局并共享给每个 CU 的 codegen），必须留在声明面里。
  名字粒度落地后，改私有方法只波及「提到该类型名或该方法名」的文件，已比修前小一个数量级，
  再拿诊断一致性换那点余量不划算。
- 嵌套类型只切两层：内层类型作为外层的一个「成员名」拿到自己的指纹，内层的成员不再单独切
  （粒度更粗，方向保守）。
- partial 类型不再需要专门的碎片 clique：改任一碎片 → 该类型名的指纹变 → 其余碎片（都写着
  `partial class Foo`）的 token 集命中 `Foo` → 连带失效。`_partialNamesOf` 随之删除。
