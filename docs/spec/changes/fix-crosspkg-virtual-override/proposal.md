# fix-crosspkg-virtual-override — 跨 zpkg 虚成员派发根因修复

> 类型：`fix`（根因修复，User 授权直接推进）。占用：`compiler`（独立分支 `claude/z42c-ctor-arity`，
> 与 `split-irgen-class` 物理隔离，合并时解冲突）。GREEN 以 CI 为权威。

## 症状

一个类 `override` 了**来自依赖 zpkg 的具体基类**的 `virtual` 方法后，经**基类静态类型的接收者**
调用该方法时，**派发到基类实现而非 override**——多态失效：

```z42
// z42.build.zpkg：BuildHooks（具体基类，虚方法带 no-op 默认实现）
public class BuildHooks { public virtual void BeforeAssets(IPipelineContext ctx) { } }

// 项目 zpkg：override
public class ProjectHooks : BuildHooks {
    public override void BeforeAssets(IPipelineContext ctx) { /* cargo build ... */ }
}

// 调用方（z42b）：
BuildHooks h = new ProjectHooks();
h.BeforeAssets(ctx);   // ❌ 跑 BuildHooks 的 no-op，ProjectHooks.BeforeAssets 永不执行
```

同类症状在**接口属性访问**上表现为读到 `Null`：`IPipelineContext ctx; ctx.Dirs` 跨 zpkg
读具体实现（`PipelineContext` 的 auto-property `Dirs`）时崩 `FieldGet: not an object, got Null`。

wire-z42b 的 build hook 机制（`z42 publish` 经项目 hook 现场 cargo 编 apphost stub，
免装 desktop workload）正卡在此：hook 靠「override 基类 `BuildHooks` + 读 `IPipelineContext`
属性」运转，两条派发路径都坏。同源单一根因，见下。

## 根因

**imported 类型的虚成员访问被 DepIndex「直呼捷径」劫持成编译期钉死的直接调用，绕过运行时 vtable 派发。**

z42c 的 `ExprEmitter` 对实例调用有一条 DepIndex 捷径：接收者的类链**不自有**该方法
（`ChainHasMethod` 对 imported 类恒返 false——G18 反劫持守卫刻意只认本地类）时，直接
emit `Call Ns.Class.Method`（按名静态派发）。对 imported **非虚**方法这是对的、且更快
（如 `_diags.Error()`）；但对 imported **虚**方法，接收者的运行时实际类型可能是子类
override，直呼把基类实现钉死 → override 永不生效。

三条 emit 路径共享此病，`ChainHasMethod` 的本地-only 守卫都覆盖不到：

| 路径 | 位置 | 症状 |
|------|------|------|
| 实例**方法**调用 | `ExprEmitter._emitCall` instance 分支 | override 被基类实现钉死 |
| 属性 **getter** | `ExprEmitter._emitMember` | 接口接收者 → 落 `FieldGet "X"`（实现类无同名字段）→ Null |
| 属性 **setter** | `ExprEmitter._emitAssign` BoundMember 分支 | 同上，`FieldSet "X"` 打空 |

接口方法调用此前已修（`ifaceRecv` 恒 VCall，fix-iface-receiver-depindex-hijack）；本次补齐
**类的虚方法** + **接口/虚属性 getter·setter** 三条同源缺口。

## 修复

**imported 虚成员恒 VCall，交由运行时按接收者实际 `type_desc.vtable` 派发**——不进 DepIndex 直呼捷径。

1. `EmitContext.ReceiverMethodIsVirtual(recvType, method)`：判接收者静态类型的该方法是否虚
   （virtual / abstract）。imported 类经 TSIG 已把继承方法展开进 `OwnMethod*`（带 virtual/abstract
   flag），直接查接收者自身 `Z42ClassType` 即命中「静态类型 = 基类本身」主场景；再沿 `BaseName`
   走一次链兜住中间类。prim 接收者（`Type()` 非 `Z42ClassType`）→ false，仍走既有 prim 派发不受影响。
2. `ExprEmitter._emitCall` instance 分支：`!owns && !ifaceRecv && !virtualRecv && Deps!=null` 才走
   DepIndex 捷径——虚方法落到 VCall fallback。
3. `ExprEmitter._emitMember` / `_emitAssign`：接口接收者的成员访问定义上就是属性 getter/setter →
   恒 VCall `get_X` / `set_X`（接口无字段，否则落 `FieldGet`/`FieldSet` 打不到实现类的 auto-property）。

## 影响面 / 验证

- **blast radius 极小**：self-host **7/7 gen1==gen2 byte-identical**——z42c 自身源码无受影响调用点
  （否则 gen1 nightly-emit ≠ gen2 fix-emit）。
- **行为回归**：test compiler **21 单元 / 327 tests 全绿，0 failed**。
- **修复验证**：`BuildHooks` override 经 `BuildHooks h` 接收者调用 → interp + jit 均执行 override；
  wire-z42b build hook e2e：`z42 publish xtask` 免 workload → hook 现场 cargo 编 apphost → `✅ apphost ready`，产物可运行。
- **运行时零改动**（纯 z42c 源码）→ VM/cargo 不受影响。

## 遗留（非阻塞）

- **用户库跨 zpkg 类作变量类型解析为 `Unknown`**：`BaseLib.Base b`（普通用户依赖 zpkg 的类）作局部/参数
  类型时，emit 期接收者 `Type()=<unknown>`、`owner=<?>`（`MemberResolver` 落 Z42Error/Unknown 分支）。
  stdlib / z42.build 类正常解析（本修复据此生效），故非本 change 阻塞项——单列 follow-up 排查
  用户库类型导入链（疑 ImportedSymbolLoader 未把该类注册为可解析**类型**）。
