# Tasks: z42c 增量失效闭包过保守 —— 加一道「声明面闸门」

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06 | 类型：fix（最小化模式）

**变更说明：** 给文件级增量的传递失效闭包加一道**传播闸门**——源变了但**声明面**
（去掉方法/属性/索引器体后的 token 流）没变的文件不当传播源。改注释、改函数体只重编它自己。

**原因：** `IncrementalBuild.Close` 的 token 保守边把 `_definedOf` 收集的**成员名**也登记成属主名，
于是任一定义了 `Name()` 的文件一变，全包凡是提到 `Name` 的文件统统失效。实测 xtask 工程
（64 文件）**只加一行注释** → `cached: 0/64 files`，增量彻底失效。这是
add-file-level-incremental design D3 里就记着的 Deferred `incremental-future-tsig-level-invalidation`。

**文档影响：** `docs/book/src/dev/build.md`（增量编译节）、`src/compiler/z42c.pipeline/README.md`、
`src/compiler/z42c.driver/README.md`。

## 任务

- [x] 1.1 `src/compiler/z42c.driver/src/SurfaceHash.z42`（NEW）：声明面指纹
      —— token 流按 AST 给出的体起始 `{` 偏移 + token 层大括号配平，把每个体压成 `{}` 记号后哈希
- [x] 1.2 `src/compiler/z42c.pipeline/src/CacheStore.z42`：`CacheMeta.SurfaceHash` + `surface` 行；
      MetaVersion 3→4（缺该行的旧条目一律作废）
- [x] 1.3 `src/compiler/z42c.pipeline/src/IncrementalBuild.z42`：`IncrFilePlan.PrevSurface` /
      `SurfaceChanged`（默认全 true = 闸门前行为）；`ProbeFiles` 填 `PrevSurface`（含种子失效行）；
      `Close` 的传播条件加 `SurfaceChanged[j]`
- [x] 1.4 `src/compiler/z42c.driver/src/IncrementalDriver.z42`：`ParseAllTk` 顺手算 fresh 行声明面指纹
      （零第二遍 lex，沿用 `want` 口径）；`Prepare` 比对并压低 `SurfaceChanged`；降级行同样压低；
      `WriteMetas` 写入 `surface`
- [x] 1.5 `src/compiler/z42c.driver/src/Main.z42`：`metaSurfaces` 贯通两条 parse 路径到 `WriteMetas`
- [x] 1.6 `src/compiler/z42c.pipeline/tests/incremental/incremental_tests.z42`：闸门两态单测
      （声明面相等 → 零传播；声明面变 → 照旧全传递）+ `PrevSurface` 保留 + meta v4 往返/pin
- [x] 1.7 文档同步（book 增量编译节 + 两个 README）
- [x] 1.8 GREEN：`xtask test` 全绿（3m14s，10 stage）+ `xtask test incremental` 单跑
      （非默认 gate stage）：demo 5/5 + xtask 65/65 dist 文件与全量逐字节相等

## 备注

- **正确性依据**（为什么「声明面没变 ⇒ 引用方 cached zbc 仍有效」）：跨文件依赖只经声明面——
  内联 `IrInline` 只在同一 `IrModule`（= 单 CU）内解析 callee；逃逸/纯度摘要同为模块级不动点，
  跨模块无摘要即保守；泛型不单态化进调用方（类型实参随 `CallInstr.MethodTypeArgs` 走运行期）；
  唯一把值抄进消费方的 `const` 字段，其初值写在**字段声明**里、本就在声明面内。
- **闸门只掐传播的起点**：被波及而转 fresh 的行 `SurfaceChanged` 保持默认 true，故
  「A 继承 B、C 用 A」这类布局经中间文件透传的链条与闸门前完全一致地整条失效。不去猜哪些扩散可省。
- **保守方向**：漏登记一种带体的声明形态 → 那段 token 留在指纹里 → 该文件一改照旧全波及
  （多编不错编）。危险方向（多挖 token）已被「只按 AST 给出的体起始 `{` + 大括号配平」挡住。
