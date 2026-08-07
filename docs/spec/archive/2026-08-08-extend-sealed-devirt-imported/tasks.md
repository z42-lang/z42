# Tasks: 去虚化扩到 imported sealed 类

> 状态：🟡 DRAFT（待 6.5 gate）| 创建：2026-08-07
> 分支/worktree：`add-sealed-devirt-imported` @ `/Users/d.s.qiu/Documents/codesigner-ui/z42-devirt-imp`（基于 origin/main 2addb5f5，含 #142 devirt v1）
> follow-up of `add-sealed-devirt`（#142）

## 进度概览
- [x] 阶段 1: 守卫放宽（EmitContext `_devirtQualifiable`）+ **Deps FQ 校验（`_depHasFunction`，实现期发现的必需项）**
- [x] 阶段 2: 测试（cross-zpkg e2e ✓；imported codegen 单测**不适用**，见 2.2）
- [x] 阶段 3: 文档 ✓ + 验证（self-host 5/5 + stdlib 280 + cross-zpkg 9/9 全绿）+ 归档

## 阶段 1: 核心（EmitContext）
- [x] 1.1 `_devirtQualifiable(name)` 私有助手 = in LocalClasses 或 in ImportedClassNs
- [x] 1.2 `SealedReceiverClass`：`LocalClasses.ContainsKey` → `_devirtQualifiable`
- [x] 1.3 `ResolveSealedTarget`：循环内 `LocalClasses.ContainsKey` → `_devirtQualifiable`
- [x] 1.4 **（实现期新增）`_depHasFunction(fq)` + imported 定义类候选返回前 Deps.Statics 校验 FQ 真实发射**
  ——排除 TSIG 展平的继承方法（否则 `Demo.Sld.Leaf.Tag` 这类未发射名 → 运行期 undefined function）。
  本地类 `LocalClasses` 分支先行短路返回；imported 未命中 Deps → 沿基链继续上溯。见 design Decision 2.5。

## 阶段 2: 测试
- [x] 2.1 cross-zpkg e2e `sealed_devirt_imported`：demo.base（Shape/Tagged virtual）× demo.sld（sealed Circle
  override Area + sealed Leaf **跨包继承** Tagged.Tag 不 override）× demo.app 精确类型调用 → 25/100/7/9 全对。
  **就是它抓到 TSIG 展平坑**（修前 `undefined function Demo.Sld.Leaf.Tag`）。
- [~] 2.2 codegen 单测（imported）：**不适用**——`IrDump.DumpFuncOpt` 是**无依赖单源** IR 文本 dump，无法引用
  imported 类；无 deps-aware 文本 dump（新增即 IrDump 越界）。imported 去虚化以 **cross-zpkg e2e（2.1）**为门。
  #142 的本地 `codegen_tests`（`test_sealed_devirt_*`）保留、仍绿（本地路径零改动）。
- [x] 2.3 回归：#142 本地 devirt 用例 + 自举不动点（gen1==gen2）+ stdlib 280/280 仍绿。
- [x] 2.4 泛型 imported sealed → `SealedReceiverClass` 非泛型铁律不变 → 仍 vcall（边界回落）。

## 阶段 3: 验证 + 文档
- [x] 3.1 完整 `xtask test`——self-host 不动点 5/5 gen1==gen2 + #142 codegen PASS + stdlib 280/280 + cross-zpkg 9/9 全绿
- [x] 3.2 spec scenarios 逐条覆盖（含新增「跨包继承基链」+「Deps 校验」scenario）
- [x] 3.3 `docs/book/src/language/sealed.md`：v1 边界「仅本地」→「本地 + imported」+ TSIG 展平坑与 Deps 解法专节
- [x] 3.4 `docs/roadmap.md` Deferred：imported 落地，保留泛型/sealed-override
- [x] 3.5 `z42c.semantics/README.md` ExprEmitter 行补 imported + Deps 校验
- [x] 3.6 归档 doc-check + move → `docs/spec/archive/2026-08-08-extend-sealed-devirt-imported/`

## 备注
- **无格式 bump**（复用 CallInstr + 既有 `Deps.Statics` 索引，无新字段）。种子已追上 0.35 → 本地直接验，无需 0.34-pin。
- **正确性铁律**：imported 目标名错=静默 miscall；主门 = cross-zpkg e2e（输出对拍）；imported 定义类必过 Deps FQ 校验；不确定即回落 VCall。
- **本地路径零改动**（`LocalClasses` 分支先行短路）→ #142 回归风险低（实测 codegen 单测 + 自举不动点仍绿）。
- **关键教训**：imported 符号的 `Methods` 是 **TSIG 展平**的（含继承方法）≠ 本地「只声明」语义；任何「用 imported
  类 Methods 命中当作声明」的逻辑都要用 Deps FQ 校验或 OwnMethod 元数据兜底。
