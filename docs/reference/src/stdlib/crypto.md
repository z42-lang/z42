<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.crypto/ -->

# z42.crypto —— 加密原语。

> 包路径 `src/libraries/z42.crypto/`；命名空间 `Std.Crypto`

摘要、MAC、密钥派生、对称加密与 AEAD、公钥签名与密钥协商、OS CSPRNG。全部是静态类，
按算法一个类，不需要实例化（RSA 的密钥对象除外）。

所有算法都是纯 z42 脚本实现，不链接 OpenSSL / libcrypto。唯一走 OS 的是
`SecureRandom` 的熵源。参数校验失败、tag 校验失败、padding 非法都抛
`Std.ArgumentException`。

## 不支持

- **没有流式 / 增量接口**：所有摘要、MAC、加密都是一次性 `byte[]` 进、`byte[]` 出，不能分块
  `Update` / `Final`。大文件必须整个读进内存。
- **没有密钥格式解析**：DER / PEM / PKCS#1 / PKCS#8 / JWK / x509 都不支持。RSA 密钥要自己把
  `n` / `e` / `d` 转成 `BigInt` 再构造；ECDSA / Ed25519 / X25519 只接受裸字节。
- **没有密钥生成**：`GeneratePublicKey` 只从既有私钥推公钥；RSA 没有 `GenerateKeyPair`。
  私钥请用 `SecureRandom.GetBytes` 自行生成。
- **RSA 只有 SHA-256 一种摘要**：没有 SHA-1 / SHA-384 / SHA-512 的签名验签变体；也没有 CRT
  解密快路径与 blinding。
- **ECDSA 公钥只有未压缩形态**：64 字节 `X ‖ Y`，不接受 33 字节压缩点，也不带 `0x04` 前缀。
- **AES 没有硬件加速**：无 AES-NI / ARMv8 Crypto Extensions 路径；也没有 Key Wrap（RFC 3394）。
- **AES-GCM 的 tag 不可截断**：固定 16 字节（CCM 才能选 `tagLen`）。
- **BLAKE3 只有默认模式**：没有 keyed hash、没有 `DeriveKey(context, …)`、没有增量或并行分块。
- **没有 Argon2 / bcrypt**：口令哈希只有 `Pbkdf2` 与 `Scrypt`。
- **没有证书 / TLS / JWT 的上层封装**：这里只有原语。
- **wasm32 上没有 CSPRNG**：`SecureRandom` 的所有入口都会抛 `NotSupportedException`。

> ⚠️ `Md5` / `Sha1` 只用于协议强制的旧系统互操作，新设计一律用 SHA-256 及以上。

## Aes


```z42
public static class Aes {
    static byte[] EncryptBlock(byte[] key, byte[] plaintext)
    static byte[] DecryptBlock(byte[] key, byte[] ciphertext)
    static byte[] EncryptCtr(byte[] key, byte[] nonce, byte[] data)
    static byte[] DecryptCtr(byte[] key, byte[] nonce, byte[] data)
    static byte[] EncryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)
    static byte[] DecryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)
    static byte[] EncryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext, int tagLen)
    static byte[] DecryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag, int tagLen)
    static byte[] EncryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] plaintext)
    static byte[] DecryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] ctAndTag)
}
```

### static byte[] EncryptBlock(byte[] key, byte[] plaintext)

加密一个 16 字节块。`key` 为 16 / 24 / 32 字节，分别选择 AES-128 / 192 / 256； key 尺寸或明文非 16 字节时抛 `ArgumentException`。

### static byte[] DecryptBlock(byte[] key, byte[] ciphertext)

解密一个 16 字节块。key 尺寸规则同 EncryptBlock。

### static byte[] EncryptCtr(byte[] key, byte[] nonce, byte[] data)

AES-CTR 加解密（CTR 模式下加密与解密同操作——与密钥流 XOR）。 IV 布局：8 字节 nonce || 8 字节大端 counter（从 0 起），逐块递增。同一 key + nonce 解密时传相同参数即可（密钥流确定）。

### static byte[] DecryptCtr(byte[] key, byte[] nonce, byte[] data)

CTR 是对称的——解密与加密相同。

### static byte[] EncryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

