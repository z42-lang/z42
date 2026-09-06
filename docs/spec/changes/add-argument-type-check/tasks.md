# Tasks: 调用实参类型检查

> 状态：🔴 DRAFT 待 User 确认 | 创建：2026-09-06
>
> 归属程序：[[restore-emit-zbc-diagnostics-program]] 第 ⑤ 步。
> **未获 6.5 确认前不得写实现代码**（lang 类变更，见 workflow.md Spec-First Self-Check）。

## 进度概览
- [ ] 阶段 0: 探索与量测（**已完成**，见 proposal 爆炸半径表）
- [ ] 阶段 1: R1 型参身份修复链（占 84%）
- [ ] 阶段 2: R2 / R7 —— 赋值上下文已可复现的既存 bug
- [ ] 阶段 3: R3 / R5 / R6
- [ ] 阶段 4: 接线开检查 + 负例门
- [ ] 阶段 5: 验证与归档

> 阶段 1–2 与阶段 3–4 的 PR 切分取决于 **Q1 裁决**（proposal Open Questions）。
> 推荐 A：PR-1 = 阶段 1+2，PR-2 = 阶段 3+4。

## 阶段 0: 探索与量测 ✅（本轮已做，无需重做）
- [x] 0.1 复现「实参零检查」：`TakeS(42)` rc=0、产物照写、运行期不报错
- [x] 0.2 定位三条调用路径的缺口（自由函数连 arity 都不查）
- [x] 0.3 确认可复用机制：`CheckImplicitConvert` + `Conversion.Classify(...).ImplicitOk()`
- [x] 0.4 确认汇聚点：`FillDeferredArgs`（21 处调用；`sig!=null` ⟺ 签名已知）
- [x] 0.5 探针实编全仓（**cache-cold**）：stdlib 39 / z42c 1 / 语料 54（14 文件为本检查独有）
- [x] 0.6 六个根因逐条溯源，确认**零真实用户类型错误**
- [x] 0.7 核实 R1 无需格式 bump（tp 块已含型参名，现读弃）

## 阶段 1: R1 —— 跨包型参身份（79 条 / 84%）
- [ ] 1.1 `ZbcReader.z42:531-541` 捕获 tp 块型参名 → `SigEntryZ.TypeParams[]`（仿 `TypeParamCount`）
- [ ] 1.2 `ExportedTypes.z42` `ExportedMethodZ` 增 `TypeParams[]`（**ctor 元数不变**，构造后赋值——
      旧种子 ABI 约定，同 `ParamsFrom`/`IsSealed`/`TypeParamCount`）
- [ ] 1.3 `TsigReconcile.z42` 把型参名从 `SigEntryZ` 传到 `ExportedMethodZ`
- [ ] 1.4 `ImportedSymbolLoader.z42:296/297` —— 自由/静态方法路径改传方法级型参表（现为 `_resolve/2` 空表）
- [ ] 1.5 `ImportedSymbolLoader.z42:348/349` —— 类方法路径把方法级型参**并入**类级 `tps`
- [ ] 1.6 回归用例：跨包 `Array.Copy(byte[], byte[], int)`；跨包泛型接口 `IBasicCollection<int>.AddOne(1)`；
      跨包 `Action<int>` / `Func<int,bool>` 传参（覆盖 `delegates/` 簇）
- [ ] 1.7 **自举字节对账**：型参从 `Z42ClassType("T")` 变 `Z42GenericParamType` 会否改动签名键
      （`OverloadResolver.TypeKey` 走 `Canon(t.Name())`，两者 `Name()` 都是 `"T"` → 预期键不变）——
      须**实证**，不得靠推理

## 阶段 2: R2 / R7 —— 赋值上下文已可复现的既存 bug
- [ ] 2.1 R2：`Conversion._classifyBuiltin` 加「任意数组 → `Array`」→ `ImplicitRef`
- [ ] 2.2 R2 回归：`Array a = new int[3];`（var-decl）+ `TakeArr(new int[3])`（argument）
- [ ] 2.3 R7：`Z42Type.z42:304-307` `Z42FuncType.IsAssignableTo` 改按 `Canon` 逐位比（对齐 `Z42ArrayType`）
- [ ] 2.4 R7 回归：`Func<int>` ↔ `Func<Int32>`；`Action` ↔ `Action`；`closure_l3_loops.z42:58` 转绿

