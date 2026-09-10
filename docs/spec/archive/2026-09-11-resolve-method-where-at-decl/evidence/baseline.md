# 修前/修后实测（2026-09-11，main = 239e98fa，编译器为本树自建）

语料：[`decl_before.z42`](decl_before.z42)（四个症状各一格）。命令：
`z42c --emit-zbc decl_before.z42 out.zbc`。

## 修前（6 条）

```
decl_before.z42(2,15): E0402: type parameter `T` cannot be both `class` and `struct` on `BoxD`
decl_before.z42(2,15): E0402: type parameter `T` cannot be both `class` and `struct` on `BoxD`   ← ① 双报
decl_before.z42(11,5): E0404: cannot access private method `called` of `Use`
decl_before.z42(4,34): E0443: unknown constraint type `IFooo` on `called`
decl_before.z42(12,5): E0404: cannot access private method `called` of `Use`
decl_before.z42(4,34): E0443: unknown constraint type `IFooo` on `called`                        ← ② 调 2 次报 2 条
```

`never`（成员、从不调用）、`freeNever`（顶层、从不调用）、`badParam`（`where U:` 挂在不存在的
型参上）**一条都没有**。

## 修后（7 条）

```
decl_before.z42(2,15): E0402: type parameter `T` cannot be both `class` and `struct` on `BoxD`   ← ① 1 条
decl_before.z42(4,34): E0443: unknown constraint type `IFooo` on `called`                        ← ② 1 条
decl_before.z42(5,33): E0443: unknown constraint type `IBarr` on `never`                         ← ③ 新
decl_before.z42(6,27): E0401: where clause references unknown type parameter `U` on `badParam`   ← ④ 新
decl_before.z42(8,33): E0443: unknown constraint type `IQuux` on `freeNever`                     ← ③ 新（顶层）
decl_before.z42(11,5): E0404: cannot access private method `called` of `Use`
decl_before.z42(12,5): E0404: cannot access private method `called` of `Use`
```

（E0404 是 fixture 自己用了 private 方法，与本 change 无关。）

## 全仓影响：**零新增违反**

完整 GREEN（13 stages）全绿 + 自举不动点 3/3，日志里**一条**新的约束诊断都没有
（`grep E0443` 命中的 13 处全是测试名 `PASS ...reports_E0443`）。

⇒ 本轮新发的三类诊断（③ 从不被调用的方法 / ④ 未知型参 / 顶层自由函数）在真实代码上
**违反数恒为 0** ⇒ **必须自带 fixture 门**，否则又是一道从不响的门。
