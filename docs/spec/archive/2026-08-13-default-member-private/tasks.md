# Tasks: 默认成员可见性 = private

> 状态：🟢 已完成 | 创建：2026-08-12 | 完成：2026-08-13

## 进度概览
- [x] 阶段 1: 位置感知默认（_vis / _visCode）
- [x] 阶段 2: 组合修饰符拒绝
- [x] 阶段 3: 破坏面修正
- [x] 阶段 4: 验证 + 文档

## 阶段 1
- [x] 1.1 `_vis(mods, dflt)` + 调用点（成员 private / 自由函数 internal via containing）
- [x] 1.2 `_visCode(mods, dflt)` + IrGen/ClassDescBuilder 调用点（成员 1 / 自由函数 3）

## 阶段 2
- [x] 2.1 `_parseModifiers` 组合访问修饰符 → E0405

## 阶段 3（破坏面：无修饰符→显式 internal/public）
- [x] 3.1 stdlib：BigInt `_fromMagSign`/`_oneMag`、Blake3、Sha256 → internal
- [x] 3.2 xtask 脚本：MicroBenchAgg 4 helper → internal
- [x] 3.3 e2e fixture：`Counter.Bump`/`Greeter.Hello` → public
- [x] 3.4 typecheck fixture 5 处补显式修饰符（继承 protected / 跨类 public）
- [x] 3.5 access_control_tests：重定默认-private + 组合修饰符测试

## 阶段 4
- [x] 4.1 `xtask test compiler`：23 units + 自举 5/5 gen1==gen2
- [x] 4.2 `xtask test`：完整 GREEN
- [x] 4.3 `cargo test`：892 passed 0 failed（反射无回归）
- [x] 4.4 REPL：默认-private E0404 ✅；组合修饰符 E0405 编译器路径 ✅（REPL 累积声明不重发 parse 诊断=次要 follow-up）
- [x] 4.5 docs：access-control.md（design + book）Status/默认可见性/组合拒绝

## 备注
- 无格式 bump（沿用 #180 u8 值域 0-3）。
- 类级访问强制（嵌套/类引用）Out of Scope → 独立后续 change。