AES-CBC 加密，PKCS#7 填充（RFC 5652 §6.3——补到下一个 16 字节边界，已对齐也追加一整块填充）。 `iv` 必须 16 字节；输出长度恒为 ((data.Length / 16) + 1) * 16。

### static byte[] DecryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

AES-CBC 解密并校验 / 剥离 PKCS#7 填充。输入非 16 的倍数或填充非法抛 `ArgumentException` （通常表示 key / IV 错误或密文损坏）。

### static byte[] EncryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext, int tagLen)

AES-CCM 加密 + 认证。`nonce` 长度决定载荷上限，`tagLen` 控制认证 tag 截断。

### static byte[] DecryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag, int tagLen)

AES-CCM 解密 + 校验。输入 = ciphertext || encTag（tagLen 字节）； tag 不匹配抛 `ArgumentException`（勿吞——篡改信号）。

### static byte[] EncryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] plaintext)

加密 + 认证。输出 = ciphertext（与 `plaintext` 等长）|| tag（16 字节）。 IV 12 字节（96 位）走快路径 J0 = IV || 0x00000001；其它长度按 NIST SP 800-38D §7.1 走 GHASH 推导。IV.Length == 0 拒绝（全零 J0 是安全隐患——同 key 多消息会复用同一 counter 流）。

### static byte[] DecryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] ctAndTag)

解密 + 校验。输入 = ciphertext || 16 字节 tag；tag 不匹配抛 `ArgumentException` （勿吞——tag 失败意味着密文或 AAD 被篡改）。

## Blake2b


```z42
public static class Blake2b {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
    static byte[] Hash256(byte[] data)
    static byte[] Hash256String(string s)
    static string Hash256Hex(byte[] data)
    static string Hash256StringHex(string s)
    static byte[] HashLen(byte[] data, byte[] key, int outLen)
}
```

### static byte[] Hash(byte[] data)

计算默认 64 字节（512 位）BLAKE2b 摘要。

### static byte[] HashString(string s)

对 UTF-8 字符串计算 BLAKE2b 摘要。

### static string HashHex(byte[] data)

计算 BLAKE2b 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 BLAKE2b 摘要并输出小写十六进制。

### static byte[] Hash256(byte[] data)

计算 32 字节（256 位）BLAKE2b 摘要（常用于 IPFS / 区块链等内容寻址）。

### static byte[] Hash256String(string s)

对 UTF-8 字符串计算 256 位 BLAKE2b 摘要。

### static string Hash256Hex(byte[] data)

计算 256 位 BLAKE2b 摘要并输出小写十六进制。

### static string Hash256StringHex(string s)

对 UTF-8 字符串计算 256 位 BLAKE2b 摘要并输出小写十六进制。

### static byte[] HashLen(byte[] data, byte[] key, int outLen)

BLAKE2b 变长 + keyed（MAC）模式：`key.Length` ∈ [0, 64]、`outLen` ∈ [1, 64]，无 key 传 `new byte[0]`。

## Blake2s


```z42
public static class Blake2s {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
    static byte[] HashLen(byte[] data, byte[] key, int outLen)
}
```

### static byte[] Hash(byte[] data)

计算默认 32 字节（256 位）BLAKE2s 摘要。

### static byte[] HashString(string s)

对 UTF-8 字符串计算 BLAKE2s 摘要。

### static string HashHex(byte[] data)

计算 BLAKE2s 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 BLAKE2s 摘要并输出小写十六进制。

### static byte[] HashLen(byte[] data, byte[] key, int outLen)

BLAKE2s 变长 + keyed（MAC）模式：`key.Length` ∈ [0, 32]、`outLen` ∈ [1, 32]，无 key 传 `new byte[0]`。

## Blake3


```z42
public static class Blake3 {
    static byte[] Hash(byte[] data)
    static byte[] Hash256(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
    static byte[] HashLen(byte[] data, int outLen)
}
```

### static byte[] Hash(byte[] data)

计算 BLAKE3 摘要（默认 32 字节）。

### static byte[] Hash256(byte[] data)

`Hash` 的同义方法，返回 32 字节 BLAKE3 摘要。

### static byte[] HashString(string s)

对 UTF-8 字符串计算 BLAKE3 摘要。

### static string HashHex(byte[] data)

计算 BLAKE3 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 BLAKE3 摘要并输出小写十六进制。

