# Proposal: 修复跨包接口实现（fix-crosspkg-interface-impl）

> 状态：DRAFT（2026-07-18 起草；根因已完整定位 + 原型验证，含一个待定 format 决策）
> 子系统：`compiler`（semantics + project/TSIG）+ 可能 `runtime`（zbc/zpkg format）
> 触发：wire-z42b 的 Z42cCompiler 注入——`z42b` 持 `ICompiler c` 调 `c.Compile(req)`，
> ICompiler 来自另一个包（z42.build）。发现 **z42c 不支持「类实现另一个包的接口」**。

## 现象（原型实证）

一个类实现**另一个包的接口**、并以接口类型使用时全线失配。最小复现（2 包）：
- `pkgA`：`interface IFace { int Val(); }`
- `pkgB`（dep pkgA）：`class Impl : IFace {...}`，`o is IFace` / `o as IFace` / `f.Val()`

VM 插桩实测（修复前）：`o is IFace` → target `PkgB.IFace`（**错 ns**，应 PkgA）却 match=true
（因实现类的接口名也被同样错限定，自洽）；`o as IFace` → target `PkgB.<unknown>` → **null**。

## 根因（四层，全部定位）

1. **`as` 与 `is` 目标名不一致**（`ExprEmitter._emitCast`）：`is` 用 AST 原始名经 QualifyClass；
   `as` 用 `c.Type().Name()`——imported 接口 `ResolveType` 得 `Unknown` → 目标名 `"<unknown>"`。
2. **QualifyClass 不认 imported 接口 ns**（`EmitContext.QualifyClass` ← `ImportedClassNs`）：imported
   **类**的 ns 记入 `ClassNamespaces`（`ImportedSymbolLoader:87`），**接口未记**（`:96` 只 Put Interfaces）
   → QualifyClass 退到当前包 ns（`PkgB.IFace` 而非 `PkgA.IFace`）。
3. **TsigReconcile 丢弃 imported 接口**（`TsigReconcile._rebuildModule:85`）：重建 ExportedModuleZ 时
   跳过 interface TYPE 条目（`Flags&16`）+ 构造 `new ExportedInterfaceZ[0], 0` → **em.Interfaces 恒空**
   → imported 接口从不入 `table.Interfaces` → 跨包 `: IFace` 被当**基类**（错限定 + 空接口表）。
4. **接口方法签名根本未序列化**（**format 缺口**）：IrGen 的接口 TYPE 条目「**无 base/字段/方法**」
   （`IrGen:335`）；抽象接口方法无 body → **不在 SIGS**；曾承载它们的 EXPT 段已被 `drop-tsig-expt`
   删除。故即便重建接口骨架，**其方法名/签名在 zpkg 里无处可取** → `c.Compile(req)` / `f.Val()`
   报 `E0401: no method X on interface`。

## 已验证的部分修复（1-3，revert 未提交——见「为何未提交」）

- (1) `BoundCast` 携原始类型名 + `_emitCast` 用 `QualifyClass(rawName)`（镜像 `_emitIs`，含 Array/Object 归一）。
- (2) `ImportedSymbolLoader` 注册 imported 接口 ns 入 `ClassNamespaces`。
- (3) `TsigReconcile._rebuildModule` 重建 interface 条目入 `em.Interfaces`（`_rebuildInterface`）。

实测：1-3 后 `is`/`as`（**类型身份**）跨包正确工作。但因 (4) 未解，接口**方法调用**由「宽松 Unknown
（VCall 兜底）」变成「严格 no-method 报错」——**这是回归**，故 1-3 不能单独落地。

## 关键决策（format 层，待 User 裁决）

**(4) 需要把接口方法签名序列化进 zpkg。** 两条路：

- **A. 接口 TYPE 条目加方法块**（zbc/zpkg **format bump**）：接口 TYPE 追加方法名/参/返回。
  reader 解析、TsigReconcile 重建。**正**——但 format bump 须走 bootstrap-seed 两代纪律
  （support 先行一个 nightly、use 晚一个），且 self-host 两代不动点验证。工作量最大、最彻底。
- **B. 复活接口维度的 EXPT**（或等价 side-table）：把 imported 接口的 ExportedInterfaceZ（名+方法）
  重新写盘。比 A 局部，但也动 writer/reader（format）。
- **C. 宽松兜底（不改 format）**：member 解析对「imported 接口、方法表空」回退 VCall-by-name
  （恢复修复前的宽松语义）+ 仅落地 1-3（is/as 类型身份修好）。**方法调用返回类型 Unknown**——
  `r.Ok` 等下游解析受限，对 Z42cCompiler 注入**不够**（Compile 返回 CompileResult 要用其字段）。

倾向 **A**（唯一让「接口方法调用」完整类型化的路径），但它是 format 变更，须 User 认可 + 排期。

## Out of Scope / 依赖
- wire-z42b 的 Z42cCompiler 注入依赖本修复（尤其 (4)）落地才能跑通。
- 与 #2c（FQ 静态调用）无关，但同属 converge 暴露的 z42c 语言完备性缺口。

## GREEN 判据
- 2 包最小复现：`is`/`as`/接口方法调用全绿。
- Z42cCompiler 注入 e2e（z42b 持 ICompiler 调 Compile 得 CompileResult）跑通。
- self-host 7/7 逐字节（format bump 走两代）。
