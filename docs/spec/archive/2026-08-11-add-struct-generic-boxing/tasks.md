# Tasks: struct 泛型容器装箱（P3a）

> 状态：🟢 已完成 | 创建：2026-08-11 | 完成：2026-08-11
> 「struct 值类型完备化」P3a（泛型边界装箱，格式中立）。P3b（真内联+写屏障）后随。

## 进度概览
- [x] 阶段 1: 装箱（存入 type-param）
- [x] 阶段 2: 拆箱（取出 → 具体 struct）
- [x] 阶段 3: 测试 + 验证 + 文档

## 阶段 1: 装箱
- [x] 1.1 `TypeChecker.BoxIfNeeded` struct 分支 `erasesS` 加 `|| (target is Z42GenericParamType)`
- [x] 1.2 `ExprTyper._bindAssign` set_Item 分支：查 `set_Item` `Signature.ParamTypes`，对 index/value 各 `BoxIfNeeded`

## 阶段 2: 拆箱
- [x] 2.1 新增 `TypeChecker.StructUnboxTarget(rawRet, instRecv)`（`_substGeneric` 判泛型返回是否为 struct）+ VM `as_cast`/`jit_as_cast` 的 **StructRef 恒等臂**（使检索统一走 AsCast：BoxedStruct→拆箱 / StructRef→恒等）
- [x] 2.2 `ExprTyper._bindIndex` get_Item：索引实参装箱 + 元素 subst 为 struct → 结果包 `BoundConvert` 拆箱
- [x] 2.3 `MemberResolver._bindInstanceMemberCall`：方法返回 type-param 为 struct（如 `List.First()`）→ 包拆箱
- [x] 2.4 `FunctionEmitter` foreach：循环变量 struct（`fe.VarType` 已是 Z42Type）→ 元素 AsCast 拆箱后 writeback

## 阶段 3: 测试 + 验证 + 文档
- [x] 3.1 golden `src/tests/types/struct_generic_container.z42`（Dictionary<P,int> 存取/ContainsKey/覆盖；List<P> add/index/foreach/Contains；取出值独立；Tagged string 键；非 struct 泛型回归）
- [x] 3.2 直接编译+运行 golden interp+jit EXIT=0（快验）
- [x] 3.3 `cargo build --release`（VM 仅加 as_cast StructRef 恒等臂，格式中立）+ `cargo test --lib`（915+21 passed）
- [x] 3.4 完整 `xtask test` GREEN（不传 Z42_HOME）+ self-host 5/5
- [x] 3.5 spec scenarios 覆盖确认
- [x] 3.6 `docs/book/.../struct-value-semantics.md`：泛型容器装箱小节 + Deferred（Dictionary<P,V> → ✅）
- [x] 3.7 归档 + PR

## 备注
- 无格式 bump（复用 `__box_struct` + `AsCast`；容器 backing/ABI 不变）。warm 环境（z42-svs4）复用。
- 只装箱非字节内联——密度收益（对象内联 struct 字段 / struct[] 字节 backing）留 P3b（bump + 写屏障分叉）。
- 环境：worktree z42-svs4，branch add-struct-generic-boxing（基于 main 694bd54e）。
