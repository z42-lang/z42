# Tasks: REPL 多行输入 + 声明累积（+ 跨包类型元数据修复）

> 状态：🟢 已完成 | 创建：2026-07-27 | 完成：2026-07-28 | 占用：`toolchain` + `compiler`（User 授权扩张 2026-07-28）

## 进度概览
- [x] 阶段 1: 多行输入（B1）
- [x] 阶段 2: 声明累积（B2，toolchain）
- [x] 阶段 3: 编译器跨包类型元数据修复（实例方法 world-extension + enum 导出）
- [x] 阶段 4: 全 GREEN + 自举不动点（gate 全绿 + canonical driver 匹配）
- [x] 阶段 5: 文档同步 + 归档

## 阶段 1: 多行输入（B1）
- [x] 1.1 `interactive_main.z42`：`ReadLine(">>> ")` → `ReadBlock(">>> ", "... ")`

## 阶段 2: 声明累积（B2，toolchain）
- [x] 2.1 `ScriptState.z42`：加 `DeclNames` + `DeclNamespaces` + 构造器初始化
- [x] 2.2 `Classifier.z42`（新）：`ParsedInput`（加 `IsDecl`/`DeclName`）+ `Classify`——跳过前导修饰符 → 类型关键字（class/struct/record/interface/enum）/ `<type> <ident> (` 函数 / `(var|type) <ident> =` var。从 `Script._classify` 迁出（Script.z42 控行数）
- [x] 2.3 `Script.z42` `Eval`：CachedScan 提前 + 声明轮分支 `_evalDecl`（重定义查重 → 编「prelude+声明原文」→ ExtendWithPackage + LoadBytes + 登记 → 不 Invoke）
- [x] 2.4 `Script.z42` `_basePrelude`：所有轮追加 `using` 全部声明 ns（+ 现有 Vars 类）；`_compileSrc`/`_compileRaw` 重构

## 阶段 3: 编译器跨包类型元数据修复（compiler，User 授权扩张）
> 实施期实测发现：REPL 声明的类**实例方法**与 **enum 类型**跨轮不 resolve（自由函数/构造/静态字段 OK）。
> 根因在 compiler 增量导入路径，超原 toolchain scope；User 裁决「直接修 compiler，一起搞定」。
- [x] 3.1 `z42c.pipeline/DepScan.z42` `ExtendWithPackage`：Rebuild 前把本包并入 `scan.Wp`（world）
      → `TsigReconcile._rebuildClass._locate` 定位类自身 + base 链 → 从 SIGS 读实例方法（此前 world
      不含增量包 → 导出 0 方法 → `no method`）。修实例方法。
- [x] 3.2 `z42.ir/TsigReconcile.z42` `_rebuildModule`：本地 enum 从 TYPE 段成员块重建 `ExportedEnumZ`
      导出（此前恒排除、只留内建 GCHandleType → 跨包 enum 全 `undefined`）。修 enum 类型跨包导入
      （一般能力，非仅 REPL）。无格式 bump（enum 数据已在 zbc TYPE 段）。
- [x] 3.3 warm-z42c 回路实测（driver + probe）：实例方法 `new Adder().add(10)=10`、类跨轮、enum 声明
      + 声明体内用（`pickColor()=Color.Green=1`）全通过；enum=long 常量语义与本地一致（`Color c=` 非法本地亦然）。

## 阶段 4: 全 GREEN + 自举不动点
- [x] 4.1 clean 重建（清 .cache → 全量 regen 重编 z42.ir/z42c.pipeline；实测增量缓存曾跳过改动文件，必清）
- [x] 4.2 `cargo build --release`（z42vm）无错——gate `build runtime ✔`（本 change 不碰 Rust）
- [x] 4.3 `xtask test`：e2e goldens **214/214** + stdlib **25/25** + compiler + vscode-syntax 全绿；
      cross-zpkg **8/8**（含新 `enum_cross_pkg` PASS）
- [x] 4.4 **z42c 自举不动点 7/7 gen1==gen2**（enum 导出：z42c 源无 enum → 不变；stdlib 25/25 无 miscompile）
- [x] 4.5 `tests/repl_decls_multiline` driver == expected_output（warm 回路实测：ok/49/ok/ok/12/ok/10/ok/ok/1/err/49/10/ok/11）
- [x] 4.6 driver 对 canonical fresh build 复验：diff vs expected_output = 0（✅ 匹配）
- [x] 4.7 spec scenarios 覆盖：函数/实例方法/enum/重定义/会话变量正交（driver）+ cross-zpkg enum 夹具（磁盘全量路径）
- 备注：2 个 `zbc-format` freeze 夹具的 regen 漂移 = **pre-existing boxing codegen 陈旧**（`__box_prim`/`Std.Int32`，
  非本 change——STRS diff 零 enum 串），已 `git checkout` 还原、不纳入本 commit（Scope 外）。

## 阶段 5: 文档同步 + 归档
- [x] 5.1 `docs/design/toolchain/repl.md`：声明累积机制（`Repl.R{N}`+`using`+world-extension）+ 多行接线 + 输入分类表刷新 + Deferred（capture-vars/supersede）
- [x] 5.2 机制文档：`repl.md` +「增量导入两处 compiler 修复」段 + design.md D7/D8（ExtendWithPackage world-extension + TsigReconcile enum 导出）
- [x] 5.3 `src/toolchain/scripting/README.md`（功能索引 + 核心文件 + 用法）+ `src/libraries/z42.ir/README.md`（TsigReconcile enum 导出）
- [x] 5.4 `docs/roadmap.md`：Deferred 索引加 2 条（capture-vars/supersede）
- [ ] 5.5 归档：mv → `archive/`；ACTIVE.md 释放 toolchain + compiler 锁

## 备注
- 前置 `fix-imported-free-func-namespace`（13ae506a）已在 main：自由函数跨包裸调解锁点（已兑现）。
- 边界：声明体不捕获会话变量（Out-of-Scope，Deferred `repl-future-decl-capture-vars`）；泛型返回自由函数不识别为声明（MVP 限制）。
- **enum=long 常量**是 z42 既有语义（`Color c = Color.Green` 本地亦非法）；本 change 令跨包 enum **成员/类型**可用，不改「enum 值即 long」语义。
- **compiler 锁**：`nested-types-followup` 持锁于 main checkout；本 change 在隔离 worktree、物理隔离（改 DepScan/TsigReconcile 不与嵌套类型重叠），User 授权预抢，合并解冲突。
- **构建缓存坑**：实测 `build stdlib`/`build compiler` 增量缓存未重编改动的 z42.ir/z42c.pipeline，需 clean 或直接建（阶段 4.1 必查）。