# tasks: fix-cache-key-manifest-inputs

> 类型：**fix**（最小化模式 —— 不改语义，补一条缓存失效边）｜ 创建：2026-09-26
> 出身：[结构审计 2026-09](../../../../.claude/../docs/roadmap.md) 的止血项 U-2（`depsId` 漏输入）。

## Why（一句话）

包级缓存身份 `depsId` **不含清单自身的身份**（name / version / kind / entry / 声明依赖名单 / profile），
而 `depsId` 相符 + 源文件全命中会走 `no changes; preserved` **早退**。于是「只改 toml、源码一字未动」
有两种静默错误：

1. **发错产物** —— `[project].version` 进 zpkg 的 META 段。改版本号重建，构建报成功，
   **发出去的包里还是旧版本号**。
2. **吞掉本该判红的诊断** —— 「声明了却不存在的依赖」检查（`Main.z42` 的 declaredDeps 循环）
   位置在早退**之后**；按名依赖删一条不改变 libsDirs 的集合与内容 ⇒ `DepIdentity.Of` 不变
   ⇒ 全命中 ⇒ 这条检查根本跑不到。DEPS 段同时陈旧（#849 的传递闭包据它建）。

同一形状此前出现过四次：`[build] incremental` / `[optimize]` / `[syntax]` 三次改为**扩键**；
`[properties]` 与 exe 依赖装配走的是**早退路径上的补偿补丁**。本次选扩键——补偿补丁只能救
已知的那一个产出物。

## Scope（允许改动的文件）

- `src/compiler/z42c.driver/src/Main.z42` —— 拼 `depsId` 处新增 `|mf:` 段与 `|dep:` 段。
- `scripts/test/xtask_test_incremental.z42` —— 新门 `_manifestIdentityTakesEffect`。
- `docs/internals/src/compiler/project-model.md` —— 新增「包级缓存身份（`depsId`）」机制节。

## Tasks

- [x] `depsId` 折入 `|mf:<name>@<version>/<kind>/<entry>/<rel|dbg>`
- [x] `depsId` 折入每条声明依赖 `|dep:<name>`（按清单顺序）
- [x] 门禁 `_manifestIdentityTakesEffect`，**两格判据**：
      ① 改 `[project].version` ⇒ 产物字节必须变；
      ② 加一条不存在的 `[dependencies]` ⇒ 构建必须判红（且理由正确）。
      ②不是①换得来的——把依赖名单从键里去掉，①照样绿。
- [x] `docs/internals/` 记下键的组成、早退的危险、以及「新增 manifest 输入要扩键、不要打补偿补丁」的纪律
- [ ] GREEN：`xtask test incremental` 本地过；CI 全矩阵绿

## 不做（Out of Scope）

- **`[lints]` 不折入本次**。它影响的是诊断而非产物字节，且与 `[syntax]` 的 E0301 那条路
  形态不同（lints 可能只改 warning 等级）——单独评估，免得把「本该 warn 的没 warn」
  和「本该 error 的没 error」混成一条。
- **依赖名单不排序**。重排 `[dependencies]` 会多触发一次全量；过度失效是安全方向。
- **不动 `DepIdentity.Of` 的签名**。`excludeName` 只用于排除自身、不进哈希这一点保持不变，
  包名改动由 `|mf:` 覆盖。

## 验证

- 正面：`_manifestIdentityTakesEffect` 两格全过。
- 阴性对照：撤回 `Main.z42` 的 `|mf:` / `|dep:` 两段 ⇒ 门必须变红（**撤回修复本身**，
  不是改期望值）。
- 无格式 bump、无指纹 bump：只改缓存键的**组成**，键变了即当 fresh、自愈；
  同一份源码的编译结果（含诊断集）不变。