## 阶段 3: R3 / R5 / R6
- [ ] 3.1 R3：`ImportedSymbolLoader.z42:376` 限定类型名不再固化成字面量 `"unknown"`
      （与欠债表 bug B3 同一处；`StubEmitter.z42:114` 用已解析名是**正确样板**）
- [ ] 3.2 R5：lambda 实参纳入 target-typed 延迟绑定通道（design D3 选项 B）
      —— `MemberResolver._bindCall:345-349` 留 `null` 占位，`BindArgsToSignature` 按形参类型回填
- [ ] 3.3 R5 风险核：全仓 grep 是否存在「同 arity ≥2 重载 + lambda 实参」的调用（会落 E0437）
- [ ] 3.4 R5 副作用确认：lambda 的 IR 类型标注由 `-> unknown` 变真实类型 →
      **golden `.zbc` 基线可能需 regen**，自举不动点须重新确认
- [ ] 3.5 R6：enum ↔ 底层整数（**按 Q2 裁决**实施：隐式双向 / 要求显式 cast）

## 阶段 4: 接线开检查 + 负例门
- [ ] 4.1 `FillDeferredArgs` → 更名 `BindArgsToSignature`，21 处调用点同步
- [ ] 4.2 加逐位检查：`sig!=null` 时对定长位调 `CheckImplicitConvert(arg, pt, syms, rawArgs[i].Span, "argument")`
- [ ] 4.3 `params` 尾位按元素类型逐位检查；定长段 `[0, ParamsFrom)`
- [ ] 4.4 检查须在 `BoxArgs` / `_withParamsExpansion` **之前**
- [ ] 4.5 **不短路**：同一调用多个不符实参逐条报
- [ ] 4.6 构造器：`ConstructTyper` 解析出 ctor `MethodSymbol` 后调同一检查（design D4）
- [ ] 4.7 新增 `z42c.semantics/tests/typecheck/argument_type/argument_type_tests.z42`
      —— spec 场景逐条覆盖（free/static/instance/ctor × 不符/窄化/常量例外/上转/装箱/接口）
- [ ] 4.8 🔴 **测试必须用 `DumpBody` / `collectDiags` 断言，禁止只用 `SemanticDump.FirstErrorCode`**
      —— 后者从不合并 collector 诊断，会造出**空门**（[[add-associated-types-program]] 实测踩过：
      7 条单测退回改动后仍 7/7 全绿）
- [ ] 4.9 反向自检：**把本次改动整体退回，新增测试必须全红**（否则就是空门）

## 阶段 5: 验证与归档
- [ ] 5.1 `cargo build --release` —— z42vm
- [ ] 5.2 完整 `xtask test` 全绿（e2e / cross-zpkg / stdlib / compiler / vscode-syntax）
- [ ] 5.3 自举不动点：gen1 == gen2 字节
- [ ] 5.4 `xtask test bootstrap` —— 无新语法/无格式改动，上一 nightly 应照常编过
- [ ] 5.5 `xtask test stdlib --mode jit` —— R5 改动 lambda 类型标注，须验 JIT 面
      （本地 `xtask test` **只跑 interp**，见 [[local-green-misses-jit-and-lines]]）
- [ ] 5.6 文档同步：`docs/book/src/compiler/source-compile.md` 增「调用实参类型检查」节——
      汇聚点、判定门与赋值同源、**残留洞清单**（design D5 四条）
- [ ] 5.7 `docs/book/src/compiler/` 页头「对齐」日期刷新
- [ ] 5.8 归档 `changes/` → `archive/YYYY-MM-DD-add-argument-type-check/`，tasks 标 🟢，**随 PR 一起提交**
- [ ] 5.9 更新 memory `restore-emit-zbc-diagnostics-program.md`：⑤ 完成、欠债分母重算

## 备注

- **本变更不开 P1 的门**（`--emit-zbc` 打印诊断）——那是本程序第 ⑧ 步，仍排最后。
  但量测阶段临时套用了 P1 补丁（`../z42-diagdebt`）才能看见单文件路径的诊断。
- **量测陷阱（务必记住）**：`z42c build` 跳过 cached 文件 → 不打印其诊断。任何"扫全仓数欠债"
  的动作**必须先清 `.cache` / `dist`**，否则得到的是低估（本轮首次扫描 37 → 冷跑 39，且 25 个包
  曾整包 cached）。
- **再次目击**：`xtask build compiler` 在 member / bootstrap 失败时仍打印 `✔` 并 exit 0
  （本轮 2 次）。memory 已记，值得单独立项。
