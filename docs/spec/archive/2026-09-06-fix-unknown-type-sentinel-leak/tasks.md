# Tasks: `<unknown>` 哨兵泄漏 → 运行期伪装成合法 Type

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06 | 变更类型：**fix**（最小化模式）

**变更说明：** 编译器表示「类型未解析」的内部哨兵 `<unknown>` 会被当成合法类型名 intern 进 zbc；
运行期因其「`<` 开头 `>` 结尾」把它**误判为构造泛型**，拆出空 base 去造类型，得到一个名字全空、
`GetMethods()` 为 0、`BaseType` 为 null 的 `Type`——一个**损坏对象被当作正常对象呈现**。
顺带兑现 `Type.GetType(fqn)` 文档承诺的 "null if unknown"（实现从不返回 null）。

**原因：** 这是 philosophy.md 点名的反模式——「解析失败时降级为 sentinel 值，让下游用启发式去猜」。
消费端拿到的对象与「真实存在但没有成员的类型」无法区分。

**发现路径：** `add-method-reference`（methodof）的前置验证。

**文档影响：** 无对外命令面 / API 变更（`Type.GetType` 是让实现回到既有文档承诺，非新行为）。

---

## 1. 编译器：哨兵不得以尖括号形态进 IR

- [x] 1.1 `ExprEmitter._typeofName`：为 `Z42UnknownType` 加显式分支返回 `"unknown"`，
      **沿用仓库既有约定**（`FunctionEmitter.z42:219` 的 SIGS 拼写已是 `<unknown>` → `"unknown"`），
      不新造机制。消除尖括号形态即让运行期无法再把它误判为构造泛型。

## 2. runtime：不得把损坏输入补全成合法对象

- [x] 2.1 `type_object.rs` 构造泛型分支：`<` 前 base 为空时**不得**按泛型构造，落到正常的
      「解析不到」路径 —— 名字如实保留，不再凭空捏造一个类型实参。
- [x] 2.2 `builtin_type_get_type`：**限定类名**（含 `.`、非 `[]`、非 `<…>`）解析不到真
      `TypeHandle` 时返回 null，兑现 `Type.z42` 的文档承诺。数组 / 构造泛型 / 基元 /
      无点简名维持既有合成语义——那些是承重的（`typeof(int[])`、跨包泛型实参简名解析依赖它）。

## 3. 测试

- [x] 3.1 `undefined_type_tests.z42`：未解析 typeof 的 IR 不含 `<unknown>`
- [x] 3.2 `undefined_type_tests.z42`：`BuildModuleD` 确实携带 `ErrorCount` / `DiagMsgs`
      （锁住后续「让 `--emit-zbc` 呈现诊断」所依赖的数据契约）+ `deps`/`imported` 传 null 可用
- [x] 3.3 `src/tests/types/type_get_type_unknown.z42`（e2e）：未知限定名 → null；
      真实类型 / 数组 / 无点简名不受影响；`GetType("<unknown>")` 名字如实保留且实参数为 0
      > 空 base 守卫**改用 e2e 而非 Rust 单测**：`reflection_tests.rs` 的裸 `VmContext` 未加载
      > z42.core，`build_type_ex` 此时返回 `Value::Null`（文件头注释自述只覆盖 "no-handle paths"），
      > 断不了名字；而 `Type.GetType(任意串)` 直通 `make_type_from_name`，正好打到守卫。

## GREEN 门

- [x] G1 `xtask test` 全绿（interp）—— `✔ test`，零 FAIL，3m23s
- [x] G2 自举字节不动点 gen1 == gen2（`compiler` stage 46.4s 通过）
- [x] G3 `xtask test lines` 全绿（`lines` stage 2.0s 通过）

---

## 本变更**不含**：`--emit-zbc` 吞诊断（P1）

原计划一并修「`--emit-zbc` 丢弃全部编译诊断、exit 0 且照写产物」。**实现并验证通过后，
按 User 2026-09-06 裁决拆出**，理由是它掀开了一大片长期不可见的破损，远超本变更体量。

实测数据（留档给后续程序，勿重复调研）：

- 打开这道门后 **78 个 e2e 用例编不过、530 条诊断**（另有 96 条是调查脚本把 cross-zpkg
  多包 fixture 当单文件编造成的**假阳性**，不计）。
- **不是单文件模式的毛病**：同一份源码走 `z42c build` 报一模一样的错（已用最小工程实证）。

| 码 | 条数 | 文件数 | 性质 |
|---|---|---|---|
| E0404 私有成员访问 | 412 | 51 | 测试语料欠债；User 已裁决**默认 private 是正确设计，该改测试** |
| E0410 `break` outside of loop | 65 | 8 | 🔴 **编译器 bug**：`switch` 内的 `break` 被误判，12 行最小程序即复现 |
| E0402 `unknown[]` → `Attribute[]` | 26 | 10 | 未查明 |
| E0401 `no field __prop_Label` | 12 | 8 | 测试直写 auto-property 内部 backing 字段名 |
| E0443 未定义类型 | 9 | 4 | 未查明 |
| E0202 `void` 不能作成员名 | 6 | 1 | parse 错，未查明 |

**关键教训：不得靠批量给测试加 `public` 把红变绿**——里面至少有一个确凿的编译器 bug（E0410），
另有 23 个文件的三类未查明问题可能藏着更多。那样做等于用症状级修复盖掉真 bug。

P1 的实现已验证可行（实测：错误逐条打印 + 非零退出 + 不写产物；正例零回归），
后续程序直接复用即可，不必重做。续推见 memory `restore-emit-zbc-diagnostics-program`。