### static byte[] HashLen(byte[] data, int outLen)

BLAKE3 可扩展输出（XOF）：`outLen` ≥ 0，前 N 字节与更长输出的前 N 字节一致。

## ChaCha20


```z42
public static class ChaCha20 {
    static byte[] Crypt(byte[] key, byte[] nonce, int counter, byte[] data)
    static byte[] Encrypt(byte[] key, byte[] nonce, byte[] data)
    static byte[] Decrypt(byte[] key, byte[] nonce, byte[] data)
    static byte[] Block(byte[] key, byte[] nonce, int counter)
}
```

### static byte[] Crypt(byte[] key, byte[] nonce, int counter, byte[] data)

加密 / 解密（对称，与密钥流 XOR）。独立使用时 `counter` 从 1 起（RFC 8439 §2.4）； 仅派生 Poly1305 一次性 key 时传 0。

### static byte[] Encrypt(byte[] key, byte[] nonce, byte[] data)

便捷形式，默认 counter=1（RFC 8439 独立使用）。

### static byte[] Decrypt(byte[] key, byte[] nonce, byte[] data)

解密——与加密同义。

### static byte[] Block(byte[] key, byte[] nonce, int counter)

产生 (key, nonce, counter) 的一个 64 字节密钥流块，供 Poly1305 key 派生（counter=0 路径）使用。

## ChaCha20Poly1305


```z42
public static class ChaCha20Poly1305 {
    static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)
    static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)
}
```

### static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)

加密 + 认证。输出 = ciphertext（与 `plaintext` 等长）|| tag（16 字节）。

### static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)

解密 + 校验。输入 = ciphertext || 16 字节 tag；tag 不匹配抛 `ArgumentException` （勿吞——表示密文或 AAD 被篡改）。

## ConstantTime


```z42
public static class ConstantTime {
    static bool Equals(byte[] a, byte[] b)
}
```

### static bool Equals(byte[] a, byte[] b)

无提前退出的字节数组比较，运行时间只依赖长度（非秘密），从不依赖首个差异位置。 长度不同立即返回 false（只泄漏长度）；长度相同时始终逐字节检查，累计差异决定结果。

## EcdsaP256


