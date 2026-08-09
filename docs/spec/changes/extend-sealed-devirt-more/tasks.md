# Tasks: 去虚化扩到 sealed override + 泛型 sealed

> 状态：🟡 IMPL | 创建：2026-08-09
> 分支/worktree：`extend-sealed-devirt-more` @ `/Users/d.s.qiu/Documents/codesigner-ui/z42-sealed`（基于 origin/main 6178e7b7，含 #142/#147 devirt）
> follow-up of `add-sealed-devirt`（#142）+ `extend-sealed-devirt-imported`（#147）
> 落地：一分支两 commit——commit1=① sealed override，commit2=② 泛型 sealed

## 进度概览
- [ ] commit1 ① sealed override：`DevirtReceiverClass` + `ResolveSealedTarget(classSealed)` 方法级门控 + 单测 + e2e
- [ ] commit2 ② 泛型 sealed：`Z42InstantiatedType` 解包 + `$N` mangle 目标名 + 单测 + e2e

## commit1 ① sealed override
- [x] 1.1 `SealedReceiverClass`→`DevirtReceiverClass`：删 `!ct.IsSealed` 铁律（接纳非 sealed 类）
- [x] 1.2 `ResolveSealedTarget` 加 `bool classSealed`；declClass（本地/imported 两分支）门控 `classSealed || ms.IsSealed`，皆非 → `""`
- [x] 1.3 ExprEmitter 调用点：`DevirtReceiverClass` + 传 `sc.IsSealed`
- [x] 1.4 单测 `test_sealed_override_devirt`（非 sealed 类 sealed override → `call @Mid.`）+ `test_nonsealed_override_stays_vcall`（非 sealed override → vcall）
- [x] 1.5 e2e `sealed_override_devirt.z42`（去虚化结果==虚派发 + 子类继承 sealed 方法 + 非 sealed override 多态）
- [ ] 1.6 GREEN：`test compiler`（单测 + 自举不动点）+ `test e2e` + 回归 #142/#147 用例
- [ ] 1.7 commit1

## commit2 ② 泛型 sealed
- [ ] 2.1 `DevirtReceiverClass` 接纳 `Z42InstantiatedType` → 解包 `.Def`；删泛型铁律
- [ ] 2.2 `ResolveSealedTarget` 目标名 `$N` mangle（`QualifyClass(Name)+"$"+GenericParamCount`）；核对 `_devirtQualifiable` 键（Name vs Name$N）
- [ ] 2.3 imported 泛型 sealed：`_depHasFunction` FQ 用 `$N` 名
- [ ] 2.4 单测：泛型 sealed receiver → `call @Box$1.`；非 sealed 泛型对照仍 vcall
- [ ] 2.5 e2e：泛型 sealed 去虚化正确
- [ ] 2.6 GREEN（含自举不动点 gen1==gen2）
- [ ] 2.7 commit2

## 阶段 3: 文档 + 归档
- [ ] 3.1 完整 `xtask test` 全绿
- [ ] 3.2 `docs/book/src/language/sealed.md`：去虚化边界更新（+ sealed override + 泛型）
- [ ] 3.3 `docs/roadmap.md` Deferred：sealed 线两项落地
- [ ] 3.4 `z42c.semantics/README.md` ExprEmitter 行补
- [ ] 3.5 归档 → `docs/spec/archive/2026-08-09-extend-sealed-devirt-more/`

## 备注
- **无格式 bump**（复用 CallInstr + 既有 `Deps.Statics` 索引；`MethodSymbol.IsSealed` 已是 #140 序列化字段）。种子 0.35 → 本地直接验。
- **正确性铁律**：不确定即回落 VCall（每个 `""` 分支虚派发永远正确）。② 目标名逐字节错=静默 miscall/自举炸 → 单独 commit、e2e + 不动点为门。
- **自举不动点**：z42c 自身用大量 sealed/sealed override → 本 change 改自编译输出 → 当次 gen1≠gen2（D7），warm 重建自愈。
