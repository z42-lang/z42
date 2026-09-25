# Proposal：依赖的部署模型（copy / shared）与运行期搜索路径

> 状态：**DRAFT，待 User 裁决**（2026-09-25）。起因见 [add-package-roles](../add-package-roles/proposal.md)
> 讨论末尾 User 提的三个问题。vm 类变更（旋钮），按 workflow 规范先行。

## 起因：三个问题

1. 只被某个 exe 引用的包，应直接复制到 exe 目录 —— 不共享、不扩散。
2. 被多个 exe 依赖的包如何共享：公共目录，还是**声明一组搜索路径**、配了就不复制？
3. 用户 exe 引用了 SDK 里的**非 stdlib** 程序集，也要能复制到 exe 目录。

三者是同一个问题的三面：**「这个依赖在运行期从哪儿来」今天是猜出来的，不是声明出来的。**
与 add-package-roles 的根因（「一个物理位置决定三件事」）同源。

## 现状（逐条读码 / 实测得出）

### 运行期：搜索序只有两级，但下游已支持多路径

| 事实 | 位置 |
|---|---|
| `search_dirs` = `[entry-zpkg 目录, libs 单目录]`，固定序、去重 —— **唯一组装点** | `runtime/src/app.rs:110` |
| `resolve_dependency(zpkg_file, libs_paths: &[PathBuf])` **已经接受路径列表** | `metadata/loader/namespace.rs:51` |
| `ValueKind::PathList`（平台分隔符分隔）**已存在**，`Z42_NATIVE_PATH` 在真用 | `config/knobs.rs:128`、`config/knob_table.rs:315` |
| `libs` 是旋钮，走五层优先级链（env / `--set` / **`[runtime].libs` sidecar** / …），但类型是 `Option<PathBuf>`，**单目录** | `config.rs:79`、`startup.rs:22` |

⇒ **插入一组 probing path 只需改 `app.rs:110` 一处**，下游与旋钮基础设施都已就绪。

### 🔴 `Z42_PATH` 是一个死旋钮，而它的名字正好承诺了问题 2

```rust
// main.rs:325
// Resolve module search paths (Z42_PATH + cwd + cwd/modules); log only for now.
let module_paths = resolve_module_paths();
if cli.verbose { log_module_paths(&module_paths); }
```

`Z42_PATH`（toml_key `path`）是 **PUBLIC 登记**的旋钮，`ValueKind::PathList`，描述写着
**"module search paths (platform-separated)"**，`consumed_by: "main.rs"`。而 main.rs 里它
**只喂日志**，之后再没进过 `search_dirs`。另一处 `resolve_namespace(ns, module_paths, libs_paths)`
的注释自陈「The VM's lazy loader **no longer** routes by namespace」——那条路是编译器工具/诊断用的。

⇒ 用户今天配 `Z42_PATH` 或 `[runtime].path`，`--list-knobs` 会如实列出它、描述看着正是他要的，
**而它什么都不做**。这是「**看起来正经、实则从不
生效的旋钮**」那一类——比没有这个旋钮更坏，因为用户以为自己配好了。

### 构建期：复制判据不一致，且在用户机器上退化成字符串前缀

| | 判据 | 传递依赖 |
|---|---|---|
| `z42c build` 的 `_bundleExeDeps`（`ExeDeps.z42:19`）| `<srcRoot>/libraries/<name>` 存在 ⇒ 真-stdlib ⇒ 不复制 | **仅直接依赖** |
| `z42b publish` 的 `_pubBundleProjectDeps`（`builder_publish.z42:561`）| **在不在 shipped `libs/`** ⇒ 不复制 | **传递闭包**（BFS）|

两处的注释各自声称与对方一致（`ExeDeps` 写「镜像 builder `_pubBundleProjectDeps`」「**与 publish
一致**：仅直接依赖」），**实际两条都不一致**。这是一条规范冲突，本 change 一并裁。

