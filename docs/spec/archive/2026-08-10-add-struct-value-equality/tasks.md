# Tasks: struct 值相等（`==` / `!=`）

> 状态：🟢 已完成 | 创建：2026-08-10 | 完成：2026-08-10
> 「struct 值类型完备化」工作流 **PR1**（值相等地基）。PR2（struct→object 健全装箱）后续独立 DRAFT。

## 进度概览
- [x] 阶段 1: codegen 脱糖实现
- [x] 阶段 2: golden 测试
- [x] 阶段 3: 验证 + 文档同步

## 阶段 1: codegen 脱糖（`ExprEmitter.z42`）
- [x] 1.1 `_emitBinary`：`==`/`!=` 且两操作数均 `_isBlobStruct` → 分流到 `_emitStructEquality(op=="==", a, c, _blobStructName(b.Left.Type()))`；否则原 `_emitCompare` 不变
- [x] 1.2 新增 `_emitStructEquality(wantEqual, a, c, structName)`：result(Bool)+failL/endL；全等块 `ConstBool(wantEqual)`、fail 块 `ConstBool(!wantEqual)`，两块 `Br(endL)` 汇合（镜像 `_emitConditional`）
- [x] 1.3 新增 `_emitLeafEqChecks(a, c, off, structName, failL)`：镜像 `_copyRegion` 递归枚举叶子；每叶子两 `StructFieldGetPrim` + `EqInstr` + `BrCondTerm(cmp, contL, failL)` + `StartBlock(contL)`；`StructLeafKind.Struct` 字段递归

## 阶段 2: golden 测试
- [x] 2.1 `src/tests/types/struct_equality.z42`：扁平相等/不等（首/末字段）、嵌套 `Line`、含 `string` 叶子 `Tagged`（内容相等）、基元叶子不同（断言自检 + EXIT=0）

## 阶段 3: 验证 + 文档
- [x] 3.1 `cargo build --manifest-path src/runtime/Cargo.toml --release`（VM 无改动，确认可建 ✓）
- [x] 3.2 完整 `xtask test` GREEN（e2e / cross-zpkg / stdlib / compiler self-host **5/5 gen1==gen2** / vscode-syntax）——不传 `Z42_HOME`；struct_equality interp+jit 均 EXIT=0
- [x] 3.3 spec scenarios 逐条覆盖确认（--dump-ir 验脱糖结构：逐叶子 field_get+eq+br.cond 短路到共享 seq_ne）
- [x] 3.4 `docs/book/src/runtime/struct-value-semantics.md`：加「struct 值相等」小节 + 页头状态；「收敛面与延后」`struct==` 移到 ✅
- [x] 3.5 归档 + PR（rebase origin/main + 重跑 GREEN）

## 备注
- 无格式 bump、无新 IR 指令、无运行时改动——纯 `ExprEmitter` 前端脱糖。
- 环境：worktree `z42-svs4`，分支 `add-struct-value-equality`（基于 origin/main `61b76d17`，zbc1.31/zpkg0.36）。
  0.36 nightly 种子（z42sdk2）warm 构建一把过（种子格式==源==VM）。
