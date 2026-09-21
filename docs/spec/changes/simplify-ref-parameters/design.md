# Design: 参数修饰符收敛为单一 `ref`

## Architecture

```
源码            void Inc(ref int x) { x = x + 1; }
                int c = 0;  Inc(ref c);
                        ↓ Lexer            `out` 移出关键字表；`in` 仅 foreach 用
                        ↓ Parser           形参侧：只认 Ref（MemberParser:340）
                                           调用点：只认 Ref + `ref var` / `ref _`（ExprParser:239）
                        ↓ TypeChecker      ① 修饰符对称性（形参有 ref ⇔ 实参有 ref）
                                           ② 实参/形参类型精确匹配（BoundRefArg 带真实类型）
                                           ③ 值类型限制
                                           ④ lvalue 检查（已有，_chkRefArgLvalue）
                        ↓ IR Codegen       ref 实参 → LoadLocalAddr（不变）
                                           被 ref 传过的无 init 局部 → 零值初始化（新）
                        ↓ VM               完全不变（copy-in / copy-out 保留）
```

**本变更不触碰运行期、JIT、GC。**

---

## Decisions

### D1：局部变量槽位自动取零值 —— 但只对被 `ref` 传过的

**问题：** 砍掉 `out` 之后，`int v; f(ref v);` 里的 `v` 从哪来一个合法初值？

**现状：** `BoundVarDeclStmt` 无 init 时 **完全不发 IR**（`StmtEmitter.z42:30` 注释「无 init（G8）：
不发 IR——变量首次赋值时 `_emitAssign` 绑定寄存器」）。所以 `ref v` 取不到槽。

**选项：**

| | 做法 | 问题 |
|---|---|---|
| A | 所有无 init 的值类型局部一律发零值初始化 | **churn 掉大量 golden**（任何含无初始化局部的函数指令流都变），且 99% 的局部随后立刻被赋值，零值是白发的 |
| **B（选定）** | 函数级预扫描：只对**在本函数内被 `ref` 传过**的无 init 局部发零值初始化 | 需要一次轻量预扫描 |
| C | 由 DA 强制用户写 `= 0` | 回到仪式性代码；且与「砍 out」的前提冲突 |

**选 B。** 预扫描在 `FunctionEmitter` 发射函数体前做一遍，收集所有 `BoundRefArg` 的 inner
所指向的局部名；`BoundVarDeclStmt` 无 init 且命中该集合 ⇒ 分配寄存器 + 发零值常量。

**收益：** 除被 `ref` 传的局部外，**所有现存函数的 IR 指令流 byte-identical** ⇒ golden 基本不动。

**不变式：** 语言里永远不存在未初始化的存储 —— 值类型局部若被取址，必已零初始化；
若未被取址，则只能经赋值后使用（现有行为）。

### D2：砍掉 `out`

`out` 的四条规则全部服务于"未初始化内存"这一个例外。D1 消灭了这个例外 ⇒ 四条规则一起消失。

| `out` 规则 | D1 之后 |
|---|---|
| callee 必须在所有正常返回路径赋值 | 不需要——调用方拿到的最差是零值，不是垃圾 |
| callee 进入时不可读 | 不需要——槽位必定已初始化，随时可读 |
| caller 调用后视为已赋值 | 不需要——调用前就已赋值 |
| throw 路径不要求赋值 | 例外的例外，一并消失 |

**已知损失：** callee 在某条正常返回路径忘写出参，不再是编译错误。
按路径分：失败路径忘写 → 调用方按约定不用该值，无害；成功路径忘写 → 主逻辑坏，测试会抓。
真正独占价值 = "抓一条没有测试覆盖的成功路径"。**接受**，需要时以 lint 形式回补。

### D3：砍掉 `in`，只读别名交给后续的优化器

`in` 的只读保证在 z42 从设计时起就是不完整的——`parameter-modifiers.md` 自记：
「`in` 仅约束 slot 不可重赋，不约束指向对象的内部状态」。要补完需要 `readonly struct` +
成员级 `readonly` 一整套标注系统，且 C# 装完之后仍有 defensive copy 陷阱。

性能价值由后续 change 以**编译器推导**替代，判据是 IR 事实而非声明：

- 着力点：`CallEmitter._emitStructAwareArgs` 对每个 blob struct 实参**无条件**发
  `StructAlloc + StructCopy`
- 优化：若 callee 的 IR 在任何路径上都不写该形参槽 ⇒ 省掉这两条，直接传句柄
- 正确性论证：别名与复制唯一可能观察到差异的情形，是调用期间有人经另一路径写同一位置；
  callee 不写 ⇒ 无此写 ⇒ 两者等价
- 无 transitive hole：`p.Mutate()` 会写 ⇒ 不做变换 ⇒ 自动退回复制

**不在本变更范围**（需跨函数摘要），但记录于此以说明砍 `in` 不损失性能目标。

### D4：不动运行期调用约定 —— copy-in / copy-out 是对的

曾怀疑 copy-in/copy-out 是性能缺陷，**实测推翻**：

