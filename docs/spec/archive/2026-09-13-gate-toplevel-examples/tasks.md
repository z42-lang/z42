# Tasks: gate-toplevel-examples

> 状态：🟢 已完成 | 创建：2026-09-13 | 类型：test + docs（假保障排查程序 · 第 6 批第 2 条）

**变更说明：** 仓库根 `examples/` 的 22 个 `.z42` **从来没有被任何 stage 编译过**，而 examples
stage 每轮照常打印「✅ examples: 1 compiled」。实测 **8 个编不过**，其中 5 个用的是 z42
**压根没有的语法**——它们是照 C# 写的、从没被编译过，烂了多久无从查起。

## 根因

`_exampleRun` 只扫两个根：`src/libraries/*` 与 manifest-target fixtures。而 `src/libraries/`
下**没有任何** `examples/` 目录或 `[[example]]` 声明 ⇒ 顶层 `examples/` 不在任何扫描面内，
那句 `✅ examples: 1 compiled` 里的 1 是合成 fixture。
（`xtask_test_changed.z42` 还把 `examples/` 映射成 skip。）

## 实测：8 个编不过（`z42c --emit-zbc`，#550 后它不再吞诊断）

修好 3 个（**真·示例 bug**，与语言能力无关）：

| 示例 | 真问题 |
|---|---|
| `target_typed_new.z42` | 字段/方法无修饰符 = **private**，跨类读 `p.X` / 调 `box.run()` 报 E0404 |
| `collection_literals.z42` | 插值洞里放字符串字面量（`$"{d[\"k\"]}"`）打挂 lexer |
| `type_alias.z42` | 同上 |

## 🔴 剩 5 个卡在「z42 没有的 C#-ism」上（逐条最小复现验过）

| 构造 | 最小复现 | 现状 |
|---|---|---|
| 异常过滤器 | `catch (E e) when (c)` | E0202 |
| 表达式体**属性** | `int P => 1;`（**方法** `f() => 1` 是支持的） | E0202 |
| 命名实参 | `f(x: 1)` | E0202 |
| 无参无体类 | `abstract class X;`（`class X(...)` / `class X { }` 都支持） | E0202 |
| 模式组合子 | `case > 0 and < 9:` | E0202 |
| 主构造转发基类实参 | `class D(m) : B(m)`（`: B` 无实参是支持的） | E0202 |
| 插值洞内字符串字面量 | `$"{d[\"k\"]}"` | E0203 |
| 范围索引 | `s[1..]` | E0202 |

⭐ **命名实参只差 parser 一个 `name:` 分支**：语义层的重排 + 可选参数填充（`_adaptArgs` /
`_withDefaults`）早就在，`docs/book/.../target-typed-new.md` 也明文提到「命名实参适配逻辑」。
而 `examples/named_args.z42` **整个文件的主题就是它**。要不要实现属语言决策，未在本 change 内做。

> 这些是**语言特性决策**，不该由一次「加门」的改动顺手把旗舰文档改写成别的样子、或顺手实现
> 5 个语言特性。故本 change 只建门 + 记账，把决策连同证据交出去。

## 实现：门 + **双向**棘轮

- 门：`_topLevelExamplesGate` 编译顶层 `examples/*.z42`，判据 = `z42c --emit-zbc` 退出码。
- 棘轮（`scripts/test/examples-known-broken.txt`，同 `line-limit-baseline.txt` 惯例）：
  - 名单**外**编不过 → **红**（新增破坏）
  - 名单**内**编不过 → 响亮打印，不阻断（已知欠债，每条注明卡在哪个构造）
  - 名单**内却编过了** → **红**，提示删除该行

  第三条是关键：`audit-silent-gates-program` 里那次「把已修好的记成永久欠债」正是被这一半抓住的
  ——**只写「失败则放行」的记账机制，本身就会变成下一个假保障**。

## 验证

- [x] 门接进 `xtask test` 的 examples stage：**17 编过 / 5 已知欠债 / 0 失败**
- [x] **校准 ①**：把 `hello.z42` 改坏（名单外）→ `✗ … 新增破坏`、1 失败
- [x] **校准 ②**：把 `hello.z42`（编得过）加进名单 → `✗ … 现在编得过了，请删掉该行`、1 失败
- [x] `xtask test` **冷构建**全绿（13 stage）+ 自举不动点 3/3
- [x] 零格式 bump

## Deferred

- **8 个 C#-ism 的取舍**（实现 / 改写示例 / 明确不做）。优先级最高的是**命名实参**——
  它只差 parser 一个分支，且有一个专门演示它的示例文件。
- `examples/embedding`、`examples/global_using` 等**子目录**未纳入本门（它们是带 toml 的工程，
  需要按工程编而非单文件）。
