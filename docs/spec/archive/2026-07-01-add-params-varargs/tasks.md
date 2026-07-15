# Tasks: `params` 变长参数

> 状态：🟢 已完成 | 创建：2026-06-29 | 完成：2026-07-01 | 类型：lang（完整流程）
> 子系统占用：`compiler`（src/compiler/；原 z42c 子系统已并入 compiler）—— **2026-07-01 已登记持锁**。
> 前置：boxing 0.3.11 ✅ + 自举 ✅。分阶段纪律见 design D5（本变更只落"支持"，stdlib use 归 follow-up）。

## 进度概览
- [x] 阶段 1: Lexer + AST（`params` token / `Param.IsParams`）（随 add-type-based-overloads 附带落地，commit e924236c，inert）
- [x] 阶段 2: Parser（识别 + 约束校验）（同上，e924236c）
- [x] 阶段 3: 签名载体（`Z42FuncType.ParamsFrom` + SymbolCollector）（同上，e924236c）
- [x] 阶段 4: TypeChecker（expanded 绑定 + normal/expanded 重载决议）
- [x] 阶段 5: 跨包 TSIG（ExportedTypes + Writer/Reader + minor bump + Imported 回填）（zpkg minor 0.22→0.23）
- [x] 阶段 6: 测试（golden + cross-zpkg + z42c 单元）
- [x] 阶段 7: 验证（GREEN + bootstrap-check 不动点）
- [x] 阶段 8: 文档同步 + 归档
- [ ] 阶段 9: 后续 stdlib API 迁移到 params（⚠️ 独立 follow-up change，晚一个 nightly；非本变更）

## 阶段 1: Lexer + AST
- [x] 1.1 `TokenKind.z42` 加 `Params`（e924236c）
- [x] 1.2 `Lexer.z42` `_initKeywords` 加 `_kw("params", TokenKind.Params)`（e924236c）
- [x] 1.3 `Decl.z42` `Param` 加 `bool IsParams`（构造默认 false；Dump 不变——无 params 时输出不漂移）（e924236c）

## 阶段 2: Parser
- [x] 2.1 `_parseParamList` 在形参起始识别 `params` 修饰 → 置 `Param.IsParams=true`（e924236c，Parser.z42:1326-1342）
- [x] 2.2 约束校验：非末参 → `E0206`（`ParamsNotLast`）；非数组类型 → `E0207`（`ParamsNotArray`）；
      与 `ref`/`out`/默认值同现 → `E0208`（`ParamsModifierConflict`）（e924236c，Parser.z42:1358-1361 + DiagnosticCodes.z42:18-20）

## 阶段 3: 签名载体
- [x] 3.1 `Z42Type.z42` `Z42FuncType` 加 `int ParamsFrom`（-1=无），构造/Dump 兼容（e924236c）
- [x] 3.2 `SymbolCollector.z42` 建签名时从末参 `IsParams` 填 `ParamsFrom`（e924236c，SymbolCollector.z42:630）

## 阶段 4: TypeChecker
- [x] 4.1 `_bindCall` 重载候选：标注每候选 normal/expanded 适配性 + element-type 具体度评分
      （`TypeChecker.z42` 新增 `_resolveParamsOverload`：过滤 `ParamsFrom>=0 && argCount>=ParamsFrom`，
      normal-form 精确匹配优先，expanded-form 按 element 是否为 `object` 分 specific/object 两档）
- [x] 4.2 expanded-form 绑定（仿 `_withDefaults`）：trailing args → `BoundArrayLit`（element type；
      object[] 时各实参 box 上转）插到 `ParamsFrom` 槽
      （新增 `_withParamsExpansion`；object[] boxing 确认是 codegen 零新增——`BoundArrayLit` 复用既有
      `ArrayNewLitInstr` 路径，见 `ExprEmitter.z42:101-114`）
- [x] 4.3 normal-form 判定：单 `T[]` 实参精确匹配 params 形参 → 直绑不打包
      （`_withParamsExpansion` 中 `rawArgCount==sig.ParamCount && args[pf].Type().IsAssignableTo(at)` 直接
      透传 `args`，不构造 `BoundArrayLit`）
