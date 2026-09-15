# Design: 构造器可见性

## Decisions

### D1: 检查点
选中构造器（`MethodSymbol`）之后，用既有 `AccessChecker.CheckAccess(ms.Visibility, ms.ContainingTypeName, env, …, "constructor", 类名, span)`：
- `ConstructTyper._bindNew`：`new` / target-typed `new()` / 对象初始化器 / 元组脱糖都汇到这里（`cms` 选定处）。
- `DeclBinder._bindMethodBody` 初始化子句：`: base(..)` 与 `: this(..)` 的目标构造器。
判据与字段 / 方法完全相同（private 看 `CurrentClass()==声明类`；protected 沿当前类基链；internal 看声明类是否导入）——
不另写一套。

### D2: 主构造器 public
C# 的主构造器（record 与 C# 12 class/struct）总是 public；z42 的元组脱糖为 `[Record] struct ValueTuple2<…>(…)`，若主构造器按
「无修饰符 = private」处理，**元组字面量在任何用户代码里都无法构造**。parser 合成主构造器时把修饰符写成 `"public"`，
比在语义层给「主构造器」开特例更直接（AST 如实表达它的可见性，TSIG / 反射可见性字节随之为 public）。

### D3: 默认 private 不变
User 裁决按 C# 规则：普通构造器不写修饰符即 private。仓库内类外构造的 39 个类补 `public`（普查清单）。

### D4: 自举边界
- 新诊断只在**当前编译器**里生效；上一 nightly 编译当前源码时不检查 ⇒ `xtask test bootstrap` 不受影响。
- 当前编译器编 xtask / z42c 源时，它们引用的 stdlib 构造器必须已是 public：z42c 自依赖库经预建总是当前源；xtask 在
  `ci-bootstrap` 里先由种子编译器编（不检查）。stdlib 里被跨包构造的类型都是主构造器（D2 使其 public），普查中 stdlib
  没有「显式无修饰符构造器被跨包调用」。仍以 CI 冷启动为准。

## Implementation Notes

- 元组 / 集合字面量等合成 `new` 的 `Span` 指向字面量本身，E0404 文案用构造器所属类名。
- 按 GREEN 失败补 `public`：只改「在类外被构造」的构造器，不顺手改其它成员。

## Testing Strategy

- 运行期不变（纯编译期检查）；回归用例以 typecheck 单测为主（spec 8 个场景），另加 cross-zpkg 负例
  `ctor_visibility_cross_pkg`（`expected_error`）覆盖跨包 internal。
- 修前逐条确认「无诊断」（红），修后精确诊断码与条数。
- 全量 `xtask test` GREEN；`xtask test bootstrap`。
