# Tasks: 去虚化扩到 sealed override + 泛型 sealed

> 状态：✅ DONE（PR #149）| 创建：2026-08-09
> 分支/worktree：`extend-sealed-devirt-more` @ `/Users/d.s.qiu/Documents/codesigner-ui/z42-sealed`（rebase 到 origin/main 038c29e4——含 #150 REPL 缩进下沉 + #151 target-typed new）
> follow-up of `add-sealed-devirt`（#142）+ `extend-sealed-devirt-imported`（#147）
> 落地：一分支两 commit——commit1=① sealed override，commit2=② 泛型 sealed

## 进度概览
- [x] commit1 ① sealed override：`DevirtReceiverClass` + `ResolveSealedTarget(classSealed)` 方法级门控 + 单测 + e2e（GREEN 5/5 不动点）
- [x] commit2 ② 泛型 sealed：`Z42InstantiatedType` 解包 + `_classShortName` `$N` 条件 mangle + 单测（3 PASS）+ e2e
- [~] commit0 fix：incomplete_at_eof 补 using Z42.Core（pre-existing E0436，#146 遗留）——**rebase 时发现 #150 已修同一 E0436（先来后到），本 commit 冗余、rebase 时 skip 掉**（隔离已确认非本 change 引入）
- [x] rebase origin/main（#150/#151）+ 重跑完整 GREEN（§3）

## commit1 ① sealed override
- [x] 1.1 `SealedReceiverClass`→`DevirtReceiverClass`：删 `!ct.IsSealed` 铁律（接纳非 sealed 类）
- [x] 1.2 `ResolveSealedTarget` 加 `bool classSealed`；declClass（本地/imported 两分支）门控 `classSealed || ms.IsSealed`，皆非 → `""`
- [x] 1.3 ExprEmitter 调用点：`DevirtReceiverClass` + 传 `sc.IsSealed`
- [x] 1.4 单测 `test_sealed_override_devirt`（非 sealed 类 sealed override → `call @Mid.`）+ `test_nonsealed_override_stays_vcall`（非 sealed override → vcall）
- [x] 1.5 e2e `sealed_override_devirt.z42`（去虚化结果==虚派发 + 子类继承 sealed 方法 + 非 sealed override 多态）
- [ ] 1.6 GREEN：`test compiler`（单测 + 自举不动点）+ `test e2e` + 回归 #142/#147 用例
- [ ] 1.7 commit1

## commit2 ② 泛型 sealed
- [x] 2.1 `DevirtReceiverClass` 接纳 `Z42InstantiatedType` → 解包 `.Def`；删泛型铁律
- [x] 2.2 新增 `_classShortName(ct)` 镜像 `IrGen._classIrShortName`（**条件** `$N`：仅 `Symbols.HasClass(Name$N)` 多 arity 才 mangle）；`ResolveSealedTarget` 目标名 + `_devirtQualifiable` ImportedClassNs 查键 + `TrackImportedClass` 全用它
  - **关键发现**：LocalClasses 用**裸名**键、ImportedClassNs 用**条件 mangle 短名**键（`ImportedSymbolLoader` clKey）；泛型 mangle 非「一律 $N」而是「多 arity 才 $N」
- [x] 2.3 imported 泛型 sealed：`_depHasFunction` FQ 复用 `_classShortName`（同名构造）
- [x] 2.4 单测：`test_generic_sealed_devirt`（单 arity → `call @Box.`）/ `test_generic_sealed_multiarity_devirt`（`call @Box$1.`）/ `test_generic_nonsealed_stays_vcall`（3 PASS）
- [x] 2.5 e2e `sealed_generic_devirt.z42`（`Box<int>`/`Box<string>` 去虚化 + 非 sealed 泛型多态）
- [x] 2.6 GREEN（含自举不动点 gen1==gen2）— 5/5 packages 逐字节复现（`$N` 条件 mangle 精确）
- [x] 2.7 commit2（`dd2a7a49`）

## 阶段 3: 文档 + 归档
- [x] 3.1 完整 `xtask test` 全绿（self-host 5/5 + z42c 21/21 + stdlib 280/280 + e2e 246 + vscode-syntax）
- [x] 3.2 `docs/book/src/language/sealed.md`：去虚化边界更新（+ sealed override + 泛型两节 + Deferred 清空 sealed 线）
- [x] 3.3 `docs/roadmap.md` Deferred：sealed override + 泛型 sealed 落地
- [x] 3.4 `z42c.semantics/README.md` ExprEmitter 行补
- [x] 3.5 归档 → `docs/spec/archive/2026-08-09-extend-sealed-devirt-more/`

## 备注
- **无格式 bump**（复用 CallInstr + 既有 `Deps.Statics` 索引；`MethodSymbol.IsSealed` 已是 #140 序列化字段）。种子 0.35 → 本地直接验。
- **正确性铁律**：不确定即回落 VCall（每个 `""` 分支虚派发永远正确）。② 目标名逐字节错=静默 miscall/自举炸 → 单独 commit、e2e + 不动点为门。
- **自举不动点**：z42c 自身用大量 sealed/sealed override → 本 change 改自编译输出 → 当次 gen1≠gen2（D7），warm 重建自愈。
