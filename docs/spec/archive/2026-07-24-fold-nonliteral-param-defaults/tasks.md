# Tasks: 非字面量参数默认值常量折叠（fold-nonliteral-param-defaults）

> 状态：🟢 已完成 | 创建：2026-07-24 | 完成：2026-07-24 | 分支：feat/reflection-fold-param-defaults（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（纯 z42c IrGenFacts；闭 add-param-metadata 的常量表达式默认值剩余项）

**变更说明：** `ParameterInfo.DefaultValue` 此前只折**字面量**默认值；常量表达式（`1+2` / `-5` /
`-3.14` / `!false` / `2*3+1` / `1<<4` / `3>2` …）返 null。现由递归常量折叠器折出其值。

**原因：** add-param-metadata（2026-07-10）default_kind 编码已落，Out of Scope 留了「扩展只需
IrGen 折叠更多 Expr 形态」。

**修复（纯 z42c，无格式 bump）：** `IrGenFacts._fillParamMeta` 的 inline 字面量 check 抽成递归
`_foldDefault(Expr) → DefaultFold`：字面量 + 一元(`-`/`!`/`~`/`+`) + 二元(算术/位/比较/逻辑)
over 已折 int/float/bool，任意深度。除零 / 越界移位 / 混合类型 / enum / 命名常量 / 字符串拼接 →
保守回落 kind 0（`DefaultValue==null`，`IsOptional` 仍由 `d!=null` 保证），**绝不产错值**。
复用 SIGS `default_kind` 编码（0/1/2/3/4/5）——不改格式。仅影响反射元数据（call-site 默认值应用
走另一路径，运行期语义不变）。

**文档影响：** `docs/design/language/reflection.md`（fold-nonliteral 标记部分落地）。

- [x] 1.1 `IrGenFacts.z42`：`DefaultFold` 类 + 递归 `_foldDefault`/`_foldUnary`/`_foldBinary`；`_fillParamMeta` 改用之
- [x] 1.2 `src/tests/types/fold_param_defaults.z42`：e2e（-5/1+2/2*3+1/!false/1<<4/&/3>2/-3.14 折出 + enum/concat 仍 null + IsOptional）——interp+jit 空输出 exit0
- [x] 1.3 全绿：types e2e **78 pass 0 fail** + stdlib 全库 pass（test_param_metadata_default_values 无回归）+ **compiler 自举不动点 5/5 gen1==gen2 byte-identical**
- [x] 1.4 `docs/design/language/reflection.md` 标记
- [x] 1.5 归档 + PR

## 备注
- 自举：IrGen 改动使含非字面量默认值的函数 SIGS 元数据字节变（折出值）→ z42c/stdlib zpkg 字节变，
  但 gen1==gen2 byte-identical 成立（新 z42c 自建两遍同）。以 test compiler 为准。
- 保守性：任何不确定折不出 → kind 0（当前安全行为），不引入错误默认值。
- 剩余：enum 成员 / 命名常量 / 字符串拼接需符号解析 + 常量求值，延后（另开 change）。
