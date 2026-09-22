# Spec：自由函数取引用的 target-typed 消解（可验证场景）

> 本变更的行为 delta。每条为 golden / 单测可断言的场景。
> `IntUn`/`LongUn` 等为示意委托：`delegate int IntUn(int)`、`delegate long LongUn(long)`、
> `delegate long Widen(int)`。示例自由函数：`int Twice(int x)`；`long Twice(long x)`（同名重载）。

## S1 赋值位按目标委托精确选中

```z42
IntUn  a = Twice;   // 选 int Twice(int)  —— a(3) == 6
LongUn b = Twice;   // 选 long Twice(long) —— b(3L) == 6L
```

- 均**编译通过**、运行期调用到**对应**重载。
- 断言：`a(3)` 结果 = int 版；`b(3L)` 结果 = long 版（两版行为可区分，如返回值或副作用）。

## S2 变量声明 / 字段初始化 / return 同等生效

```z42
IntUn f = Twice;                       // 局部声明
class C { public IntUn F = Twice; }    // 字段初始化
IntUn make() { return Twice; }         // return 位
```

三处均按各自目标委托类型精确选中 `int Twice(int)`，编译通过。

## S3 调用实参位

```z42
void apply(IntUn g, int v) { ... }
apply(Twice, 5);          // 实参位按形参委托 IntUn 选 int Twice(int)
```

- 编译通过，`apply` 内 `g(5)` 调 int 版。
- 单份（非重载）funcref 实参 `apply(Solo, 5)`（`Solo` 无重载）行为不变。

## S4 无重载匹配目标 → E0477

```z42
StrUn s = Twice;   // delegate string StrUn(string)；Twice 无 (string)->string 重载
```

- 报 **E0477**，消息含 ``no overload of free function `Twice` matches target delegate type
  `StrUn```，并列出全部候选签名（`int Twice(int)` / `long Twice(long)`）。
- 下划线落在 `Twice` 标识符。

## S5 无目标位 → 保持 E0425（消息更新）

```z42
var x = Twice;          // 无声明委托类型
someOverloadedSink(Twice);   // 目标形参本身也重载、无法定位 → 见 S7
```

- 报 **E0425**，消息在原文后追加「assign to a variable of the target delegate type to
  disambiguate」。

## S6 单份自由函数取引用：行为与字节不变

```z42
IntUn f = Solo;   // Solo 无重载
```

- 编译通过、运行期正确（现状已支持）。
- **发射逐字节不变**（RegKey == base 名）。由不动点 3/3 gen1==gen2 守。

## S7 泛型基类替换后同签名多匹配 → E0425（歧义）

（仅当两个候选替换后签名与目标精确相等时可达；正常同名重载因签名不同不会多匹配。）
报 **E0425 ambiguous**，列出匹配候选。

## S8 跨包重载取引用

```z42
// 目标自由函数重载定义在依赖包 Dep，import 后取引用
IntUn a = Dep.Twice;    // 若走 ns 限定；或 using Dep; IntUn a = Twice;
```

- 选中 imported 重载，发 `LoadFn @QualOf(Dep-ns, RegKey)` + 记依赖 ns。
- 运行期解析到 Dep 包内该重载并正确调用。

## 不变量（回归护栏）

- **无目标 + 无重载**：`IntUn f = Solo;` 永远等价现状（S6）。
- **存量 zbc/zpkg 字节稳定**：任何既有单份 funcref 发射不变（不动点 3/3）。
- **E0477 有触发对象**：本 spec 自带 S4 fixture，避免「新诊断零触发 = 零证据」。
