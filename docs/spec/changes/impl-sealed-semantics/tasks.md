# Tasks: sealed 修饰符语义强制 + 元数据 + 反射

> 状态：🟡 进行中 | 创建：2026-08-06 | 拆分：2026-08-07（④ 去虚化 → follow-up `add-sealed-devirt`）
> 分支/worktree：`impl-sealed-semantics` @ `/Users/d.s.qiu/Documents/codesigner-ui/z42-sealed`（基于 origin/main #137）
> 冷种子：`/Users/d.s.qiu/Documents/codesigner-ui/z42-sealed-seed`（nightly，Z42_HOME）

## 进度概览
- [x] 阶段 1: ① sealed 语义强制（继承 / override）—— 代码
- [x] 阶段 2: ③ 方法 sealed 简写 + 校验 —— 代码
- [x] 阶段 3: ② 方法级 sealed 位 + 格式 bump + 反射 + 跨包 —— 代码
- [ ] 阶段 4: 构建 + 测试 + fixture 重生（GREEN）
- [ ] 阶段 5: 文档同步 + 归档

## 阶段 1: ① sealed 语义强制（代码 ✅）
- [x] 1.1 `Z42Type.z42`：`Z42ClassType.IsSealed`
- [x] 1.2 `Symbol.z42`：`MethodSymbol.IsSealed`
- [x] 1.3 `DiagnosticCodes.z42`：E0427/E0428/E0429
- [x] 1.4 `SymbolCollector`：本地类/方法标 IsSealed
- [x] 1.5 `SymbolCollector._passSealedEnforce` + `_nearestBaseMethod` + 接入 3 条 collect 路径（继承 E0427 / override sealed E0428）

## 阶段 2: ③ 方法 sealed 简写（代码 ✅）
- [x] 2.1 `SymbolCollector` 2 处 override 识别点（:268/:319）认 sealed → 简写参与槽对齐
- [x] 2.2 `_passSealedEnforce`：sealed 无匹配基类 virtual → E0429
- [x] 2.3 `IrGenFacts._methodFlags`：sealed 连带 virtual 位（bit0）+ bit2

## 阶段 3: ② 方法级 sealed 位 + 格式 bump + 反射 + 跨包（代码 ✅）
- [x] 3.1 `bytecode.rs`：`METHOD_FLAG_SEALED = 1<<2`
- [x] 3.2 `zbc_reader.rs`：`ZBC_VERSION_MINOR` 30 / `ZPKG_VERSION_MINOR` 35 + changelog
- [x] 3.3 `ZbcFormat.z42` Minor 30 / `ZpkgWriter.z42` Minor 35
- [x] 3.4 `reflection.rs`：`MethodInfo.IsSealed`（两处构造点）+ `MethodInfo.z42` 字段
- [x] 3.5 `ExportedTypes.z42`：`ExportedClassZ.IsSealed` / `ExportedMethodZ.IsSealed`（post-construction）
- [x] 3.6 `TsigReconcile.z42`：从 `(cd.Flags&2)` / `(f.MethodFlags&4)` 提取 sealed 入 TSIG
- [x] 3.7 `ImportedSymbolLoader.z42`：还原到 `Z42ClassType.IsSealed` / `MethodSymbol.IsSealed`（3 处）

## 阶段 4: 构建 + 测试 + fixture（GREEN）
- [x] 4.1 冷种子建 z42c（`Z42_HOME=<seed>` cold build compiler）+ stdlib —— 本地 0.34-pin 全通过
- [x] 4.2 `cargo build --release`（z42vm）无错 —— 0.34 + 0.35 reader 均编译通过
- [ ] 4.3 **格式 bump fixture 重生（CI 完成）**：0.35 fixture 需两代自举，撞 macOS 本地墙（memory `escape-stack-format-bump-ci-learnings`）→ 由 CI ci-bootstrap 两代自举重生（必要时加临时 CI 步骤重生+回写，见 version-bumping.md 步骤 4/5/9）。**本地无法验，以 CI 为准**（workflow 阶段 8 冷路径规则）
- [x] 4.4 测试用例（6 semantics 报错/简写 + 1 反射）+ `examples/sealed.z42` —— 本地 0.34 全通过
- [ ] 4.5 `cargo test --test zbc_compat` / `cargo test lazy_loader` —— CI（依赖 4.3 的 0.35 fixture）
- [x] 4.6 本地逻辑验证：semantics 26/26 + z42c 自举不动点 gen1==gen2 5/5 + 反射（用本 change z42c 编译时通过）；**完整 0.35 `xtask test` 以 CI 为准**
- [x] 4.7 spec scenarios 逐条覆盖确认 —— 见 collect_tests 6 例 + method_sealed_flag

> **验证状态小结**：逻辑（①②③+跨包+反射）本地全绿；格式 bump（zbc 1.30/zpkg 0.35）的 fixture 重生 + 完整 GREEN 因 macOS 两代自举本地墙 → **CI 判定**（提交+推分支，走 CI ci-bootstrap）。

## 阶段 5: 文档 + 归档
- [ ] 5.1 `docs/book/src/language/sealed.md`（NEW）+ 挂 SUMMARY.md：语义 + shorthand；去虚化标 Deferred
- [ ] 5.2 `docs/design/runtime/zbc.md` / `zpkg.md` changelog（version-bumping 步骤 3/8）
- [ ] 5.3 `z42c.semantics/README.md` 功能索引 + 关联 change
- [ ] 5.4 `docs/roadmap.md` Deferred Backlog Index：sealed 去虚化（follow-up `add-sealed-devirt`）
- [ ] 5.5 归档 doc-check 清单（触发矩阵 / 死链 / 命令面 grep）
- [ ] 5.6 归档 move → `docs/spec/archive/YYYY-MM-DD-impl-sealed-semantics/`

## 备注
- **两-nightly 纪律**：本 change 不在 z42c/stdlib/xtask 源码写 sealed；仅落 support。use 属下一 change。
- **④ 去虚化拆出**：follow-up `add-sealed-devirt`。地基已备：类级 `CLASS_FLAG_SEALED`（既有）、方法级 `METHOD_FLAG_SEALED`（本 change）、`Z42ClassType.IsSealed`/`MethodSymbol.IsSealed`（本地+跨包，本 change）。去虚化落点 `ExprEmitter._emitCall`（:651）+ `EmitContext`（新增目标解析）。
- **格式 bump 无源码 use**：本 change 无方法写 sealed，method_flags bit2 实际全 0，golden 变化仅 header minor 字段。