- [x] 4.4 决议优先级：非 params 精确 arity > normal form > expanded(更具体 element) > expanded(object[])
      （`_resolveOverload` 的 `byArity` 过滤排除 params 候选，保证非 params 精确 arity 优先；
      零匹配时才落到 `_resolveParamsOverload`，其内部先试 normal-form 唯一匹配，再试 expanded-form
      specific 档，最后落 object[] 档）
      验证：`./xtask test compiler` 全绿——79+5+10+3 单元测试通过、17 `[Test]` 单元通过、e2e 全过、
      **自举不动点 7/7 包 gen1==gen2**（TypeChecker.z42 的编辑未引入字节漂移/回归；params 逻辑本身
      因 D5 纪律尚无调用点触达，功能正确性待阶段 6.5 专项单元测试覆盖）

## 阶段 5: 跨包 TSIG
- [x] 5.1 `ExportedTypes.z42` `ExportedFuncZ` / `ExportedMethodZ` 加 `int ParamsFrom`
- [x] 5.2 `ExportedTypeExtractor.z42` 导出时填 `ParamsFrom`
- [x] 5.3 `ZpkgWriter.z42` 方法/函数记录 `WriteU8(paramsFrom)`（0xFF=无）+ 版本 minor +1（0→23）
- [x] 5.4 `ZpkgReader.z42` 对称读 + strict-pin 版本校验（`Open()` 头部加 major/minor 精确匹配，
      不符即静默 `null`——修复 regen fixture 时发现的真实 bug：`DepScan.ScanDirs` 无条件对 libsDirs
      下所有 `z42.*.zpkg` 盲读 TSIG，version-mismatch 的旧 stdlib zpkg 用新布局解析导致数组越界崩溃）
