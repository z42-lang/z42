# z42.crypto — 加密原语

## 职责

z42 标准库的加密算法包：摘要 / MAC / 密钥派生 / 对称加密与 AEAD / 公钥签名与密钥交换。
**算法纯脚本实现**（z42 源码 + `long` 算术，不依赖 OpenSSL / libcrypto，便于审计与多平台含 wasm）；
唯一的 native 入口是 `SecureRandom`（OS 熵源，经 `z42.core` 的 `Entropy` 声明点）。
只提供一次性 `byte[]` 进 / `byte[]` 出的原语，不含流式接口、密钥格式解析、证书 / TLS 封装。

API 参考：[`docs/reference/src/stdlib/crypto.md`](../../../docs/reference/src/stdlib/crypto.md)。

## 功能索引

按能力分组，全部是 `Std.Crypto` 下的 `static class`（RSA 另有两个密钥类）。

| 能力 | 入口 / 文件 |
|------|-----------|
| 摘要 | `Sha1.z42`（legacy）/ `Sha256.z42` / `Sha384.z42` / `Sha512.z42` / `Sha3.z42` / `Md5.z42`（legacy）/ `Blake2b.z42` / `Blake2s.z42` / `Blake3.z42` |
| MAC | `Hmac.z42`（`HmacSha256` / `HmacSha1` / `HmacSha512` 等）/ `Poly1305.z42` |
| 密钥派生 | `Pbkdf2.z42`（HMAC-SHA-256）/ `Hkdf.z42`（`HkdfSha256` / `HkdfSha512` / `HkdfSha1`）/ `Scrypt.z42` |
| 对称加密 / AEAD | `Aes.z42` / `ChaCha20.z42` / `ChaCha20Poly1305.z42` |
| 签名 / 密钥交换 | `Ed25519.z42` / `X25519.z42` / `EcdsaP256.z42` / `EcdsaSecp256k1.z42` / `Rsa.z42`（`Rsa` + `RsaPublicKey` / `RsaPrivateKey`） |
| 随机 / 常数时间比较 | `SecureRandom.z42`（CSPRNG）/ `ConstantTime.z42`（`Equals(byte[], byte[])`，防 MAC 时序侧信道） |

## 基础用法

```z42
using Std.Crypto;

byte[] digest = Sha256.Hash(data);                 // 原始 32 字节 digest
string hex    = Sha256.HashStringHex("abc");       // UTF-8 + Hash + lowercase hex
byte[] mac    = HmacSha256.Compute(key, message);
```

**命名约定**：不同参数形态用不同方法名（`Hash` / `HashString` / `HashHex` / `HashStringHex`，
`Compute` / `ComputeString` / `ComputeHex` / `ComputeStringHex`），而非按参数类型重载
（`byte[]` vs `string` 的重载解析有歧义，见 `crypto.md`）。

## 如何测试验证

```bash
xtask test stdlib z42.crypto    # 本库全部 [Test]：每个算法一个 *_vectors.z42，对照 NIST / RFC 官方向量
```

`tests/secp256k1/` 是目录单元（`source.z42` + `vectors.z42` 共享 namespace，须一起编）。全部通过即可。

## 关联文档
- API 参考与「不支持」清单：[`docs/reference/src/stdlib/crypto.md`](../../../docs/reference/src/stdlib/crypto.md)

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `Sha256.z42` / `Sha384.z42` / `Sha512.z42` | `static class` | SHA-2 家族（FIPS 180-4） |
| `Sha1.z42` / `Md5.z42` | `static class` | legacy / 兼容用途，新设计用 SHA-256 |
| `Sha3.z42` / `Blake2b.z42` / `Blake2s.z42` / `Blake3.z42` | `static class` | SHA-3（FIPS 202）与 BLAKE 系列 |
| `Hmac.z42` | `static class HmacSha*` | HMAC（RFC 2104） |
| `Poly1305.z42` | `static class` | 一次性 MAC（RFC 8439） |
| `Pbkdf2.z42` / `Hkdf.z42` / `Scrypt.z42` | `static class` | 口令哈希 / 密钥派生 |
| `Aes.z42` / `ChaCha20.z42` / `ChaCha20Poly1305.z42` | `static class` | 对称加密 + AEAD |
| `Ed25519.z42` / `X25519.z42` / `EcdsaP256.z42` / `EcdsaSecp256k1.z42` / `Rsa.z42` | `static class`（+ `RsaPublicKey` / `RsaPrivateKey`） | 公钥原语（基于 `z42.numerics` 的 `BigInt`） |
| `SecureRandom.z42` | `static class` | OS 熵源 CSPRNG；wasm32 上抛 `NotSupportedException` |
| `ConstantTime.z42` | `static class` | 常数时间字节比较 |

## 依赖关系

- `z42.core`：`byte[]` / `string` 基元
- `z42.encoding`：`*Hex` 变体用 `Hex.Encode`；字符串入口用 `Utf8`
- `z42.numerics`：`BigInt`，供 Ed25519 / ECDSA / RSA / X25519 / Poly1305

## 待办
- Argon2 / bcrypt 口令哈希
- 流式 / 增量接口；DER / PEM / JWK 密钥格式；RSA 密钥生成
- 完整「不支持」清单见 `crypto.md`