```z42
public static class EcdsaP256 {
    static byte[] GeneratePublicKey(byte[] privateScalar)
    static byte[] Sign(byte[] privateScalar, byte[] message)
    static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

### static byte[] GeneratePublicKey(byte[] privateScalar)

从 32 字节私钥标量派生 64 字节未压缩公钥（X ‖ Y）。

### static byte[] Sign(byte[] privateScalar, byte[] message)

用 32 字节私钥标量对 `message` 签名（RFC 6979 确定性），返回 64 字节（r ‖ s，各 32 字节大端）。

### static bool Verify(byte[] publicKey, byte[] message, byte[] signature)

校验 64 字节签名；无效签名 / 非曲线上点 / 越界 r、s 返回 `false`，仅公钥非 64 字节抛 `ArgumentException`。

## EcdsaSecp256k1


```z42
public static class EcdsaSecp256k1 {
    static byte[] GeneratePublicKey(byte[] privateScalar)
    static byte[] Sign(byte[] privateScalar, byte[] message)
    static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

### static byte[] GeneratePublicKey(byte[] privateScalar)

从 32 字节私钥标量派生 64 字节未压缩公钥（X ‖ Y）。

### static byte[] Sign(byte[] privateScalar, byte[] message)

用 32 字节私钥标量对 `message` 签名（RFC 6979 确定性），返回 64 字节（r ‖ s，各 32 字节大端）。

### static bool Verify(byte[] publicKey, byte[] message, byte[] signature)

校验 64 字节签名；无效签名 / 非曲线上点 / 越界 r、s 返回 `false`，仅公钥非 64 字节抛 `ArgumentException`。

## Ed25519


```z42
public static class Ed25519 {
    static byte[] GeneratePublicKey(byte[] secretKey)
    static byte[] Sign(byte[] secretKey, byte[] message)
    static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

### static byte[] GeneratePublicKey(byte[] secretKey)

从 32 字节私钥派生 32 字节 Ed25519 公钥。

### static byte[] Sign(byte[] secretKey, byte[] message)

用 32 字节私钥对 `message` 签名，输出 64 字节 `R || S`（RFC 8032 §5.1.6）。

### static bool Verify(byte[] publicKey, byte[] message, byte[] signature)

用 32 字节公钥校验 64 字节签名；有效返回 `true`。签名内容错误返回 `false`（仅长度错误抛 `ArgumentException`）。

## HkdfSha256


```z42
public static class HkdfSha256 {
    static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)
    static byte[] Extract(byte[] salt, byte[] ikm)
    static byte[] Expand(byte[] prk, byte[] info, int length)
    static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
}
```

### static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)

一次性 Extract + Expand。两阶段单独暴露（`Extract` / `Expand`）， 供需要在其间穿插其他状态（如 TLS 1.3 密钥表）的调用方使用。

### static byte[] Extract(byte[] salt, byte[] ikm)

HKDF-Extract（RFC 5869 §2.2）：PRK = HMAC-Hash(salt, IKM)。空 salt → 32 个零字节，输出 32 字节。

### static byte[] Expand(byte[] prk, byte[] info, int length)

HKDF-Expand（RFC 5869 §2.3）：把 PRK 扩展成 `length` 字节输出密钥材料，`info` 作上下文绑定。

### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

十六进制便捷变体，用于文档 / 调试。

## HkdfSha512


```z42
public static class HkdfSha512 {
    static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)
    static byte[] Extract(byte[] salt, byte[] ikm)
    static byte[] Expand(byte[] prk, byte[] info, int length)
    static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
}
```

### static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)

一次性 Extract + Expand（HKDF-SHA-512，HashLen 64 字节）。

### static byte[] Extract(byte[] salt, byte[] ikm)

HKDF-Extract：PRK = HMAC-SHA512(salt, IKM)，输出 64 字节。

### static byte[] Expand(byte[] prk, byte[] info, int length)

HKDF-Expand：把 PRK 扩展成 `length` 字节输出密钥材料。

### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

十六进制便捷变体。

## HkdfSha1


```z42
public static class HkdfSha1 {
    static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)
    static byte[] Extract(byte[] salt, byte[] ikm)
    static byte[] Expand(byte[] prk, byte[] info, int length)
    static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
}
```

### static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)

一次性 Extract + Expand（HKDF-SHA-1，HashLen 20 字节）。

### static byte[] Extract(byte[] salt, byte[] ikm)

HKDF-Extract：PRK = HMAC-SHA1(salt, IKM)，输出 20 字节。

### static byte[] Expand(byte[] prk, byte[] info, int length)

HKDF-Expand：把 PRK 扩展成 `length` 字节输出密钥材料。

### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

十六进制便捷变体。

## HmacSha256


```z42
public static class HmacSha256 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

### static byte[] Compute(byte[] key, byte[] message)

计算 `message` 在 `key` 下的 HMAC-SHA256，返回 32 字节摘要。

### static byte[] ComputeString(string key, string message)

对 UTF-8 编码的字符串 key / message 计算 HMAC-SHA256（返回 32 字节）。

### static string ComputeHex(byte[] key, byte[] message)

计算 HMAC-SHA256 并输出小写十六进制（64 字符）。

### static string ComputeStringHex(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA256 并输出小写十六进制。

## HmacSha1


```z42
public static class HmacSha1 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

### static byte[] Compute(byte[] key, byte[] message)

计算 `message` 在 `key` 下的 HMAC-SHA1，返回 20 字节摘要。

### static byte[] ComputeString(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA1。

### static string ComputeHex(byte[] key, byte[] message)

计算 HMAC-SHA1 并输出小写十六进制。

### static string ComputeStringHex(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA1 并输出小写十六进制。

## HmacSha512


```z42
public static class HmacSha512 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

### static byte[] Compute(byte[] key, byte[] message)

计算 `message` 在 `key` 下的 HMAC-SHA512，返回 64 字节摘要。

### static byte[] ComputeString(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA512。

### static string ComputeHex(byte[] key, byte[] message)

计算 HMAC-SHA512 并输出小写十六进制。

### static string ComputeStringHex(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA512 并输出小写十六进制。

## HmacSha384


