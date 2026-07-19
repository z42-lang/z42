# Proposal: 接口成员枚举（typeof(IFoo).GetMethods()）

> 状态：🟢 已批准（User「继续」2026-07-19）+ **重定为纯 runtime**（勘察发现格式与发射已就位，见下）。
> 子系统：**runtime**（reader 存储 + `builtin_type_methods` 表面化）。**无 compiler 改动、无格式 bump。**

## 勘察发现（2026-07-19，重定 scope）

原计划要改编译器发射 + 可能 bump 格式。**勘察证实两者都已就位**：并发变更 `fix-crosspkg-interface-impl`（2026-07-18，zbc 1.28 / zpkg 0.33）**已给接口 TYPE 条目加了方法签名块**（`mcount + (name, ret, pcount, ptypes)×n`，gated on interface flag），供跨包接口实现恢复签名。**运行期 reader 已解析该块但当前丢弃**——`zbc_reader.rs:508` 原注释："the VM resolves interface calls via vtable, so we parse for cursor correctness and **discard (future: surface via reflection)**"。本变更即兑现该 "future"：把丢弃改为存储 + 在 GetMethods 表面化。**纯 runtime，零编译器改动，无格式 bump（已在 1.28/0.33）。**

## Why

反射的类型面已相当完整，但**接口成员枚举**是 0.3.x C 主线「反射完整化」退出标准里**最后一个未落地的反射缺口**（roadmap 0.3.12：非泛型 Invoke ✅ / IsEnum ✅ / 嵌套泛型 GetGenericArguments 🟡 / **接口成员枚举 ❌**）。

现状：接口自 zbc 1.19（add-reflection-interface-class-predicates）起产**最小 TYPE 条目**（identity + flags + 基接口 + TypeParams，**无方法表**）——故 `typeof(IFoo).GetMethods()` 返回**空数组**。用户无法通过反射发现接口声明的方法契约（DI 容器、mock 生成、序列化契约等场景需要）。

不做的代价：反射对接口"知道它是接口、知道它继承哪些接口，但看不到它声明了什么方法"，与 C# `Type.GetMethods()` 行为不一致，0.3.x C 主线退出标准挂着最后一项。

## What Changes（纯 runtime）

- **reader 存储接口方法块**（`zbc_reader.rs`）：把当前丢弃的接口方法签名块（name / ret / param types）读入 `ClassDesc`（新字段 `iface_methods`）→ `TypeDesc`（cold slice，反射专用，同 enum_members 模式）。
- **新建接口方法 MethodInfo builder**：接口方法**无 backing Function**（无 body、不进 SIGS/FUNC），故不能走 `resolve_func_sig`；直接从块数据（name / ret_type / param_types）构建 MethodInfo，设 `IsAbstract=true` / `IsVirtual=true` / `IsStatic=false`，params 用类型名（无 debug 名 → `arg{n}`，同 stripped 构建）。
- **`builtin_type_methods` 表面化**：td 是接口（有 `iface_methods`）时，从块构建并追加 MethodInfo → `typeof(IFoo).GetMethods()` 非空；连带进 `GetMembers()`（其 methods 部分复用 `builtin_type_methods`）。
- **无 compiler 改动、无格式 bump**（格式已在 zbc 1.28 / zpkg 0.33，块已 emit + 已被 reader 解析）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/spec/changes/add-interface-member-reflection/*` | NEW | 本变更规范 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `ClassDesc` 加 `iface_methods` 字段（name/ret/param-type 名）+ 结构体 `IfaceMethodSig` |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | 接口方法块从"parse+discard"改为读入 `iface_methods` |
| `src/runtime/src/metadata/types.rs` | MODIFY | `TypeDescCold` 加 `iface_methods`（反射专用 cold）+ accessor |
| `src/runtime/src/metadata/loader*.rs` | MODIFY | ClassDesc.iface_methods → TypeDesc 组装 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | 接口方法 MethodInfo builder + `builtin_type_methods` 接口分支 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | 加 [Test]：`typeof(IShape).GetMethods()` 含声明方法 + 签名/flags 正确 |
| `docs/design/language/reflection.md` | MODIFY | 接口成员枚举落地 + Deferred（继承接口方法/接口属性）更新 |
| `docs/roadmap.md` | MODIFY | 0.3.12 退出标准「接口成员枚举」标 ✅ |

**只读引用**：
- `src/runtime/src/metadata/zbc_reader.rs:503-519`（当前 parse+discard 的接口方法块）
- `src/runtime/src/corelib/reflection.rs` `build_method_info` / `builtin_type_methods`（MethodInfo 构建 + 消费）
- `src/runtime/src/metadata/types.rs` `TypeDescCold`（enum_members 先例——cold 反射专用块）

## Out of Scope

- **继承接口的方法**：`interface IBar : IFoo` 时 `typeof(IBar).GetMethods()` 只返 IBar **直接声明**的方法，**不含** IFoo 的（对齐 C# 默认——继承接口方法经 `GetInterfaces()` + 各自 `GetMethods()` 获取）。传递方法闭包 → Deferred。
- **接口属性 / 索引器 / 事件**：MVP 只做方法枚举；接口声明的 property（get_/set_）→ 后续（可复用 GetProperties 派生逻辑）。
- **Invoke on 接口方法**：接口方法无 body，`MethodInfo.Invoke` 于接口句柄上无意义（需具体实现实例上的 virtual 派发）——不在本变更。
- **default interface methods**：z42 无此特性。

## Open Questions

- [ ] Q1（实现期确认，不阻塞批准）：接口 TYPE 序列化是否已无条件写方法块（→ 无 bump）还是整块省略（→ bump zbc 1.27→1.28 / zpkg 0.32→0.33）？
- [ ] Q2：接口方法在 `GetMethods()` 里归入 vtable（virtual）还是 own_methods？倾向 vtable（接口方法隐式 virtual），与类的 virtual 方法一致呈现。
- [ ] Q3：并发 compiler 锁——`compiler` 锁现被 `split-irgen-class` 等占用；本变更需协调排队或独立分支隔离（同 stabilize-dispatch-keys 先例）。开工前定。
