# Tasks: prim 类型实例方法 type-based 重载（阶段 1 support）

> 状态：🟢 已完成（GREEN 全绿 + gen1==gen2 + 跨包 T.2 派发正确） | 创建：2026-08-31 | 完成：2026-08-31
> 类型：feat（编译器绑定 / codegen） | 子系统：compiler
> 本 change **只做阶段 1（support）**：扩 z42c 能力，源自身不使用。String 方法补齐 = 阶段 2（晚一个 nightly，独立 change）。
> 分支/worktree：`add-prim-instance-type-overload`（本 worktree，基于 origin/main）。
>
> **实施纪要（2026-08-31）**：主修 B（仅 `wms==null` 追加决议）**单独不足**——跨包（imported）prim
> wrapper 因 `ImportedSymbolLoader.z42:308-312` 为 mangle 方法额外注册**裸-first-wins 别名键**，令
> `wms!=null` 绕过 `wms==null` 门 → 仍 emit 裸名 → 运行期 `VCall: expected object`。停下回报后 User 裁决
> 改 **方案 1**：门扩展为「同 arity 重载 ≥2（`OverloadBinder._sameArityOverloadCount`）才走
> `_resolveOverload` 取 mangle RegKey；否则原 arity-only 快路径」。本地 + 跨包同时生效，无重载代码字节
> 中性。VM 侧无需改动（`exec_vcall.rs:321-379` 本就按 `<class>.<RegKey>` 派发）。自举能力版本号=不 bump
> （该机制实际未实现，靠 bootstrap check 下载 nightly 实编验越界）。

## 进度概览
- [x] 方案 1（根因）：`MemberResolver` prim-wrapper 分支——同 arity 多重载走 `_resolveOverload` 取 mangle RegKey；否则原 arity-only 快路径（字节中性）
- [x] 辅修 A（防御）：`CallEmitter` 实例 DepIndex 捷径加 local-wins 守卫
- [x] e2e 实测 VCall vtable 派发键（Risk#3，跨包必跑）——emit 正确 mangle 名 + 运行期各自派发正确
- [x] gen1==gen2 自举字节不动点验证（字节中性硬门）——3/3 逐字节相同
- [x] 文档同步（source-compile.md）

## 阶段 B → 方案 1：主修（根因）
- [x] B.1 `MemberResolver.z42`（`:130-152` prim-wrapper 分支）：门扩展。先 `int _sameArity = _overload._sameArityOverloadCount(...)`；`_sameArity<2` 走原 `_overloadKey`/`_findMethod` 快路径（`wms!=null` 用 mkey，字节中性）；`_sameArity>=2` **跳过快路径**、直接 `_resolveOverload` → `BoundCall(OwnerClass=PrimModel.Keyword(rt.Name()), MethodName=rms.RegKey, ret=rms.Signature.Ret)`（镜像 class 路径 `:57-62`）。`wms==null`（方法真不存在）也落决议（0 候选返 null → loose-bind，字节中性）。
  - 附：`OverloadBinder.z42` 新增 internal `_sameArityOverloadCount(symbols, ct, name, argCount)`（复用 private `_collectOverloads` RegKey 去重 + 走基链 + `_resolveOverload` 同款 byArity 过滤）。
- [x] B.2 实参填充：命中真实符号用 `_withDefaults`（镜像 `:61`）——对拍确认字节中性。
- [x] B.3 访问/弃用检查：镜像 class 路径 `:59-60`（`CheckAccess` / `CheckDeprecatedM`）。

## 阶段 A：辅修（对称守卫）
- [x] A.1 `CallEmitter.z42`（`:160` 实例 DepIndex 捷径）：加 `ownerIsLocalInst = LocalClasses != null && LocalClasses.ContainsKey(TypeFactsTc._primWrapper(c.OwnerClass))`，捷径条件追加 `!ownerIsLocalInst`（对称静态路径 `:201-202`）。
- [x] A.2 `CallEmitter` 与 `TypeFactsTc` 同 `Z42.Semantics` 命名空间 → 直接可引用 `TypeFactsTc._primWrapper`（public static），无需补 using。
  - 决议：采 `TypeFactsTc._primWrapper`（不提升 `EmitContext._primWrapper` 可见性）——见 design Decision 2。

