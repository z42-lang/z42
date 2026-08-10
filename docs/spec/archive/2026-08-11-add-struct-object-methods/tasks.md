# Tasks: struct 合成对象协议方法（PR2b）

> 状态：🟢 已完成 | 创建：2026-08-11 | 完成：2026-08-11
> 「struct 值类型完备化」工作流 PR2b（PR2a 收口）。

## 进度概览
- [x] 阶段 1: 合成方法体发射（FunctionEmitter/ExprEmitter）
- [x] 阶段 2: IrGen 注入 + 反射签名
- [x] 阶段 3: VM boxed vcall 派发（interp+JIT）
- [x] 阶段 4: 测试 + 验证 + 文档

## 阶段 1: 合成 body 发射
- [x] 1.1 暴露 PR1 `_emitStructEquality`/`_emitLeafEqChecks`（改 internal 或加 ExprEmitter public 入口）
- [x] 1.2 `FunctionEmitter.EmitSynthStructEquals(name, owner)`：ctx 脚手架（reg0=this,reg1=other Ref）+ `other is P`→BrCond→(AsCast 拆箱 + `_emitStructEquality(true,r0,u,name)`) / false；build IrFunction ret bool
- [x] 1.3 GetHashCode —— **改用 native `__struct_hash_code`**（对 boxed bytes+refs FNV，vcall 臂路由），非合成 IR 方法（更简、correct-enough；±0/NaN 边角文档标注）
- [x] 1.4 ToString —— **vcall 臂直接返回短类型名**（C# ValueType 默认），非合成函数

## 阶段 2: IrGen 注入 + 反射
- [x] 2.1 `IrGen.Generate` 成员循环末尾：blob struct ∧ 未显式声明 → 合成 3 方法 `_pushFunc`（键 Equals$1/GetHashCode$0/ToString$0）
- [~] 2.2 `ExportedTypeExtractor` 反射签名注入 —— **Deferred**（可选、动 SIGS 有自举字节稳定性风险；反射 GetMethods 见合成 Equals 留后续；VCall 靠 func_index 按名，不受影响）

## 阶段 3: VM boxed vcall 派发
- [x] 3.1 `exec_vcall.rs` BoxedStruct 臂：unbox this(→StructRef) + prepend `{type_name}.{m}$arity`/`{m}` 候选（GetType 保留特判）
- [x] 3.2 `jit/helpers/vcall.rs` BoxedStruct 臂对称

## 阶段 4: 测试 + 验证 + 文档
- [x] 4.1 golden `src/tests/types/struct_object_methods.z42`（Equals 同/异值·异类型·嵌套；GetHashCode 同值同 hash；==/!= on boxed；ToString）—— Dictionary<P,V> Out-of-Scope（泛型边界装箱=P3，PR2b 前本就不工作）
- [x] 4.2 `cargo build --release` + `cargo test --lib`
- [x] 4.3 完整 `xtask test` GREEN（不传 Z42_HOME）+ self-host 5/5
- [x] 4.4 spec scenarios 覆盖确认
- [x] 4.5 `docs/book/.../struct-value-semantics.md`：合成方法小节 + Deferred 更新
- [x] 4.6 归档 + PR

## 备注
- 无格式 bump（复用 PR1 叶子指令 + 现有 builtin/vcall 派发）。
- D5 定案：`==` on boxed=值相等（PR2a PartialEq）；`.Equals`=合成叶子方法；NaN 边角文档标注。
- 环境：worktree z42-svs4，branch add-struct-object-methods（基于 main a0947f57 含 PR2a），warm 环境复用。