```z42
public static class HmacSha384 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

### static byte[] Compute(byte[] key, byte[] message)

计算 `message` 在 `key` 下的 HMAC-SHA384，返回 48 字节摘要。

### static byte[] ComputeString(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA384。

### static string ComputeHex(byte[] key, byte[] message)

计算 HMAC-SHA384 并输出小写十六进制。

### static string ComputeStringHex(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-SHA384 并输出小写十六进制。

## HmacMd5


```z42
public static class HmacMd5 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

### static byte[] Compute(byte[] key, byte[] message)

计算 `message` 在 `key` 下的 HMAC-MD5，返回 16 字节摘要。

### static byte[] ComputeString(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-MD5。

### static string ComputeHex(byte[] key, byte[] message)

计算 HMAC-MD5 并输出小写十六进制。

### static string ComputeStringHex(string key, string message)

对 UTF-8 字符串 key / message 计算 HMAC-MD5 并输出小写十六进制。

## Md5


```z42
public static class Md5 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

### static byte[] Hash(byte[] data)

计算 MD5 原始摘要（16 字节）。

### static byte[] HashString(string s)

对 UTF-8 编码后的字符串计算 MD5 摘要。

### static string HashHex(byte[] data)

计算 MD5 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 MD5 摘要并输出小写十六进制。

## Pbkdf2


```z42
public static class Pbkdf2 {
    static byte[] DeriveKey(byte[] password, byte[] salt, int iterations, int dkLen)
    static byte[] DeriveKeyString(string password, string salt, int iterations, int dkLen)
    static string DeriveKeyHex(byte[] password, byte[] salt, int iterations, int dkLen)
    static string DeriveKeyStringHex(string password, string salt, int iterations, int dkLen)
}
```

### static byte[] DeriveKey(byte[] password, byte[] salt, int iterations, int dkLen)

PBKDF2-HMAC-SHA256 派生，返回 `dkLen` 字节。`iterations < 1` 或 `dkLen < 1` 抛 `ArgumentException`。

### static byte[] DeriveKeyString(string password, string salt, int iterations, int dkLen)

字符串 password / salt 便捷形式，按 UTF-8 编码。

### static string DeriveKeyHex(byte[] password, byte[] salt, int iterations, int dkLen)

派生并输出小写十六进制（长度 2 × dkLen）。

### static string DeriveKeyStringHex(string password, string salt, int iterations, int dkLen)

对字符串 password / salt 派生并输出小写十六进制。

## Poly1305


```z42
public static class Poly1305 {
    static byte[] Mac(byte[] key, byte[] message)
    static string MacHex(byte[] key, byte[] message)
}
```

### static byte[] Mac(byte[] key, byte[] message)

在 32 字节一次性 key 下计算 `message` 的 16 字节认证 tag，key 长度错误抛 `ArgumentException`。

### static string MacHex(byte[] key, byte[] message)

计算 tag 并输出十六进制。

## RsaPublicKey

RSA 公钥（模数 + 公钥指数）。

```z42
public class RsaPublicKey {
    BigInt n
    BigInt e
    RsaPublicKey(BigInt n, BigInt e)
}
```

### BigInt n

模数。

### BigInt e

公钥指数。

### RsaPublicKey(BigInt n, BigInt e)

用模数 n 与公钥指数 e 构造。

## RsaPrivateKey

RSA 私钥（模数 + 私钥指数）。保留公钥指数以便派生匹配的 `RsaPublicKey` 而无需单独跟踪 e。

```z42
public class RsaPrivateKey {
    BigInt n
    BigInt e
    BigInt d
    RsaPrivateKey(BigInt n, BigInt e, BigInt d)
    RsaPublicKey GetPublicKey()
}
```

### BigInt n

模数。

### BigInt e

公钥指数。

### BigInt d

私钥指数。

### RsaPrivateKey(BigInt n, BigInt e, BigInt d)

用模数 n、公钥指数 e、私钥指数 d 构造。

### RsaPublicKey GetPublicKey()

派生匹配的公钥。

## Rsa


```z42
public static class Rsa {
    static byte[] SignPkcs1v15Sha256(RsaPrivateKey privateKey, byte[] message)
    static bool VerifyPkcs1v15Sha256(RsaPublicKey publicKey, byte[] message, byte[] signature)
    static byte[] SignPssSha256(RsaPrivateKey privateKey, byte[] message, byte[] salt)
    static bool VerifyPssSha256(RsaPublicKey publicKey, byte[] message, byte[] signature, int saltLen)
    static byte[] EncryptOaepSha256(RsaPublicKey publicKey, byte[] message, byte[] label, byte[] seed)
    static byte[] DecryptOaepSha256(RsaPrivateKey privateKey, byte[] ciphertext, byte[] label)
    static byte[] RawPublicOp(RsaPublicKey publicKey, byte[] message)
    static byte[] RawPrivateOp(RsaPrivateKey privateKey, byte[] ciphertext)
}
```

### static byte[] SignPkcs1v15Sha256(RsaPrivateKey privateKey, byte[] message)

用 SHA-256 + PKCS#1 v1.5 填充对 `message` 签名，输出恰好 `n.byteLength()` 字节。

### static bool VerifyPkcs1v15Sha256(RsaPublicKey publicKey, byte[] message, byte[] signature)

校验 SHA-256(message) 的 PKCS#1 v1.5 签名；有效返回 true。签名内容错误返回 false（仅长度错误抛异常）。

### static byte[] SignPssSha256(RsaPrivateKey privateKey, byte[] message, byte[] salt)

RSASSA-PSS / SHA-256 / MGF1-SHA-256 签名，`salt` 长度由调用方定（推荐 32 字节），输出 n 的字节长度。

### static bool VerifyPssSha256(RsaPublicKey publicKey, byte[] message, byte[] signature, int saltLen)

校验 RSASSA-PSS / SHA-256 签名，`saltLen` 必须与签名时一致（RFC 8017 §9.1.2 步骤 11——验签方知道 salt 长度而非 salt 本身）。

### static byte[] EncryptOaepSha256(RsaPublicKey publicKey, byte[] message, byte[] label, byte[] seed)

RSA-OAEP / SHA-256 / MGF1-SHA-256 加密。`label` 可为空（用于上下文绑定），`seed` 必须 32 字节。

### static byte[] DecryptOaepSha256(RsaPrivateKey privateKey, byte[] ciphertext, byte[] label)

RSA-OAEP / SHA-256 / MGF1-SHA-256 解密，返回明文。填充非法抛 `ArgumentException` （勿在调用层区分错误——区分 padding 错误与值错误的计时泄漏风险正是 OAEP 要缓解的）。

### static byte[] RawPublicOp(RsaPublicKey publicKey, byte[] message)

裸 `message^e mod n` 模幂（RSAEP，RFC 8017 §5.1.1），无 padding。输入须在 [0, n)，返回 n.byteLength() 字节。

### static byte[] RawPrivateOp(RsaPrivateKey privateKey, byte[] ciphertext)

裸 `ciphertext^d mod n`（RSADP，RFC 8017 §5.1.2），无 padding 剥离，输入约束同 RawPublicOp。

## Scrypt


```z42
public static class Scrypt {
    static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)
}
```

### static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)

用 scrypt(N, r, p) 从 `password` + `salt` 派生 `dkLen` 字节。 约束：N 为 2 的幂且 ≥ 2，r ≥ 1，p ≥ 1，r × p < 2^30，dkLen ≥ 0。

## SecureRandom


```z42
public static class SecureRandom {
    static byte[] GetBytes(int n)
    static int NextInt()
    static long NextLong()
    static int NextU32Bounded(int bound)
}
```

### static byte[] GetBytes(int n)

从 OS CSPRNG 填充 n 字节（n ≥ 0）。

### static int NextInt()

整个 i32 范围均匀分布的随机 int（含负数）。由 4 个 CSPRNG 字节大端构建。

### static long NextLong()

整个 i64 范围均匀分布的随机 long，由 8 个 CSPRNG 字节大端构建。

### static int NextU32Bounded(int bound)

[0, bound) 均匀分布的随机 int，用 rejection sampling 避免模偏差；bound ≤ 0 抛 `ArgumentException`。

## Sha1


```z42
public static class Sha1 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

