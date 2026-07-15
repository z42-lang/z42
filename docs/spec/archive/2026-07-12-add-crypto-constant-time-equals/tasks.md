# Tasks: 公开常数时间比较 API（ConstantTime.Equals）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：feat（最小化模式，additive 安全 API）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32）实施+验证。

**变更说明：** review §2.3——AEAD 内部各手写常数时间 tag 比较（ChaCha20Poly1305/AES-GCM/CCM 等 5 处），
但**无对外公开 API**，用户校验 HMAC/auth token/密码哈希只能用 `==`（首字节短路→时序泄漏匹配前缀长度，
可逐字节伪造）。新增 `Std.Crypto.ConstantTime.Equals(byte[],byte[])`：长度不等即 false（长度非秘密，
对齐 .NET FixedTimeEquals），等长则无短路遍历全部字节累积差异。

**原因：** review §2.3——缺公开常数时间比较 API，用户被迫用时序泄漏的 `==`。

**文档影响：** 新增对外 API + 新文件。crypto README「核心文件」表加 ConstantTime.z42 行。无 book 变更
（常数时间比较为局部安全原语，非跨组件机制）。

- [x] 1.1 新文件 `ConstantTime.z42`：`public static bool Equals(byte[] a, byte[] b)`（无短路 diff|=^）
- [x] 1.2 回归测试 `tests/constant_time.z42`：等长相等/首字节差/末字节差/长度不等/空数组/高位字节无符号扩展
- [x] 1.3 crypto README 核心文件表同步
- [x] 1.4 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁

## 未做（本 change 外，独立 follow-up）
- 把 5 处 AEAD 内部手写 ct-compare 收敛到 `ConstantTime.Equals`（DRY；改 crypto 内部有风险，独立 refactor）
- 其余 §2.3 crypto 安全项（ECDSA/Ed25519 恒时 ladder、CBC padding oracle、AEAD nonce 唯一性）——较高风险，独立评估

## doc-check
- [x] 触发矩阵：新增文件 + 对外 API → crypto README 核心文件表已更新；无 book 变更
- [x] 本次触及文档相对链接可解析
