# Proposal: 移除方法注册键的读侧回落（unify-regkey-phase2）

> 类型：refactor(compiler) + test(compiler) | 创建：2026-09-05
> 前身：[`archive/2026-09-04-unify-regkey-source-of-truth`](../../archive/2026-09-04-unify-regkey-source-of-truth/proposal.md)
> （阶段 1 = PR #427，已合并）。本变更是那条 change 明确拆出的 **PR-2**。

## 背景

阶段 1 把「方法注册键」的推导收敛成两个口子：

- **写侧** `SymbolCollector.RegisterMethod(ct, md, sym, key)` —— 算键与写键绑死，
  结构上不可能「注册了却没填 `md.RegKey`」；
- **读侧** `OverloadResolver.MethodKeyOf(owner, md)` —— 6 个消费点唯一的解析实现，
  也是**全仓库最后一处**保留「按名字猜键」回落的地方：

```z42
if (md.RegKey != "") { return md.RegKey; }
string arityKey = md.Name + "$" + md.ParamCount.ToString();
if (owner != null && owner.Methods.ContainsKey(arityKey)) { return arityKey; }
return md.Name;                       // ← 猜
```

阶段 1 之后这段回落**已无人依赖**：impl-block 方法由 `_passImpls` 走注册入口补齐了键；
被擦除的 decl-only partial 由 `MemberCollector` 的擦除分支顺手写键。回落只剩下「万一有
漏网」的兜底语义——而这种兜底恰恰有害：猜出来的裸名在 #414 的键规则下可能命中**另一个
同名重载**（primary 才是裸键），于是编译成功、运行期才派发错人。

## 目标

1. **删掉读侧回落**，把 `MethodKeyOf` 收窄成不变量守卫：`RegKey` 为空即抛，
   附「注册路径漏走 `RegisterMethod`」的诊断。顺带去掉不再需要的 `owner` 形参。
2. **补上两块零覆盖的测试**（阶段 1 tasks 里列的硬前置）：
   - 跨-CU partial 的 TSIG 导出面（`src/tests/partial-types/*` 全是单文件用例）；
   - `static partial` 方法（全仓库此前无此写法，而它走的是**全签名 mangle** 键，
     与实例的 primary/非-primary 规则不同）。
3. 顺带交付 User 在 #427 上要求的 **`dict_set_get` 回归确认**（见 tasks.md ①）。

## 不做

- 不改键的格式或规则（primary 裸 / 非-primary 全键不动）→ **无格式 bump、无两代自举**。
- 不把 impl 方法并入 primary/非-primary 规则。
- 不动 `CallEmitter` 的静态 DepIndex 查找。

## 风险与判据

「回落已死」不能靠推理断言 —— 用**命中即抛**的方式实测：把回落分支换成 `throw` 后，
凡是还依赖它的路径都会在编译期炸出来。跑通「自举（含 gen1==gen2 不动点）+ 全量
`xtask test` + JIT 双 shard + bootstrap 边界」即证明全语料无人触发；同时这个 `throw`
**就是最终形态**，不是临时探针。

字节中性判据沿用阶段 1：新旧 z42c 各编一遍**未改动**的 `z42.core`，产物逐字节相同。