### static byte[] Hash(byte[] data)

计算 SHA-1 原始摘要（20 字节）。

### static byte[] HashString(string s)

对 UTF-8 编码后的字符串计算 SHA-1 摘要。

### static string HashHex(byte[] data)

计算 SHA-1 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 SHA-1 摘要并输出小写十六进制。

## Sha256


```z42
public static class Sha256 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

### static byte[] Hash(byte[] data)

计算 SHA-256 原始摘要（32 字节）。

### static byte[] HashString(string s)

对 UTF-8 编码后的字符串计算 SHA-256 摘要。

### static string HashHex(byte[] data)

计算 SHA-256 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 SHA-256 摘要并输出小写十六进制。

## Sha3


```z42
public static class Sha3 {
    static byte[] Hash224(byte[] data)
    static byte[] Hash256(byte[] data)
    static byte[] Hash384(byte[] data)
    static byte[] Hash512(byte[] data)
    static byte[] Hash224String(string s)
    static byte[] Hash256String(string s)
    static byte[] Hash384String(string s)
    static byte[] Hash512String(string s)
    static string Hash224Hex(byte[] data)
    static string Hash256Hex(byte[] data)
    static string Hash384Hex(byte[] data)
    static string Hash512Hex(byte[] data)
    static string Hash224StringHex(string s)
    static string Hash256StringHex(string s)
    static string Hash384StringHex(string s)
    static string Hash512StringHex(string s)
    static byte[] KeccakLegacy256(byte[] data)
    static byte[] KeccakLegacy512(byte[] data)
    static byte[] KeccakLegacy256String(string s)
    static byte[] KeccakLegacy512String(string s)
    static string KeccakLegacy256Hex(byte[] data)
    static string KeccakLegacy512Hex(byte[] data)
    static string KeccakLegacy256StringHex(string s)
    static string KeccakLegacy512StringHex(string s)
    static byte[] Shake128(byte[] data, int outputLen)
    static byte[] Shake256(byte[] data, int outputLen)
    static byte[] Shake128String(string s, int outputLen)
    static byte[] Shake256String(string s, int outputLen)
    static string Shake128Hex(byte[] data, int outputLen)
    static string Shake256Hex(byte[] data, int outputLen)
    static string Shake128StringHex(string s, int outputLen)
    static string Shake256StringHex(string s, int outputLen)
}
```

### static byte[] Hash224(byte[] data)

计算 SHA3-224 原始摘要（28 字节）。

### static byte[] Hash256(byte[] data)

计算 SHA3-256 原始摘要（32 字节）。

### static byte[] Hash384(byte[] data)

计算 SHA3-384 原始摘要（48 字节）。

### static byte[] Hash512(byte[] data)

计算 SHA3-512 原始摘要（64 字节）。

### static byte[] Hash224String(string s)

对 UTF-8 字符串计算 SHA3-224 摘要。

### static byte[] Hash256String(string s)

对 UTF-8 字符串计算 SHA3-256 摘要。

### static byte[] Hash384String(string s)

对 UTF-8 字符串计算 SHA3-384 摘要。

### static byte[] Hash512String(string s)

对 UTF-8 字符串计算 SHA3-512 摘要。

### static string Hash224Hex(byte[] data)

计算 SHA3-224 摘要并输出小写十六进制。

### static string Hash256Hex(byte[] data)

计算 SHA3-256 摘要并输出小写十六进制。

### static string Hash384Hex(byte[] data)

计算 SHA3-384 摘要并输出小写十六进制。

### static string Hash512Hex(byte[] data)

计算 SHA3-512 摘要并输出小写十六进制。

### static string Hash224StringHex(string s)

对 UTF-8 字符串计算 SHA3-224 摘要并输出小写十六进制。

### static string Hash256StringHex(string s)

对 UTF-8 字符串计算 SHA3-256 摘要并输出小写十六进制。

### static string Hash384StringHex(string s)

对 UTF-8 字符串计算 SHA3-384 摘要并输出小写十六进制。

### static string Hash512StringHex(string s)

对 UTF-8 字符串计算 SHA3-512 摘要并输出小写十六进制。

### static byte[] KeccakLegacy256(byte[] data)

计算 legacy Keccak-256 原始摘要（32 字节，域分隔字节 0x01）。

### static byte[] KeccakLegacy512(byte[] data)

计算 legacy Keccak-512 原始摘要（64 字节，域分隔字节 0x01）。

### static byte[] KeccakLegacy256String(string s)

对 UTF-8 字符串计算 legacy Keccak-256 摘要。

### static byte[] KeccakLegacy512String(string s)

对 UTF-8 字符串计算 legacy Keccak-512 摘要。

### static string KeccakLegacy256Hex(byte[] data)

计算 legacy Keccak-256 摘要并输出小写十六进制。

### static string KeccakLegacy512Hex(byte[] data)

计算 legacy Keccak-512 摘要并输出小写十六进制。

### static string KeccakLegacy256StringHex(string s)

对 UTF-8 字符串计算 legacy Keccak-256 摘要并输出小写十六进制。

### static string KeccakLegacy512StringHex(string s)

对 UTF-8 字符串计算 legacy Keccak-512 摘要并输出小写十六进制。

### static byte[] Shake128(byte[] data, int outputLen)

SHAKE128 可扩展输出（`outputLen` ≥ 0）。

### static byte[] Shake256(byte[] data, int outputLen)

SHAKE256 可扩展输出（`outputLen` ≥ 0）。

### static byte[] Shake128String(string s, int outputLen)

对 UTF-8 字符串做 SHAKE128 可扩展输出。

### static byte[] Shake256String(string s, int outputLen)

对 UTF-8 字符串做 SHAKE256 可扩展输出。

### static string Shake128Hex(byte[] data, int outputLen)

SHAKE128 可扩展输出并输出小写十六进制。

### static string Shake256Hex(byte[] data, int outputLen)

SHAKE256 可扩展输出并输出小写十六进制。

### static string Shake128StringHex(string s, int outputLen)

对 UTF-8 字符串做 SHAKE128 可扩展输出并输出小写十六进制。

### static string Shake256StringHex(string s, int outputLen)

对 UTF-8 字符串做 SHAKE256 可扩展输出并输出小写十六进制。

## Sha384


```z42
public static class Sha384 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

