# Tasks: crypto 内部 ct-compare 收敛到 ConstantTime.Equals

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16 | 类型：refactor（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32）实施+验证。

**变更说明：** review §2.3 follow-up——crypto 内部各手写常数时间比较循环（`diff |= a^b`）散落多处。
`add-crypto-constant-time-equals` 落地公开 `ConstantTime.Equals` 后，把**确认整数组等长比较**的 4 处
收敛到该单一 vetted 原语：ChaCha20Poly1305 tag / AES-GCM tag / AES-CCM tag / RSA EMSA-PKCS1 编码比较。

**保留不动（非全数组等长比较，直接替换会破坏正确性）：**
- `Rsa.z42` PSS `hPrime vs H`（H 长度未逐字核对，保守不动）
- `Rsa.z42` OAEP `DB[0..hLen] vs lHash`（DB 长于 hLen，是前缀比较，须切片才能用等长 API）

**原因：** DRY + 让所有 AEAD tag 校验走同一常数时间实现（单点正确性）。

**文档影响：** 纯内部实现收敛，无对外行为变化（tag 不符仍抛同异常、RSA 验签仍返 bool）。无文档同步。

- [x] 1.1 `ChaCha20Poly1305.z42`：tag 比较 → `ConstantTime.Equals(actual, expected)`
- [x] 1.2 `Aes.z42`：GCM + CCM tag 比较 → `ConstantTime.Equals`
- [x] 1.3 `Rsa.z42`：EMSA-PKCS1 `em vs expected` → `ConstantTime.Equals`（含长度检查，删显式 guard）
- [x] 1.4 GREEN：worktree 内 `xtask test` 全绿（crypto 28 文件向量测试覆盖，行为不变）
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：纯内部实现收敛，无对外行为/API/依赖变化 → 无文档同步