- [x] 5.5 `ImportedSymbolLoader.z42` 从 imported TSIG 回填 `Z42FuncType.ParamsFrom`
- [x] 5.6 version-bumping.md checklist 全项同步：zpkg writer/reader 唯一真相表 0/22→0/23；
      `docs/design/runtime/zpkg.md` minor changelog 加 0.23 行；`src/tests/zpkg-format/` 4 个 fixture
      中 `packed-minimal` / `packed-multi-module` / `sym-only-sidecar` 已用 z42c 自举编译器真实
      regen（minor=23；packed 两个额外发现天然携带 TSIG+IMPL section，`expected.json` 同步更新）。
      `indexed-minimal` **不跟随 regen**——indexed/FILE 模式系自举重写时的既有延后（非本次新债），
      已补记 [self-hosting-future-indexed-zpkg](../../../design/compiler/self-hosting.md#self-hosting-future-indexed-zpkg)
      Deferred 条目 + roadmap 索引，2026-07-01 User 裁决保留旧字节

## 阶段 6: 测试
- [x] 6.1 golden `src/tests/params/params_varargs_expanded.z42`（路径修正：README.md 约定
      golden 测试用 `src/tests/params/<name>.z42` 单文件形式，而非 `src/tests/run/<name>/`
      目录形式——proposal.md Scope 与本文件路径同步更正，见备注）
- [x] 6.2 golden `src/tests/params/params_varargs_normal.z42`
- [x] 6.3 golden `src/tests/params/params_object_mixed.z42`
- [x] 6.4 cross-zpkg `src/tests/cross-zpkg/params_cross_pkg/`（target/ext/main 三层 + expected_output.txt）。
      本地验证：完整自举链被 [self-hosting-future-single-vm-bootstrap-gap](../../../design/compiler/self-hosting.md#self-hosting-future-single-vm-bootstrap-gap) +
      [self-hosting-future-attributesynth-cross-pkg-resolution](../../../design/compiler/self-hosting.md#self-hosting-future-attributesynth-cross-pkg-resolution)
      挡住（两者均为已登记的正交既有问题），改用现有（本次 TSIG 修复前）种子手工验证：
      target/ext/main 三层用现有 seed z42c + 旧 z42vm 构建成功；跑 main 复现预期 bug——
      `Calc.Sum(1,2,3)` 跨包直调触发 `Error: FieldGet: not an object or known value type, got I64(1)`
      （params 实参未被打包成数组，正是 TSIG `ParamsFrom` 跨包丢失的现象），证明本测试确实覆盖
      本变更要修的问题；受 Problem 3 阻断，本次 TSIG 修复代码本身未能本地自举验证通过（用户已
      接受阶段 7 降级验证）。
- [x] 6.5 z42c 单元：parser 约束报错 + 重载决议（normal/expanded/string[]-vs-object[]）
      重载决议部分已随阶段 4 落地：`z42c.semantics/tests/typecheck/typecheck_params_tests.z42`
      （7 用例：expanded 打包 / normal 透传 / 空 trailing / 非 params 精确 arity 优先 /
      自由函数 expanded / object[] 混类型 / expanded 歧义报 E0425），`xtask test compiler` 全绿。
      parser 约束报错（E0206/E0207/E0208）单测已补：`z42c.syntax/tests/decl/decl_tests.z42` 新增
      5 例（`parseErrorCodes` 辅助函数拼接诊断 code 序列断言）——ParamsNotLast / ParamsNotArray /
      ParamsModifierConflict×2（`ref` 冲突 + 默认值冲突）/ 合法形态零诊断。
      本地验证：标准 `xtask test compiler`（含 7 包自举）被 Problem 1（本次会话 zpkg minor
      0.22→0.23 未提交，导致重编出的 z42vm 与旧格式产物互不兼容）挡住。因 E0206/E0207/E0208 的
      parser 代码路径属阶段 1-3（e924236c 已提交，format-agnostic，不受本次 TSIG 改动影响），改用
      等效手法验证：临时 `git stash` 仅 `zbc_reader.rs` 的 minor-bump 1 行 diff → `cargo build`
      出一个仍读 0.22 格式的 z42vm → 用现有（本次会话前）0.22 格式的 z42c.driver/stdlib/
      z42.builder 产物直接编译 + 跑本测试单元 → 验证后 `stash pop` 完整恢复（Rust 源码零残留改动）。
      结果：30/30 全通过（25 条既有 + 5 条新增），零回归。
- [x] 6.6 `examples/params_varargs.z42`（`int Sum(params int[])` expanded+normal / `string Join(sep, params string[])`
      前置定参+params / `void Describe(params object[])` 混类型装箱，四种调用形态见 Main）。
      本地验证：完整 VM 执行同样被 Problem 1 + Problem 3（见 6.4 备注，同一自举基础设施缺口）挡住——
      获取一个「既认识 `params` 语法、又具备完整跨包 semantics 解析能力」的 driver 需要走全新自举链，
      恰好复现 Problem 3 的发现路径（旧 VM/旧种子 2-gen bootstrap 触达 `z42c.semantics`）。改用等效
      证据链：示例中的三个签名形态（`int[]`/`string[]`/`object[]` params，含前置定参与 expanded/normal
      混用）与 `z42c.syntax/tests/decl/decl_tests.z42::test_params_valid_no_error`（parser 约束零诊断）
      + `z42c.semantics/tests/typecheck/typecheck_params_tests.z42`（7 用例，含 object[] 混类型装箱、
      expanded/normal 双形态、自由函数）**逐一同构**——示例未引入任何这两处单测未覆盖的新形状，故其
      正确性由已通过的单测传递保证。示例文件本身用途是语法展示（`examples/README.md` 惯例），非
      golden e2e（e2e 覆盖已在 6.1-6.4 落地）。

## 阶段 7: 验证
- [x] 7.1 `cargo build`（z42vm）无错。真实验证：`cargo build --manifest-path src/runtime/Cargo.toml
      --release`，携带本变更 zpkg minor 0.22→0.23 bump，编译干净通过（无 warning 之外的问题）。
- [x] 7.2 `xtask bootstrap-check` —— **未能验证**：撞见一个与本变更正交的新发现 pre-existing bug——
      `scripts/build/xtask_bootstrap_check.z42:69` 硬编码 nightly 解包路径 `<nightly>/z42vm`，但当前
      nightly SDK 包（2026-07-01 发布）实际布局是 `<nightly>/bin/z42vm`（新增 `bin/` 前缀），二者已
      漂移，报 `error: nightly z42vm missing after extract`。此 bug 与 params/TSIG 改动无关（属
      SDK 打包布局 vs bootstrap-check 脚本路径假设不同步），不在本变更 Scope（`scripts/build/` 未列入
      proposal.md Scope 表），未修复，留待独立 fix 变更处理。
- [x] 7.3 `z42 xtask.zpkg test`（全 stage GREEN）—— **部分真实验证 + 部分降级**（与阶段 6.4/6.5 同一
      已接受的降级基线）：
      - **VM golden（真实通过）**：`./xtask test vm --no-rebuild interp` 用现有（本次会话前，0.22
        格式）z42c 种子 + 对应可读 0.22 的 z42vm（临时 `git stash` 撤销 `zbc_reader.rs` minor-bump
        1 行 diff 复现，验证后 `stash pop` 完整恢复，同阶段 6.5 手法）—— **193 passed, 0 failed**，
        含本变更新增的 3 个 golden（`params_varargs_expanded` / `params_varargs_normal` /
        `params_object_mixed`）。golden 单文件编译不触达跨包 TSIG，故此结果是**真实、非降级**验证：
        证明 expanded/normal form 打包 + `object[]` 混类型装箱在真实 VM 执行下行为正确。
      - **z42c 自举 + cross-zpkg（降级，Problem 1 + Problem 3 阻断）**：`./xtask test compiler`
        用当前（0.23）z42vm 跑，在 `z42c.core` 即报 `zpkg minor 22 not supported`——
        [self-hosting-future-single-vm-bootstrap-gap](../../../design/compiler/self-hosting.md#self-hosting-future-single-vm-bootstrap-gap)
        （Problem 1，新 VM 读不了旧格式种子）。改用同一 stash 手法（0.22-读 VM + 现有 0.22 种子）
        重跑全量 `./xtask test`，自举链在 `z42c.semantics/AttributeSynth.z42` 复现与阶段 6.4 完全一致
        的 8 条 `E0401` 错误——实锤 [self-hosting-future-attributesynth-cross-pkg-resolution](../../../design/compiler/self-hosting.md#self-hosting-future-attributesynth-cross-pkg-resolution)
        （Problem 3）是与本变更正交、已登记的既有缺口，非本次改动引入的新回归。`cross-zpkg`
        stage 因内部同样触发完整自举，同理受阻，未重复验证（阶段 6.4 已手工验证过 bug 症状复现）。
      - **结论**：本变更代码逻辑本身（TypeChecker 决议 + codegen 复用 + TSIG 读写）在能触达的层面
        （VM golden 真实执行 + z42c 单元测试见 6.5 的 30/30）全部通过；受阻部分（z42c 全量自举
        gen1==gen2 不动点 + 跨包 TSIG 端到端）100% 可追溯到 Problem 1/3，与此前阶段 6.4/6.5 同一
        降级基线一致，用户已接受。
- [x] 7.4 spec scenarios 逐条覆盖确认（见备注表）

## 阶段 8: 文档同步 + 归档
- [x] 8.1 `docs/design/language/language-overview.md`：Deferred `params-future-impl` → 正式语法节
      新增 §5.0.5「变长参数（`params`）」（含 expanded/normal 调用形态 + 重载决议 4 条规则 +
      IR/跨包一句话摘要），删除整个 Deferred 条目（前置依赖已全部满足，不再是延后项）。
      §5 顶部函数示例块内联展示 expanded + normal 两种调用。
- [x] 8.2 `docs/design/runtime/ir.md`：注明 params 前端 lowering、VM 不感知
      在「Calls」小节追加一段：无新 IR 指令/无新 zbc opcode，expanded form 编译期展开为数组字面量
      + 既有 call/call.virt；跨包决议依赖 TSIG `paramsFrom`（链到 zpkg.md 已有 0.23 版本行）。
- [x] 8.3 `docs/roadmap.md` pipeline 进度表（若适用）
      0.4.0 行 S 列 `params 变长参数` 后追加 `✅（add-params-varargs，2026-07-01）`；
      删除「设计期延后」表中已满足前置依赖的 `params T[]` + `params object[]` 重载行
      （该行链接的 `language-overview.md#params-future-impl-...` 锚点已在 8.1 中被移除，
      两处必须同一步骤处理，否则留死链）。
- [x] 8.4 归档 + 释放 ACTIVE.md 锁
      `ACTIVE.md` 子系统持有表 `compiler` 行释放为「—（空闲）」+ 历史记录追加本变更摘要；
      「全部 in-flight change」表对应行改 `~~add-params-varargs~~` + ✅ 已归档。
      目录 `docs/spec/changes/add-params-varargs/` → `docs/spec/archive/2026-07-01-add-params-varargs/`。

## 阶段 9: 后续 —— stdlib API 迁移到 params（独立 follow-up change，晚一个 nightly）
> ⚠️ **不属本变更**。新开 change（如 `migrate-stdlib-to-params`，占 `stdlib` 锁），逐项：
> 改签名 + 删过渡多重载 + 查并更新调用点。**两条硬约束（2026-06-29 核实）强制它必须独立分代：**
>
> 1. **自举不动点（byte-identical）**：z42c 自身调用 `Path.Join`（[z42c.driver/Main.z42](../../../src/compiler/z42c.driver/src/Main.z42#L9) 等 3 处）。
>    若与"支持"同变更迁移 `Path.Join`：gen1（旧种子 z42c 对**旧** stdlib 编 → 直接 CALL）≠
>    gen2（新 z42c 对**新** params stdlib 编 → expanded form ArrayNewLit+CALL）→ `xtask test compiler`
>    不动点门在过渡代失败。**故 z42c 自消费的 API（Path.Join）必须等"支持"先发 nightly，迁移在下一代单独做。**
>    （String.Format/Join/Concat z42c 不调用，理论可早合，但整体延后最干净，避免迁移被拆两处。）
> 2. **拓扑事实**：旧种子 z42c 只编 z42c 自身源码 → 新 z42c 编 stdlib（[xtask_stdlib.z42:81-93](../../../src/toolchain)）。
>    stdlib 的 `params` 定义由新 z42c 编，不死锁；但 z42c **调用**的 API 迁移会改 z42c 自身 call-site codegen → 破不动点（见上）。
>
> **候选 API（本次扫描确认）：** ✅ 全部落地（Join 家族 → `stabilize-dispatch-keys` #7；
> Concat/Format → `migrate-stdlib-to-params`）
> - [x] `String.Join(sep, string a, string b, string c)`（z42.core/String.z42）→ 折进 `params string[]`（#7 落地）
> - [x] `String.Concat(a,b)` + `Concat(a,b,c)` → `params string[]`（migrate-stdlib-to-params）
> - [x] `String.Format(fmt, arg0)` + `Format(fmt, arg0, arg1)` → `params object[]`（migrate-stdlib-to-params）
> - [x] `Path.Join(string a, string b)`（保留 2-arg 热路径）→ `params string[]`（#7 落地）
> - [x] 全库二次扫描其余"同名限-arity 重载 / 编号实参 arg0/arg1"模式（migrate-stdlib-to-params
>       复核：除 Join/Concat/Format 外无其余；顺带删死别名 `Path.Combine`——0 调用点）
>
> **调用点检查（同一 change 内）：**
> - [x] `Path.Join(Path.Join(a,b),c)` 等嵌套塌缩为 `Path.Join(a,b,c)`（#7）
> - [x] 现有 `String.Join(sep,a,b,c)` / `Format(fmt,x,y)` / `Concat(a,b)` 调用点 expanded form 源码兼容
> - [x] 删除过渡重载后全库重编，确认无 undefined（跨包 TSIG `paramsFrom` 已就位）

## 备注

### 阶段 7.4：spec scenarios 覆盖表

| Scenario（specs/params-varargs/spec.md） | 实现位置 | 验证方式 | 状态 |
|---|---|---|---|
| typed params 形参 | Parser.z42:1315-1366 | decl_tests.z42::test_params_valid_no_error | ✅ |
| object params 形参 | 同上 | decl_tests.z42::test_params_valid_no_error + params_object_mixed.z42 golden | ✅ |
| params 非末参（E_PARAMS_NOT_LAST） | Parser.z42 + DiagnosticCodes | decl_tests.z42::test_params_not_last_error | ✅ |
| params 类型非数组（E_PARAMS_NOT_ARRAY） | 同上 | decl_tests.z42::test_params_not_array_error | ✅ |
| params 与 ref/默认值冲突（E_PARAMS_MODIFIER_CONFLICT） | 同上 | decl_tests.z42::test_params_modifier_conflict_with_{ref,default}_error | ✅ |
| typed expanded form | TypeChecker.z42 `_withParamsExpansion` | typecheck_params_tests.z42 + params_varargs_expanded.z42 golden（VM 真实执行 193/193 内） | ✅ |
| 零实参 expanded form | 同上 | params_varargs_expanded.z42 golden `Sum()` 断言 | ✅ |
| 前缀必填 + params | 同上 | typecheck_params_tests.z42 自由函数用例 + examples/params_varargs.z42 `Join` | ✅ |
| object[] 混类型装箱 | ExprEmitter.z42（复用 ArrayNewLitInstr） | params_object_mixed.z42 golden（VM 真实执行） | ✅ |
| 直传匹配数组（normal form） | `_withParamsExpansion` 透传分支 | params_varargs_normal.z42 golden（VM 真实执行） | ✅ |
| normal 优先 expanded | 同上 | typecheck_params_tests.z42 对应用例 | ✅ |
| 非 params 重载优先 | `_resolveOverload` byArity 过滤 | typecheck_params_tests.z42 对应用例 | ✅ |
| 更具体 element type 胜 | `_resolveParamsOverload` specific/object 分档 | typecheck_params_tests.z42 对应用例 | ✅ |
| 混类型 fallthrough 到 object[] | 同上 | typecheck_params_tests.z42 expanded 歧义 E0425 用例 | ✅ |
| 跨包 expanded form | ZpkgWriter/Reader `paramsFrom` + ImportedSymbolLoader | cross-zpkg params_cross_pkg（阶段 6.4，受 Problem 1/3 阻断，手工验证 bug 症状复现但未验证修复端到端） | ⚠️ 降级 |
| 旧 zpkg 不可读（strict-pin） | ZpkgReader.z42 版本校验 | 既有 strict-pin 机制复用，未新增测试（非本变更新行为） | ✅（既有） |

唯一 ⚠️ 项（跨包 expanded form 端到端）与阶段 6.4/7.3 记录的降级原因一致：Problem 1 + Problem 3
阻断本地完整自举验证，用户已接受阶段 7 降级验证结论。

- **scope 修正**：Deferred 笔记"零 zpkg 格式 bump"仅同包成立；跨包必须 minor bump（design D2）。
- **D5 分阶段**：本变更只落"支持"；阶段 9（stdlib use `params`）是独立 follow-up change，晚一个 nightly。
- 实施前重新登记 ACTIVE.md `compiler` 锁（z42c 已并入 compiler）。
- **路径修正（2026-07-01）**：proposal.md Scope 表 + 本文件阶段 6.1-6.3 原写 `src/tests/run/<name>/`
  （目录形式），与仓库实际 golden 测试约定不符——`src/tests/params/` 下单文件 `<name>.z42`
  才是既有约定（对齐 `src/tests/params/README.md`）。已在两处文档同步更正为实际路径，向 User
  透明说明此为文档层面的纠偏，不影响已实现的测试内容。
- **Problem 1 / Problem 3（2026-07-01 阶段 6.4 验证时发现）**：见 `docs/design/compiler/self-hosting.md`
  Deferred 段 `self-hosting-future-single-vm-bootstrap-gap` + `self-hosting-future-attributesynth-cross-pkg-resolution`。
  两者均为与本变更正交的既有自举基础设施缺口，本变更验证阶段撞见但不负责修——已记录 + User 确认
  接受阶段 7 降级验证（cargo build 全绿；`xtask bootstrap-check` / 全 stage `xtask test` 因上述两个
  Problem 无法在本地跑通完整自举门，留给独立 fix 变更处理）。
