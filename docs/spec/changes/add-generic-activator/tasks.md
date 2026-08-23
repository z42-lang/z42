# Tasks: 泛型 Activator.CreateInstance<T>（G3）

> 状态：🟡 进行中 | 创建：2026-08-23

## 进度概览
- [x] 1. stdlib 实现（Activator.z42 泛型薄壳）
- [x] 2. 方法级形参转发（compiler + runtime，实施期扩展）
- [x] 3. 测试（reflection.z42 [Test]，含转发用例）
- [ ] 4. 文档同步（README / book / roadmap）
- [ ] 5. GREEN + 归档 + PR

## 阶段 1: stdlib 实现
- [x] 1.1 `Activator.z42`：`public static T CreateInstance<T>() { return (T)Activator.CreateInstance(typeof(T)); }` + 更新头注

## 阶段 2: 方法级形参转发（`$mta:<idx>`，实施期扩展）
- [x] 2.1 `Bound.z42`：`BoundCall.MethodTypeArgFwd`（int[]，与 MethodTypeArgs 平行）+ 构造默认空
- [x] 2.2 `MemberResolver.z42`：`_applyMethodTypeArgs` 填 fwd（`Z42GenericParamType` + `env.MethodParamIndexOf ≥ 0`）
- [x] 2.3 `CallEmitter.z42`：`_methodTypeArgNames` 据 fwd 发 `$mta:<idx>`（否则 `_typeofArgName`）
- [x] 2.4 `mod.rs`：`resolve_forwarded_mta(caller, mta)` helper
- [x] 2.5 `exec_call.rs` / `exec_vcall.rs`：入口按调用方 frame 解析 `$mta:N`（`starts_with` 门控，无标记不 alloc）

## 阶段 3: 测试
- [x] 3.1 `z42.core/tests/reflection.z42`：`CreateInstance<T>` 往返（用户类 ctor 副作用 + 类型正确 + 泛型方法内转发）——47/47 绿

## 阶段 4: 文档同步
- [x] 4.1 `docs/book/src/language/generic-methods.md`：方法级形参转发机制（`$mta:<idx>`）+ 边界更新
- [x] 4.2 `docs/roadmap.md`：0.4.3 G3 标 ✅ + Deferred Backlog 更新
- （`z42.core/src/README.md` 不改——未 itemize Reflection/ 子目录，机制文档落 book）

## 阶段 5: 验证 + 归档
- [ ] 5.1 `xtask test`：完整 GREEN（stdlib + self-host 不动点 5/5 + e2e）
- [ ] 5.2 spec scenarios 逐条覆盖确认
- [ ] 5.3 归档 + PR

## 备注
- **无格式 bump**：转发用 `$mta:<idx>` 标记字符串（method_type_args 本就 string[]），运行期解析。无新 native / IR opcode。
- 无标记调用产物字节不变（`starts_with("$mta:")` 门控）→ self-host 不动点不受影响。
- 跨包 typeof(T) 短名 handle 依赖 add-json-serde 已落地的 `make_type_from_name` 兜底。
- 嵌套构造泛型里的方法级形参转发（`Bar<List<T>>`）留 Deferred。
