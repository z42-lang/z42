# Spec: 跨包 `ref` 对称性

## ADDED Requirements

### Requirement: 跨包调用点受 `ref` 对称性约束

#### Scenario: 跨包漏写 `ref`
- **WHEN** pkgA 导出 `public static void Inc(ref int x)`，pkgB 调用 `A.Inc(v)`
- **THEN** 报 `E0472`，编译失败
- **注**：本变更前**编译通过且写入静默丢失**

#### Scenario: 跨包正确写 `ref`
- **WHEN** 同上但 pkgB 写 `A.Inc(ref v)`
- **THEN** 编译通过，调用后 `v` 被改

#### Scenario: 跨包多写 `ref`
- **WHEN** pkgA 导出 `public static void ByValue(int x)`，pkgB 调用 `A.ByValue(ref v)`
- **THEN** 报 `E0473`

#### Scenario: 实例方法的 this 槽不错位
- **WHEN** pkgA 导出实例方法 `public void Inc(ref int x)`，pkgB 调用 `new A().Inc(v)`
- **THEN** 报 `E0472`（而非把 `this` 槽误当第一个形参）

#### Scenario: 自由函数同样受检
- **WHEN** pkgA 导出自由函数 `void Inc(ref int x)`，pkgB 裸名调用 `Inc(v)`
- **THEN** 报 `E0472`

### Requirement: 旧包不产生假阳性

#### Scenario: 引用没有 `$ByRef` 通道的包
- **WHEN** pkgB 引用一个**本变更之前**编译出的 pkgA（其 param attr 块里没有 `$ByRef`），
  且 pkgA 有 `ref` 形参的方法，pkgB 写 `A.Inc(ref v)`
- **THEN** **不报错** —— 该签名的 ref 信息标记为「未知」，整条对称性检查跳过
- **注**：若按「空数组 = 全都不是 ref」判断，这里会误报 `E0473`。
  这是本变更最容易出错的一处，`RefInfoKnown` 显式布尔就是为它而加

#### Scenario: 混合引用
- **WHEN** pkgB 同时引用旧 pkgA 与新 pkgC
- **THEN** 对 pkgC 的调用受检，对 pkgA 的不受检——逐签名判断，不是全局开关

### Requirement: 哨兵对旧读者无害

#### Scenario: 旧读者读新包
- **WHEN** 一个只认 `$Default` / `$Caller:` 的读者读到带 `$ByRef` 的 param attr 块
- **THEN** 忽略该哨兵，其余元数据照常读出 —— attr-ref 列表本就可扩展