### static byte[] Hash(byte[] data)

计算 SHA-384 原始摘要（48 字节）。

### static byte[] HashString(string s)

对 UTF-8 编码后的字符串计算 SHA-384 摘要。

### static string HashHex(byte[] data)

计算 SHA-384 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 SHA-384 摘要并输出小写十六进制。

## Sha512


```z42
public static class Sha512 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
    static long[] _initialHash()
    static byte[] _pad(byte[] data)
    static void _compressBlock(long[] h, byte[] block, int offset)
    static long _lshr64(long x, int n)
    static long _rotr64(long x, int n)
    static long _readBE64(byte[] data, int offset)
    static void _writeBE64(byte[] data, int offset, long v)
}
```

### static byte[] Hash(byte[] data)

计算 SHA-512 原始摘要（64 字节）。

### static byte[] HashString(string s)

对 UTF-8 编码后的字符串计算 SHA-512 摘要。

### static string HashHex(byte[] data)

计算 SHA-512 摘要并输出小写十六进制。

### static string HashStringHex(string s)

对 UTF-8 字符串计算 SHA-512 摘要并输出小写十六进制。

### static long[] _initialHash()


### static byte[] _pad(byte[] data)


### static void _compressBlock(long[] h, byte[] block, int offset)


### static long _lshr64(long x, int n)


### static long _rotr64(long x, int n)


### static long _readBE64(byte[] data, int offset)


### static void _writeBE64(byte[] data, int offset, long v)


## X25519


```z42
public static class X25519 {
    static int U_BASE
    static byte[] Clamp(byte[] scalar)
    static byte[] ScalarMult(byte[] scalar, byte[] point)
    static byte[] ScalarMultBase(byte[] scalar)
}
```

### static int U_BASE

Curve25519 基点 u 坐标（RFC 7748 §4.1）。

### static byte[] Clamp(byte[] scalar)

按 RFC 7748 §5 clamp 32 字节标量：清 byte0 低 3 位、清 byte31 最高位、置 byte31 次高位， 返回新的 clamp 副本（不改入参）。

### static byte[] ScalarMult(byte[] scalar, byte[] point)

X25519 标量乘：`scalar × point`（各 32 字节小端编码）。`scalar` 内部自动 clamp（RFC 7748 §5）， 调用方不必预 clamp。

### static byte[] ScalarMultBase(byte[] scalar)

便捷形式：`scalar × 基点`（u=9），即由私钥派生公钥。