| 事实 | 依据 |
|---|---|
| copy 的是 `Value`（16 B），不是 struct blob | blob 值 struct 以 `StructRef { idx, frame_id }` 句柄在寄存器间流转 |
| `ref` 实参**不走** struct copy-in | `BoundRefArg` 的 `Type()` 是 `Z42UnknownType` ⇒ `_isBlobStruct` 为假 ⇒ 跳过 `_emitStructAwareArgs` 的复制分支 |
| callee 改 struct 直接改到 caller 的 blob 上 | copy-in 得到的是同一个句柄 |
| 对标量，两次 16 B 复制**快于**真别名 | 真别名要求每次访问都间接 |
| architecture E 的收益 | 「callee 的 80+ 指令 handler 完全不需要感知 Ref」（`interp/mod.rs`） |

⇒ 真别名改造（会牵动 interp + JIT + GC 间接根）**不做**，本决策写入文档防止后人重复怀疑。

### D5：`ref _` 与 `ref var v`

| 形式 | 语义 | 实现 |
|---|---|---|
| `ref var v` / `ref int v` | 调用点声明局部 + 零值 + 传地址 | 已有 `IsVarDecl` 路径（`ExprParser:241`、`ExprTyper:287`、`ExprEmitter:113`），改关键字即可 |
| `ref _` | 隐藏零值槽；表达「不要这个出参」 | `RefArgExpr.IsDiscard`；发射期分配匿名寄存器 + 零值，不登记 `Locals` |

`ref var v` 声明的变量类型：沿用现有做法（`env.Define(name, Z42UnknownType())` 后由赋值推导）。
本变更不改这一点 —— 但由于形参类型现已可用，**改为按形参类型定义**是更好的做法，列入 tasks。

### D6：限制到值类型

`ref` 形参与实参的类型必须是值类型（基元 / enum / struct）。引用类型报错。

理由：引用类型本身即按引用传递，`ref` 对它唯一的含义是"重新绑定调用方的变量本身"，极罕见；
限制后 `ref` 写回逃逸（#690 那族）的面缩小，也让 `ref` 与后续可空标记模型完全不交互。

### D7：`BoundRefArg` 携带真实类型 + 不做隐式转换

`ExprTyper.z42:300` 现以 `Z42UnknownType` 构造 `BoundRefArg`，而 `Conversion.Classify` 对
unknown 吸收 ⇒ 类型检查形同虚设。改为携带 inner 的真实类型。

匹配规则：**精确匹配**（别名归一后，如 `int` ≡ `i32`），不做任何隐式转换——
转换会产生临时值，地址就失去意义，写回也会丢。

### D8：错误码

| 码 | 条件 |
|---|---|
| `RefArgModifierMissing` | 形参有 `ref`，实参无 |
| `RefArgModifierUnexpected` | 形参无 `ref`，实参有 |
| `RefParamNotValueType` | `ref` 用在引用类型上 |
| `RefArgTypeMismatch` | 实参与形参类型不精确匹配 |
| `ObsoleteParamModifier` | 源码使用 `out` / `in` 作参数修饰符，提示改用 `ref` |

沿用 `DiagnosticCodes.z42` 现有 E04xx 段位分配惯例；具体数值在实施时取未占用值。

`ObsoleteParamModifier` 必须给出可操作的迁移提示（`out T x` → `ref T x`，调用点 `out v` → `ref v`）。

### D9：迁移

全仓 32 处，全在测试/脚本：

| 位置 | 处理 |
|---|---|
| `src/tests/refs/out_var/` | 改 `ref var`，目录改名 `ref_var` |
| `src/tests/refs/in_param/` | 删除；替换为阴性用例 `in` 作形参修饰符 → 报错 |
| `src/tests/refs/ref_local`, `ref_nested`, `ref_array_elem` | 不变（本就用 `ref`） |
| `scripts/test/xtask_*.z42`（12 处） | 逐处改写 |
| `src/compiler/z42c.semantics/tests/{codegen,layout}` | 内嵌源串里的 `out` 改写 |

---

## Risks

| 风险 | 说明 | 缓解 |
|---|---|---|
| **自举** | 改 parser ⇒ 新编译器要能编自己。若 stdlib/编译器自身有 `out`/`in` 写法会卡住冷启动 | 已实测：生产代码零使用，仅测试与 scripts。仍须按 `bootstrap-seed.md` 走冷种子流程 |
| `ForwardGenerator:384` | 对 `out`/`in` 形参生成错关键字 | 三态消失后自然只剩 `ref`；须有转发用例覆盖 |
| golden churn | D1 若按选项 A 会大面积 churn | 选 B：仅被 `ref` 传过的局部受影响 |
| `scripts/test/` 不在常规构建里 | 12 处 `out` 改漏不会被本地构建发现 | tasks 单列一步，grep 全仓 `\bout\s+[A-Za-z_]` 清零 |
| Q1 未决 | 参与重载与否影响 `_dupSigKey` | spec 按 A 写；改 B 只动一节 |

**不涉及**：格式 bump（`IsRef` 不进元数据——运行期靠寄存器里是否为 `Value::Ref` 动态识别）、
JIT、GC、zbc/zpkg minor。
