# Tasks: 泛型 arity 重载过滤 + 配对排序

> 状态：🟢 完成 | 创建：2026-09-03 | 归档：2026-09-03 | 无格式 bump

## 进度概览
- [x] 阶段 1: TypeParamCount 捕获 + 上浮（z42.ir）
- [x] 阶段 2: MethodSymbol 侧填充（local + imported）
- [x] 阶段 3: `_resolveOverload` 防御性 arity 过滤 + 7 调用点
- [x] 阶段 4: `Array.Sort<TKey,TValue>` + 测试
- [x] 阶段 5: 验证 + 文档同步

## 阶段 1: 捕获 + 上浮
- [x] 1.1 `SigEntryZ.TypeParamCount` + `_readSigs` 捕获 tpc
- [x] 1.2 `ZpkgReader.ReadModuleSigs`：`stub.TypeParamCount = tpc`
- [x] 1.3 `ExportedMethodZ.TypeParamCount`（post-ctor，ABI 安全）
- [x] 1.4 `TsigReconcile._methodFromSig`：`em.TypeParamCount = f.TypeParamCount`

## 阶段 2: MethodSymbol
- [x] 2.1 `MethodSymbol.TypeParamCount`
- [x] 2.2 `SymbolCollector`：`ms.TypeParamCount = mtpc`（本地）
- [x] 2.3 `ImportedSymbolLoader`：`sym.TypeParamCount = me/m.TypeParamCount`（2 处，跨包）

## 阶段 3: 决议器
- [x] 3.1 `_resolveOverload` 加 `typeArgCount` 形参 + 防御性 arity 过滤（非空才采用）
- [x] 3.2 7 个调用点传 typeArgCount（有 call 传 call.TypeArgCount，否则 0）

## 阶段 4: feature + 测试
- [x] 4.1 `Array.Sort<TKey,TValue>` + `_mergeSortPaired`
- [x] 4.2 `test_sort_paired` / `test_sort_paired_noop_small` + comparator 回归验证
- [x] 4.3 README Array 行

## 阶段 5: 验证
- [x] 5.1 z42.core 快信号绿（comparator descending PASS + paired PASS + noop）
- [x] 5.2 完整 `xtask test` **GREEN — all stages** + 自举不动点 **3/3 gen1==gen2**
- [x] 5.3 spec scenario 逐条覆盖确认

## 备注
- 关键正确性保证 = 防御性过滤（不误删全部候选），counts 缺失时退回原行为不回归。
- 无格式 bump（SIGS tp 块早已存，仅捕获上浮）。
