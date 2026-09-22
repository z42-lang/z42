# Tasks: 硬转换 `(T)x` 失败时正确报错

> 状态：🟡 规范就绪，待实施 | 创建：2026-09-22
> 分支/worktree：待开 | 基于：origin/main #742
> 类型：`lang` + `vm`（**无格式 bump** —— 编译期降解，用现有指令）
> 授权：User「持续实现，PR 变绿自动合并」= 阶段 6.5 批量授权

## 进度概览
- [ ] 阶段 1: `InvalidCastException` + 指纹
- [ ] 阶段 2: `BoundCast.IsHardCast` 分流
- [ ] 阶段 3: 发射层降解「先查后转」+ 静态可判时省略
- [ ] 阶段 4: **摸底** —— 现存代码有没有依赖「硬转换不抛」
- [ ] 阶段 5: 运行期两条 `bail!` 改真异常
- [ ] 阶段 6: 用例（interp + jit）+ GREEN + PR

## 阶段 1
- [ ] 1.1 `Exceptions/InvalidCastException.z42`（与既有 18 个同形）
- [ ] 1.2 `CacheStore.CompilerFingerprint++`（新增 stdlib 公开类 ⇒ zpkg 字节变、格式不变）

## 阶段 2
- [x] 2.1 ~~加 IsHardCast~~ —— 实测 `as` → `BoundCast`、`(T)x` → `BoundConvert`，**本来就分开**
- [x] 2.2 全仓唯一的 `new BoundCast` 在 `TypeOpTyper._bindAsExpr`（已 grep 确认）

## 阶段 3
- [ ] 3.1 `TypeOpEmitter._emitConvert`（**不是 `_emitCast`**）：分支 ② `fromIr == toIr` 与
      ③ `toIr == Ref/Unknown` 现在**什么都不发** ⇒ 在这两处按需插 `IsInst` + 分支 + `Throw`
- [ ] 3.2 null 分流：目标值类型 → `NullReferenceException`；目标引用类型 → 放行
- [ ] 3.3 静态可判时省略检查（`Conversion.Classify` 为 Identity / 可赋）
- [ ] 3.4 核对 golden：`as` 与静态可判的 cast 应 byte-identical

## 阶段 4: 摸底
> 与 `enforce-value-type-non-null` 的 Q1 同法：先只加检查、跑**全量**（不是只 build stdlib
> ——那次「0 命中」是缓存骗的），看命中。
- [ ] 4.1 全仓跑，收集所有新抛出的点
- [ ] 4.2 逐个判读：是真 bug（该抛）还是需改写的合法写法
- [ ] 4.3 结论写回 proposal

## 阶段 5
- [ ] 5.1 `semantics.rs` 的 `bail!("InvalidCastException: …")` → 真异常（用户可读类型名）
- [ ] 5.2 确认反射 / 泛型擦除路径也走到真异常
- [ ] 5.3 JIT 侧：grep 是否另有 cast 快路；有则同步

## 阶段 6
- [ ] 6.1 e2e 用例覆盖 spec 全部场景，**interp + jit 双模式**
- [ ] 6.2 跨包接口不误抛的用例（cross-zpkg）
- [ ] 6.3 `xtask test all` + `cargo test --lib` + docs + examples
- [ ] 6.4 文档：`reference/language/conversions.md` 的失败语义
- [ ] 6.5 PR + auto-merge
