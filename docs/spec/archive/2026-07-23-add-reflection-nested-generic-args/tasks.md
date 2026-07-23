# Tasks: 嵌套泛型反射参数（方案 A：括号实参串 + runtime 递归解析）

> 状态：🟢 已完成 | 创建：2026-07-23 | 完成：2026-07-23 | 分支：feat/reflection-nested-generic-args（隔离，User 授权）
> 方案：A（User 二次裁决 2026-07-23，B 撞自举纪律否决——详见 design.md Decision 1）

## 进度概览
- [x] 阶段 1: z42c emitter 发括号实参串
- [x] 阶段 2: runtime 递归解析括号
- [x] 阶段 3: 测试
- [x] 阶段 4: 文档同步 + 归档

## 阶段 1: z42c emitter（compiler）
- [x] 1.1 `ExprEmitter.z42`：新增递归 `_typeofArgName(Z42Type)`（instantiated → 带尖括号完整名）
- [x] 1.2 `ExprEmitter.z42`：`_emitTypeof` 实参循环 `_typeofName` → `_typeofArgName`（根名不动）

## 阶段 2: runtime（Rust）
- [x] 2.1 `reflection.rs`：新增 `split_generic_args`（括号深度感知，顶层逗号切）
- [x] 2.2 `reflection.rs`：`make_type_from_name` 加 `<...>` 检测 → `make_constructed_type`（递归落点）

## 阶段 3: 测试
- [x] 3.1 `src/tests/types/nested_generic_args.z42`：覆盖 spec 全 Scenario（一层/多层/平铺不回归/Name）——golden interp+jit 空输出 exit0 全过
- [x] 3.2 平铺泛型回归确认（generic_type_definition ✓；types e2e 全套 70 pass 无回归）

## 阶段 4: 验证 + 文档 + 归档
- [x] 4.1 `cargo build --release`（z42vm）—— reflection.rs 编译通过
- [x] 4.2 验证：nested_generic_args interp+jit GREEN + types e2e 全套无回归 + **z42c 自举不动点 5/5 gen1==gen2 byte-identical**（本地 warm；无格式 bump 故不涉 CI 两代自举）
- [x] 4.3 `docs/design/language/reflection.md`：nested-generic-args 标记已落地（方案 A）+ 构造型泛型节
- [x] 4.4 `docs/roadmap.md`：0.3.12 C 主线「嵌套泛型 args」→ ✅（方案 A）
- [x] 4.5 `.claude/rules/version-bumping.md`：修陈旧路径（z42c.ir/z42c.project → z42.ir，User 批准）
- [x] 4.6 归档 → `docs/spec/archive/2026-07-23-add-reflection-nested-generic-args/`

## 备注
- **无 zbc/zpkg 格式 bump**：`TypeofInstr` / `TypeofInsn` 接口与 wire 布局不变，仅实参串内容带括号。
- 无自举影响（z42c 源不新用 z42.ir API、无新语法）→ 本地 warm 路径即可全验，不依赖 CI 两代自举。
- 曾实现方案 B（递归 TypeNode wire + 格式 bump）并本地两代自举验证，因撞 bootstrap axis ③/④ 回退。