更要命的是 `_srcRoot` 靠向上找「同时含 `libraries/` 与 `compiler/` 的目录」—— **用户机器上这恒为空**，
于是回落到 `dep.StartsWith("z42.")`：

- 用户自己的包叫 `z42.mylib` ⇒ 被误判为 stdlib、**不复制**，运行期全看 `Z42_LIBS` 里碰巧有没有；
- 反过来，SDK 里的非 stdlib（`z42c.*`）不以 `z42.` 开头 ⇒ 复制 —— 碰巧对，但是**凭名字碰对的**。

## 主张

把「运行期从哪儿来」变成 per-dependency 的一等声明，三个问题收敛成一个机制：

```toml
[dependencies]
"mylib"       = { path = "../mylib" }                  # 默认 copy（问题 1）
"z42.core"    = "0.1.0"                                # framework → 默认 shared
"z42c.syntax" = { version = "0.1.0", deploy = "copy" } # 问题 3：显式要 SDK 包
"bigdata"     = { version = "1.0", deploy = "shared" } # 问题 2：不复制

[runtime]
probing-paths = ["../shared", "plugins/*"]             # 相对 exe 目录；支持通配符
```

运行期搜索序 `[entry-dir, ...probing-paths, libs]`。`deploy = "shared"` 即「配了就不复制过去」。

**配套的硬要求**：声明 `shared` 却在任何 probing path 里都找不到 ⇒ **构建期报错**，不等运行期炸。
否则这就是又一个「配了以为生效」的旋钮——本 change 正是为修一个这样的旋钮而起。

## 待裁决

| # | 问题 | 选项 | 建议 |
|---|------|------|------|
| **A** | `deploy` 的默认值 | ① framework 默认 `shared`、其余 `copy`；② 一律默认 `copy`（self-contained）| **①** —— 等于今天的实际行为，只把判据从「名字前缀」换成声明，零行为变更且顺手消灭误判 |
| **B** | 传递依赖 | ① 统一成**闭包**（随 publish）；② 统一成仅直接依赖（随 build）| **①** —— 要求用户「直接声明全部所需兄弟」是把实现细节推给他；代价是 `z42c build` 产物变大 |
| **C** | probing path 的载体 | ① **复活 `Z42_PATH`**（它已登记、类型已对）；② 退役 `Z42_PATH`、新增 `probing-paths`；③ 扩 `libs` 为 PathList | **②** 见下 |
| **D** | framework 判据 | ① 在不在 shipped `libs/`（publisher 今天的判据，用户机器上可靠）；② 等 `role` 落地后读 role | **①先行、②收编** —— 不阻塞在 role 上 |

### C 为什么建议新增而不是复活

`Z42_PATH` 的语义是 **module search paths（`.zbc` 模块）**，历史上与 `.zpkg` 依赖解析是两条路
（`resolve_namespace` 的 `module_paths` vs `libs_paths`）。把它改成「zpkg 依赖的 probing path」
是**改变一个已登记公开旋钮的含义**，而它今天还有 `.zbc` 那条语义的残留调用方。

⇒ 建议：**新增 `probing-paths`（PathList）承载本设计**，同时把 `Z42_PATH` 按「死旋钮」单独处理
——要么接通它原本承诺的 `.zbc` 语义，要么明确退役并从 `--list-knobs` 移除。**两件事分开，
不要让一个死旋钮的历史债决定新机制的形状。**

## 通配符与相对路径（User 明确要求）

- **相对谁**：相对 **exe（entry zpkg）所在目录**，不是 cwd —— cwd 会让同一个安装在不同工作目录下
  行为不同。绝对路径原样使用。
- **通配符**：`plugins/*` 展开为一层子目录；`**` 递归。**展开发生在运行期解析时**（安装后新增的
  插件目录无需重新构建即可被发现），但**构建期也要展开一次**做 `shared` 的存在性校验。
- 展开结果需**稳定排序**（common-pitfalls §1：禁依赖 FS 碰巧序），否则同名 zpkg 落在两个目录时
  解析结果不确定。
