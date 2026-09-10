# Tasks: report-duplicate-type-name

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：lang（新诊断 E0458）

**变更说明：** 同一命名空间内重复声明同一个类型此前**静默 last-wins**（后声明的赢，前一个连同
成员一起消失），**单文件与跨 CU 两种形态都零诊断**。现在报 **E0458**，对齐 C# CS0101。

```z42
namespace R;
public static class Dup { public static string Who() { return "first"; } }
public static class Dup { public static string Who() { return "second"; } }
void Main() { Console.WriteLine(Dup.Who()); }   // 改动前：rc=0，打印 "second"
```

**跨 CU 形态在真实工程里更危险**：两个文件各写一个 `class Config`，谁也不会注意到其中一个
从来没生效过。实测同样 rc=0 零诊断。

## 根因

`StubCollector._passClassStubs` 的碰撞分支：任一侧是 `partial` → 合并碎片；
**均非 partial → 直接 `_putClassStub` 覆盖**。那行注释白纸黑字写着
「碰撞但均非 partial（后者维持既有 last-wins 覆盖，无回归）」—— 是刻意保留、但从未被审视的行为。

## 修复：判据 = (ns, 名字, arity) 三者都相同

三个维度各有一个不能踩的坑，少一个就会误报：

| 维度 | 坑 |
|---|---|
| **ns** | `Classes` 是**裸名**键，同短名跨 ns（`A.Foo`/`B.Foo`，同一个包里）也撞它 —— 那是**使用点**歧义（E0456），不是重复声明。故按 `ClassesByFqn` 判。 |
| **arity** | arity-mangle 预扫是 **per-CU**（`_passClassStubs` 开头），符号表却是 per-package ⇒「a.z42 的 `class Foo` + b.z42 的 `class Foo<T>`」都拿裸键 `Foo`、撞进同一 FQN。**实测那种写法今天能编过**，报成「重复定义」是错的诊断。故还要比 `GenericParamCount`。Deferred：`arity-mangle-not-package-wide`。 |
| **partial** | partial 的重复是合并，走上面那条分支，不进重复判定。 |

**语义层用字面量 `"E0458"` 发码**（同 E0449–E0457 手法，避 core→semantics 新跨成员符号撞 F2
冷启动 stale-cache）；常量仍登记进 `DiagnosticCodes.z42` 作文档。

## 🔴 量欠债时我先量错了一次

第一版脚本按 **(ns, 类型名) 全仓分组**，得出「66 处重复」—— **全是假的**：那 66 组分布在
彼此独立编译的单文件 golden 里（`src/tests/types/get_properties.z42` 与
`static_fields_reflect.z42` 是两个各自编译的 CU）。**「重复声明」的单位是每一次编译**
（一个包 / 一个单文件 CU），不是仓库。改按编译体分组后降到个位数，其中还有几条是我的正则
把 `delegate <ret> <Name>` 的返回类型当成了名字（`Std.void` / `Demo.int`）。

⭐ 结论：**与其用正则做考古，不如把检查实现出来让编译器自己报** —— 判据用编译器真正的键
（FQN + arity），一次跑完 GREEN 就知道全仓有没有违反。

## 验证

- [x] 端到端：单文件形态 + 跨 CU 形态各报 1 条 E0458、非零退出；跨 CU 不同 arity **不误报**
- [x] 单测 6 条（`collect_tests.z42`，复用既有 `collectDiags` / `pmergeDiags` / `hasCode` helper）：
      3 条正例 + 3 条负例（跨 ns 不算重复 / 不同 arity 不算重复 / partial 不算重复）
- [x] 退回对照：撤掉 E0458 分支后 2 条正例变红、4 条负例照绿
- [x] `xtask test` 全绿 + 自举不动点 3/3
