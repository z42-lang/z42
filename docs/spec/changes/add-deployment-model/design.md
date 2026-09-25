# Design：依赖部署模型

> 动机与现状见 [proposal.md](proposal.md)。本文件是技术设计 SoT。**待 User 裁决 A–D 后定稿。**

## 模型

一个依赖有两个正交维度：

| 维度 | 问的问题 | 取值 | 谁决定 |
|---|---|---|---|
| `role`（add-package-roles） | 它在**哪个域**活 | `runtime` / `compile-time` | **被依赖方**在自己清单里声明 |
| `deploy`（本 change） | 运行期**从哪儿来** | `copy` / `shared` | **消费方**在 `[dependencies]` 里声明 |

两者的联动只有一条：`role = compile-time` 的包**永不进 payload** ⇒ `deploy` 对它无意义，
写了要报错（它根本不在运行期出现）。

### `deploy = "copy"`

构建期把 `<name>.zpkg`（+ `.zsym`）复制进 exe 的 dist。运行期由 `search_dirs` 的第一级
（entry-zpkg 目录）命中。**私有、不共享、不扩散** —— 问题 1 与问题 3 的答案。

### `deploy = "shared"`

构建期**不复制**。运行期由 probing path 命中。构建期须验证「至少一条 probing path 里有它」，
否则报错 —— 问题 2 的答案。

## 运行期搜索序

```
search_dirs = [entry-zpkg 目录] ++ probing-paths(展开后) ++ [libs]
```

唯一组装点 `runtime/src/app.rs:110`；下游 `resolve_dependency` 早已接受路径列表，不改。

### probing-paths 的解析规则

| 规则 | 决定 |
|---|---|
| 相对路径的基准 | **entry-zpkg 所在目录**（不是 cwd —— 同一安装在不同工作目录下行为必须一致）|
| 绝对路径 | 原样使用 |
| 通配符 | `*` 一层、`**` 递归；展开为**目录**（zpkg 从展开后的每个目录里按文件名找）|
| 展开时机 | 运行期解析时展开（安装后新增插件目录无需重建）；构建期也展开一次，仅用于 `shared` 的存在性校验 |
| 顺序 | 声明序；同一模式的展开结果按路径 **Ordinal 稳定排序**（禁 FS 碰巧序）|
| 不存在的路径 | 跳过，不报错（插件目录可以是可选的）|

> ⚠️ **同名 zpkg 出现在多个目录**：按搜索序取第一个命中，不做版本比较 —— 与今天
> `resolve_dependency` 的行为一致（`for dir in libs_paths { if path.is_file() { return } }`）。
> 版本选择是另一件事，不在本 change。

## 构建期判据

```
deploy 显式声明？ → 用它
否则 → framework？ → shared : copy
```

`framework` 的判据（裁决 D）：**在不在 shipped `libs/`**（= publisher 今天用的那条）。
它在用户机器上可靠（SDK 布局固定），而 `_bundleExeDeps` 今天那条 `<srcRoot>/libraries/<name>`
在用户机器上恒退化为 `dep.StartsWith("z42.")`。

⇒ **两处判据统一到一个 helper**，消灭「ExeDeps 注释声称镜像 publisher、实际不同」这条冲突。

## 传递依赖（裁决 B）

统一成**闭包**（随 publish 今天的做法）：消费方只声明直接依赖，间接依赖由工具补齐。
`_bundleExeDeps` 从「仅直接依赖」改为 BFS 闭包，与 `_pubBundleProjectDeps` 共用同一套
「复制判定 + 递归穿透」规则。

> 穿透规则同样要统一：publisher 今天「除**真·stdlib 成员**外一律递归」，为的是穿透
> 「已 ship 进 libs/ 却越界依赖非框架」的库（`z42.scripting` 依赖 `z42c.*`）。role 落地后
> 这条可以收编成「`role = runtime` 且 framework ⇒ 到此为止」。

## 与 add-package-roles 的关系

本 change **不阻塞在 role 上**：framework 判据先用「在不在 shipped libs/」，role 落地后
（批 2.5）再把它换成读 role。两者顺序无关，各自独立可合。
