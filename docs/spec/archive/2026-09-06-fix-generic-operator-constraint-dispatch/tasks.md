# Tasks: fix-generic-operator-constraint-dispatch

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

**变更说明：** `where T : INumber<T>` 下的 `a + b` 现在真的派发到约束接口的 `static abstract op_X`。
此前该代码路径**整个不存在**：类型检查报 E0402，emitter 照发**裸 `add i32`**。

**原因：** `ExprTyper._bindBinary` 的运算符重载分支要求 `lt is Z42ClassType`，`Z42GenericParamType`
不匹配 → 落到 `BinaryTypeTable` → `_checkOperand` 报「operator `+` requires numeric operand, got `T`」。
而发射端不看这个诊断，直接按算术表发 `add i32`。后果：

- **对用户类型是静默错误代码**：`struct Money : INumber<Money>` 传进 `T Add<T>(T,T) where T : INumber<T>`
  → IR 是 `add i32 %0, %1` 打在 struct blob 上，解释器 `int_binop` 收不了。
- `int` / `double` 之所以一直绿，**纯属意外**——解释器的 `add` 对 `Value` 动态派发。
- **约束从未被读**：删掉整条 `where` 子句，诊断逐字节相同（实测）。

`src/tests/operators/static_abstract_operator.z42` 的抬头注释当时已经把这条路径描述得一清二楚
（"TryBindOperatorCall sees a Z42GenericParamType and finds the static abstract member…"），
但那是**设计意图而非现状**——又一处「文档白纸黑字写着这样是对的」。

**文档影响：** `docs/book/src/language/generic-constraints.md` 新增机制小节；测试抬头做事实校正。

## 1. 根因修复
- [x] 1.1 `TypeChecker.z42`：新增 `_curMethodDecl` + 构造置 null —— 方法级 `where` **不预登记**
      符号表（`ConstraintChecker.CheckMethod` 每个调用点即席建 bundle），体内绑定 `a + b` 时唯一
      能拿到约束的地方就是当前 MethodDecl
- [x] 1.2 `DeclBinder._bindMethodBody`：设 `_curMethodDecl = md`（紧邻既有 `_curMemberName`）
- [x] 1.3 `ExprTyper._bindBinary`：新增 `lt is Z42GenericParamType` 分支，**先于**既有
      `Z42ClassType` 运算符重载分支；命中则发 `BoundCall("instance", true, left, T, op_X, [right], 1, T)`
      —— 与既有可用写法 `a.op_Add(b)`（`MemberResolver` 的 Z42GenericParamType 分支）**同一发射形状**
- [x] 1.4 `ExprTyper._constraintOperatorMethod`：查方法级 `where`（`md.Wheres`）+ 类级
      `ConstraintSet`（`SymbolTable.ClassConstraints`，键走 `ConstraintKey` + **类级** arity）
- [x] 1.5 `ExprTyper._ifaceOperator`：接口按裸名查 `Methods`（`MemberCollector` 就是这么登记的），
      并沿 `BaseNames` 递归父接口
- [x] 1.6 结果类型**恒取左操作数型参**，不读 `gopMs.Signature.Ret`
      —— 依据是协议本身（`INumber` 抬头：`T + T → T only`）。**这条是必需的**：`INumber` 是导入
      接口，签名经 `ImportedSymbolLoader` 还原后返回类型已不是型参形态 → 链式 `a + b + c` 的第二个
      `+` 拿到非型参左类型就落不回本分支，退化成裸算术（实测 Money 链式炸在这，先写成读 Ret 才发现）

## 2. 测试
- [x] 2.1 `src/tests/operators/static_abstract_operator.z42`：补 `struct Money : INumber<Money>`
      + 四条断言（Add / Sub / Mul / **Chain3 链式**）—— 用户类型才是真正会炸的形态，原有
      int/double 断言靠解释器动态派发意外能绿
- [x] 2.2 同文件抬头**事实校正**：把「TryBindOperatorCall 已处理型参」改写为现状 + 历史说明
- [x] 2.3 `src/tests/generics/generic_inumber.z42`（`x.op_Add(x)` 方法写法）不回归

## 3. 文档同步
- [x] 3.1 `docs/book/src/language/generic-constraints.md`：新增「运算符如何在型参上派发」小节
      —— 绑定路径 + 两条必须知道的规则（结果类型恒为 T / 实现方必须 `static override`）+ 历史

## 4. 验证
- [x] 4.1 `xtask build compiler` 自建全绿
- [x] 4.2 最小复现从错到对：`Money` 的 Add/Sub/链式全部由 `add i32` 变为 `vcall op_X` 并跑对
- [x] 4.3 完整 `xtask test` GREEN（含 self-host 不动点）
- [x] 4.4 分支基于 origin/main 顶 → PR

## 备注
- **自举安全**：z42c / stdlib 里没有「型参 + 运算符 + INumber 约束」的写法（否则今天就已经在发裸
  算术、跑不起来），故对自举字节无影响；无格式 bump、无新语法。
- **踩过的坑**：`--dump-ir` 走**无 deps** 路径（不吃 `Z42_LIBS`），此时 `INumber` 解析不到、约束
  查不到 → 仍显示 `add i32`。验证本修复必须用 `--emit-zbc`（带 `Z42_LIBS`）后**真跑**，别只看 dump。
- **实现方必须 `static override`**：只写 `static` 会注册到另一个键，运行期报
  `VCall: function X.op_Add not found`。写最小复现时先踩了这一次。
- **本修复是 [[restore-emit-zbc-diagnostics-program]] 的第 4 步第 2 项**（口令「推进诊断可见性」），
  三条静默错误代码之二。第一项 = `fix-autoprop-getonly-backing-write`（属性访问打在源名字段上）。