## 阶段 T：验证（本地 + e2e）
- [x] T.1 绑定级验证：由 e2e（T.2）+ 字节不动点（T.4）共同覆盖——同 arity type-based 重载 emit 正确 mangle RegKey；无重载键不变（gen1==gen2 证）。未单列 unit test（e2e 已充分实证跨包派发）。
- [x] T.2 **e2e 实测 vtable 派发（Risk#3）**：探针 `Std.String.__PrimOvldProbe(string)/(char[])`（避开 Split——z42c 自身 `_hasWord` 调 Split，remangle 破自执行），app **跨包**调两重载 → emit `__PrimOvldProbe$1$string` / `$1$char[]`（非裸名），运行期 `STR:arg` / `CHAR:3` **各自派发正确**，无 `VCall: expected object`。测完删除探针。
  - 结论：VM 本就以完整 mangle RegKey 派发 prim VCall（`exec_vcall.rs:321-379` 拼 `<class>.<RegKey>`）；缺口在编译器绑定 emit 裸名，方案 1 修复。
- [x] T.3 E0436 回归：带临时 `Split(char[])` 的树编 z42.core → **无** `namespace Std.Regex is used but not imported`；本地 this.Split 绑到 `Split$1$string`/`Split$1$char[]`/`Split$2`。测完删除。
- [x] T.4 **gen1==gen2 自举字节不动点（硬门）**：`xtask test compiler` 现有树逐字节相同（z42c.semantics/pipeline/driver 3/3）→ 证字节中性（新门今天 `_sameArity` 恒 <2 → 快路径不变）。
- [x] T.5 `xtask test bootstrap`：本 change 为 support、源不使用新能力 → 无越界（GREEN 内自举链绿）。
- [x] T.6 完整 GREEN：`xtask test` 全 stage passed（前置 `rm -rf /tmp/z42c-e2e-*`）。

## 阶段 D：文档同步
- [x] D.1 `src/compiler/z42c.semantics/README.md`：无需改——README 是文件索引 / 职责总纲，MemberResolver 既有条目已涵盖实例绑定职责，本 change 未新增/删文件。
- [x] D.2 book 机制页：并入 `docs/book/src/compiler/source-compile.md` TypeCheck 小节新增「prim 接收者实例方法的 type-based 重载决议」（缺陷 → 方案 1 → VM 派发键，配 file:line）。User 裁决落点 = 并入该页、不新建专页。
- [x] D.3 触发矩阵 doc-check：编译器机制变更 → book ✓；命令面无变化；核心文件表无触发（OverloadBinder 既有文件加 helper，非新增/删文件）。

## 阶段 归档 / 落地
- [x] Z.1 归档 change → `docs/spec/archive/2026-08-31-add-prim-instance-type-overload/`。
- [ ] Z.2 PR：**交由 main 统一排 PR 顺序**（与 KVP fix 都改 z42c.semantics，有潜在语义耦合，需定合并序 + rebase）。本 worktree **不 push、不开 PR**。
- [ ] Z.3 合并前并入 origin/main 最新 + 重跑 GREEN；合并后删远程/本地分支 + worktree（由 main 编排）。
- [ ] Z.4 阶段 2（String 补齐）另开 change，等本 change 随 nightly 发布后开工——**不在本 change**。

## 备注
- 无 zbc/zpkg 格式 bump、无新语法、无新 IR。自举能力版本号=不 bump（机制未实现，见头部纪要）。
- 两阶段分离原因见 design Decision 5（bootstrap-seed 分阶段引入纪律）。
- 关键风险处置：① gen1==gen2 字节不动点 ✅；② 防整体替换漂移——方案 1 用 `_sameArity` 门确保无重载代码走原快路径 ✅；③ VM vtable 派发键实测——跨包 e2e 实证 VM 按 RegKey 派发 ✅；④ 两阶段纪律——本 change 仅 support ✅。
