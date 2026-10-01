# tasks: signal-missing-body-key

> 类型：**fix**（把一处静默丢弃改为内部不变式）｜ 创建：2026-09-27
> 出身：[结构审计 2026-09](../../../internals/src/compiler/source-compile.md) 的 R3-e。

## Why

`IrGenMemberEmitter.EmitMethod` 里：

```z42
if (md.HasBody) {
    string key = ownerKey + "." + methKey;
    if (bodyM.HasBody(key)) { …发射函数… }
    // ← 没有 else
}
```

`md.HasBody` 为真（**AST 亲口说有体**）而 `bodyM.HasBody(key)` 为假时，**这个方法一个字节都不发**，
编译成功、零诊断。直到运行期才以 `undefined function …` / `MissingSymbolException` 现形 ——
位置离原因很远，而原因是**编译器自己的键构造 bug**，不是用户的错。

这条缝为什么一直在：体查找键是**手拼字符串**，写端 `DeclBinder` 7 处、读端约 10 处各拼一遍，
**没有共享构造器**；发射侧还另有 `emitKey`（会被改写成 `IrStaticCtor.MethodKey` 或
`methKey + "$struct"`）与 `irName` 两套名字。本文件上方的注释就记着一次实测：
「两者一起改会让 `model.HasBody` 落空 → 函数根本不发射（实测：报 `static ctor C.$cctor not found`）」。

## What Changes

补上 `else if (!g.HasTypeErrors) { throw … }`，消息里同时打出**两个键**并指出该去对哪几处拼法。

取 `throw` 而非诊断码，同本包既有 **4 处**不变式先例（`OverloadResolver.MethodKeyOf` 的
`unify-regkey 不变量` / `ExprEmitter` 的 unhandled kind / `IrGen` 两处不收敛）：这是「不该发生」
的内部状态，报一条用户看不懂的诊断码不如直接指出不变式被破 —— 而且栈回溯正是有用的那部分。

⚠️ **`!g.HasTypeErrors` 这道守卫是必须的**：有类型错误时绑定器本来就可能没绑这个体
（**IrGen 在 `ErrorCount > 0` 时照常全量跑**），那时落空是预期的、诊断已经报过 ——
不加守卫会把「一堆诊断」变成「编译器崩」。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/Emission/IrGenMemberEmitter.z42`
- `docs/internals/src/compiler/source-compile.md`（记下键的协议 + 这道守卫）

## Tasks

- [x] `else if (!g.HasTypeErrors) { throw … }` + 消息打两个键
- [x] `docs/internals/` 新增「方法体的查找键是两端手拼的，落空必须大声失败」一节
- [x] **阳性**：stdlib 25 包 + 编译器自建全过 ⇒ 真实代码上不响
- [x] **阴性对照**：把 `key` 故意改成 `ownerKey + ".__NEGCTRL_" + methKey`，用**它编出来的 driver**
      去编一个小工程 ⇒ 抛 `z42c internal: no bound body for 'A.__NEGCTRL_F$1$int' while emitting
      'N.A.F$1$int' …`，栈回溯指到 `IrGenMemberEmitter.z42`
      > ⚠️ 这个阴性对照有个坑：`build compiler` 是**用旧 driver 编新源码**，所以「改坏源码后
      > build compiler 成功」不算对照 —— 必须再拿**新建出来的那个 driver** 去编东西。
      > 而坏 driver 连自己都编不动（`build compiler` 用的就是它）⇒ 恢复要从种子拷回
      > `artifacts/build/compiler`，这是自举的鸡蛋问题。
- [ ] GREEN：`xtask test compiler`（含自举不动点）+ 全量；CI 全矩阵绿

## 不做（Out of Scope）

- **不把键构造收敛成共享函数**（审计建议的 `BodyKey.Of(ownerKey, methKey)`）。那要动写端 7 处 +
  读端 10 处、并把 `emitKey`/`irName` 的两处特例改写一起理清，是中刀、需单独一刀；且**先有这道
  守卫再去重构更安全** —— 重构期间任何一处拼错都会当场大声失败，而不是静默丢方法。
- **不给它一个诊断码**。内部不变式不是用户可见契约；给码还会撞上新加的规则 ⑨
  （活码必须有测试按码断言），而「编译器键 bug」很难从用户源码触发。

## 验证

- 不改任何产物字节（只在**本不该发生**的分支上抛）⇒ 无格式 bump、无指纹 bump；
  自举字节不动点由 `test compiler` 的 gen1==gen2 确认。
