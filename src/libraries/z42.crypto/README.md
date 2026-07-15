# z42.crypto — 加密原语

## 职责

z42 标准库的加密算法子模块。**纯脚本实现** —— 不依赖 OpenSSL / libcrypto，
所有算法用 z42 源码 + `long` (i64) 算术实现，便于审计 + 多平台 (包括 wasm)。
本包**不**做需要 OS 熵源的 CSPRNG（见 Deferred 段）。

设计参考：详见 [`docs/design/stdlib/crypto.md`](../../../docs/design/stdlib/crypto.md)。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `Sha256.z42` | `static class Sha256` | SHA-256 hash（FIPS 180-4）— 4 个 entry point |
| `Sha1.z42` | `static class Sha1` | SHA-1 hash（FIPS 180-4）— legacy/compat only (SHAttered); use SHA-256 for new designs |
| `Hmac.z42` | `static class HmacSha256` / `HmacSha1` | HMAC-SHA-256 + HMAC-SHA-1（RFC 2104） |
| `Pbkdf2.z42` | `static class Pbkdf2` | PBKDF2-HMAC-SHA256 KDF（RFC 8018 §5.2）— 密码哈希 / 派生密钥 |
| `ConstantTime.z42` | `static class ConstantTime` | 常数时间 `Equals(byte[],byte[])` — 校验 MAC / auth token 防时序侧信道（镜像 .NET FixedTimeEquals） |

## 入口点

### `Std.Crypto.Sha256`（add-z42-crypto, 2026-05-17）

```z42
Sha256.Hash(byte[] data) -> byte[32]               // 原始 digest
Sha256.HashString(string s) -> byte[32]            // UTF-8 + Hash
Sha256.HashHex(byte[] data) -> string              // lowercase hex
Sha256.HashStringHex(string s) -> string           // UTF-8 + Hash + hex
```

### `Std.Crypto.HmacSha256`（add-hmac-sha256, 2026-05-24）

```z42
HmacSha256.Compute(byte[] key, byte[] message) -> byte[32]
HmacSha256.ComputeString(string key, string message) -> byte[32]
HmacSha256.ComputeHex(byte[] key, byte[] message) -> string
HmacSha256.ComputeStringHex(string key, string message) -> string
```

**命名约定**：mirror `Sha256` — distinct method name per parameter form
而非 overload-by-arg-type（z42 当前 overload 解析对 `byte[]` vs `string`
有歧义，见 [`crypto.md`](../../../docs/design/stdlib/crypto.md)）。

## 依赖关系

- **`z42.core`** — `byte[]` / `string` primitives + `Std.Encoding.Utf8` for string→bytes
- **`z42.encoding`** — `Hex.Encode` for `*Hex` 变体

无 native 依赖；纯 z42 脚本算术 + builtin `__str_*` UTF-8 helpers。

## Deferred / Future Work

详见 [`docs/design/stdlib/crypto.md`](../../../docs/design/stdlib/crypto.md) "Deferred / Future Work" 段。
摘要：

- **CSPRNG** (`Std.Crypto.Random`) — 阻塞于 z42.os / z42.io.fs syscall 抽象层
- **AES / ChaCha20** symmetric ciphers — 阻塞于 BigInt 性能 + IV 管理设计
- **RSA / ECDSA** asymmetric — 阻塞于 `z42.math.BigInteger`
- **X.509 / TLS** — 阻塞于上述全部 + ASN.1 parser

注：HMAC-SHA-384 / SHA-512 等 SHA 家族其他变体 v0 不做；后续作为 sibling
类（`HmacSha384` / `HmacSha512`）落地，不破 API。

## 测试

`tests/`：2 个 `.z42` 测试文件 ——

- `sha256_vectors.z42` — NIST FIPS 180-2 Appendix B 全部向量（"" / "abc" /
  56-char two-block / 1000×"a" 多块 / 单字符）+ 32-byte digest length
- `hmac_sha256_vectors.z42` — RFC 4231 §4.2-4.4 / §4.5 / §4.7 / §4.8 全部向量

运行：

```bash
z42 xtask.zpkg test lib         # 完整 stdlib 测试套
```
