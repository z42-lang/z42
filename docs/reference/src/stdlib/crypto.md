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

[`src/libraries/z42.crypto/src/Aes.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L7)


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

成员：[`EncryptBlock`](#Std.Crypto.Aes.EncryptBlock(byte[],byte[])) · [`DecryptBlock`](#Std.Crypto.Aes.DecryptBlock(byte[],byte[])) · [`EncryptCtr`](#Std.Crypto.Aes.EncryptCtr(byte[],byte[],byte[])) · [`DecryptCtr`](#Std.Crypto.Aes.DecryptCtr(byte[],byte[],byte[])) · [`EncryptCbcPkcs7`](#Std.Crypto.Aes.EncryptCbcPkcs7(byte[],byte[],byte[])) · [`DecryptCbcPkcs7`](#Std.Crypto.Aes.DecryptCbcPkcs7(byte[],byte[],byte[])) · [`EncryptCcm`](#Std.Crypto.Aes.EncryptCcm(byte[],byte[],byte[],byte[],int)) · [`DecryptCcm`](#Std.Crypto.Aes.DecryptCcm(byte[],byte[],byte[],byte[],int)) · [`EncryptGcm`](#Std.Crypto.Aes.EncryptGcm(byte[],byte[],byte[],byte[])) · [`DecryptGcm`](#Std.Crypto.Aes.DecryptGcm(byte[],byte[],byte[],byte[]))

<a id="Std.Crypto.Aes.EncryptBlock(byte[],byte[])"></a>
### static byte[] EncryptBlock(byte[] key, byte[] plaintext)

[`src/libraries/z42.crypto/src/Aes.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L15)

加密一个 16 字节块。`key` 为 16 / 24 / 32 字节，分别选择 AES-128 / 192 / 256； key 尺寸或明文非 16 字节时抛 `ArgumentException`。

<a id="Std.Crypto.Aes.DecryptBlock(byte[],byte[])"></a>
### static byte[] DecryptBlock(byte[] key, byte[] ciphertext)

[`src/libraries/z42.crypto/src/Aes.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L38)

解密一个 16 字节块。key 尺寸规则同 EncryptBlock。

<a id="Std.Crypto.Aes.EncryptCtr(byte[],byte[],byte[])"></a>
### static byte[] EncryptCtr(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:63`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L63)

AES-CTR 加解密（CTR 模式下加密与解密同操作——与密钥流 XOR）。 IV 布局：8 字节 nonce || 8 字节大端 counter（从 0 起），逐块递增。同一 key + nonce 解密时传相同参数即可（密钥流确定）。

<a id="Std.Crypto.Aes.DecryptCtr(byte[],byte[],byte[])"></a>
### static byte[] DecryptCtr(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L101)

CTR 是对称的——解密与加密相同。

<a id="Std.Crypto.Aes.EncryptCbcPkcs7(byte[],byte[],byte[])"></a>
### static byte[] EncryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L107)

AES-CBC 加密，PKCS#7 填充（RFC 5652 §6.3——补到下一个 16 字节边界，已对齐也追加一整块填充）。 `iv` 必须 16 字节；输出长度恒为 ((data.Length / 16) + 1) * 16。

<a id="Std.Crypto.Aes.DecryptCbcPkcs7(byte[],byte[],byte[])"></a>
### static byte[] DecryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:144`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L144)

AES-CBC 解密并校验 / 剥离 PKCS#7 填充。输入非 16 的倍数或填充非法抛 `ArgumentException` （通常表示 key / IV 错误或密文损坏）。

<a id="Std.Crypto.Aes.EncryptCcm(byte[],byte[],byte[],byte[],int)"></a>
### static byte[] EncryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext, int tagLen)

[`src/libraries/z42.crypto/src/Aes.z42:201`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L201)

AES-CCM 加密 + 认证。`nonce` 长度决定载荷上限，`tagLen` 控制认证 tag 截断。

<a id="Std.Crypto.Aes.DecryptCcm(byte[],byte[],byte[],byte[],int)"></a>
### static byte[] DecryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag, int tagLen)

[`src/libraries/z42.crypto/src/Aes.z42:226`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L226)

AES-CCM 解密 + 校验。输入 = ciphertext || encTag（tagLen 字节）； tag 不匹配抛 `ArgumentException`（勿吞——篡改信号）。

<a id="Std.Crypto.Aes.EncryptGcm(byte[],byte[],byte[],byte[])"></a>
### static byte[] EncryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] plaintext)

[`src/libraries/z42.crypto/src/Aes.z42:432`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L432)

加密 + 认证。输出 = ciphertext（与 `plaintext` 等长）|| tag（16 字节）。 IV 12 字节（96 位）走快路径 J0 = IV || 0x00000001；其它长度按 NIST SP 800-38D §7.1 走 GHASH 推导。IV.Length == 0 拒绝（全零 J0 是安全隐患——同 key 多消息会复用同一 counter 流）。

<a id="Std.Crypto.Aes.DecryptGcm(byte[],byte[],byte[],byte[])"></a>
### static byte[] DecryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] ctAndTag)

[`src/libraries/z42.crypto/src/Aes.z42:456`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L456)

解密 + 校验。输入 = ciphertext || 16 字节 tag；tag 不匹配抛 `ArgumentException` （勿吞——tag 失败意味着密文或 AAD 被篡改）。

## Blake2b

[`src/libraries/z42.crypto/src/Blake2b.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L10)


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

成员：[`Hash`](#Std.Crypto.Blake2b.Hash(byte[])) · [`HashString`](#Std.Crypto.Blake2b.HashString(string)) · [`HashHex`](#Std.Crypto.Blake2b.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Blake2b.HashStringHex(string)) · [`Hash256`](#Std.Crypto.Blake2b.Hash256(byte[])) · [`Hash256String`](#Std.Crypto.Blake2b.Hash256String(string)) · [`Hash256Hex`](#Std.Crypto.Blake2b.Hash256Hex(byte[])) · [`Hash256StringHex`](#Std.Crypto.Blake2b.Hash256StringHex(string)) · [`HashLen`](#Std.Crypto.Blake2b.HashLen(byte[],byte[],int))

<a id="Std.Crypto.Blake2b.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L14)

计算默认 64 字节（512 位）BLAKE2b 摘要。

<a id="Std.Crypto.Blake2b.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L18)

对 UTF-8 字符串计算 BLAKE2b 摘要。

<a id="Std.Crypto.Blake2b.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L20)

计算 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L22)

对 UTF-8 字符串计算 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.Hash256(byte[])"></a>
### static byte[] Hash256(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L25)

计算 32 字节（256 位）BLAKE2b 摘要（常用于 IPFS / 区块链等内容寻址）。

<a id="Std.Crypto.Blake2b.Hash256String(string)"></a>
### static byte[] Hash256String(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L29)

对 UTF-8 字符串计算 256 位 BLAKE2b 摘要。

<a id="Std.Crypto.Blake2b.Hash256Hex(byte[])"></a>
### static string Hash256Hex(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L31)

计算 256 位 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.Hash256StringHex(string)"></a>
### static string Hash256StringHex(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L33)

对 UTF-8 字符串计算 256 位 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.HashLen(byte[],byte[],int)"></a>
### static byte[] HashLen(byte[] data, byte[] key, int outLen)

[`src/libraries/z42.crypto/src/Blake2b.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L36)

BLAKE2b 变长 + keyed（MAC）模式：`key.Length` ∈ [0, 64]、`outLen` ∈ [1, 64]，无 key 传 `new byte[0]`。

## Blake2s

[`src/libraries/z42.crypto/src/Blake2s.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L10)


```z42
public static class Blake2s {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
    static byte[] HashLen(byte[] data, byte[] key, int outLen)
}
```

成员：[`Hash`](#Std.Crypto.Blake2s.Hash(byte[])) · [`HashString`](#Std.Crypto.Blake2s.HashString(string)) · [`HashHex`](#Std.Crypto.Blake2s.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Blake2s.HashStringHex(string)) · [`HashLen`](#Std.Crypto.Blake2s.HashLen(byte[],byte[],int))

<a id="Std.Crypto.Blake2s.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Blake2s.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L14)

计算默认 32 字节（256 位）BLAKE2s 摘要。

<a id="Std.Crypto.Blake2s.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Blake2s.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L18)

对 UTF-8 字符串计算 BLAKE2s 摘要。

<a id="Std.Crypto.Blake2s.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Blake2s.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L20)

计算 BLAKE2s 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2s.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Blake2s.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L22)

对 UTF-8 字符串计算 BLAKE2s 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2s.HashLen(byte[],byte[],int)"></a>
### static byte[] HashLen(byte[] data, byte[] key, int outLen)

[`src/libraries/z42.crypto/src/Blake2s.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L25)

BLAKE2s 变长 + keyed（MAC）模式：`key.Length` ∈ [0, 32]、`outLen` ∈ [1, 32]，无 key 传 `new byte[0]`。

## Blake3

[`src/libraries/z42.crypto/src/Blake3.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L12)


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

成员：[`Hash`](#Std.Crypto.Blake3.Hash(byte[])) · [`Hash256`](#Std.Crypto.Blake3.Hash256(byte[])) · [`HashString`](#Std.Crypto.Blake3.HashString(string)) · [`HashHex`](#Std.Crypto.Blake3.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Blake3.HashStringHex(string)) · [`HashLen`](#Std.Crypto.Blake3.HashLen(byte[],int))

<a id="Std.Crypto.Blake3.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Blake3.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L27)

计算 BLAKE3 摘要（默认 32 字节）。

<a id="Std.Crypto.Blake3.Hash256(byte[])"></a>
### static byte[] Hash256(byte[] data)

[`src/libraries/z42.crypto/src/Blake3.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L32)

`Hash` 的同义方法，返回 32 字节 BLAKE3 摘要。

<a id="Std.Crypto.Blake3.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Blake3.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L37)

对 UTF-8 字符串计算 BLAKE3 摘要。

<a id="Std.Crypto.Blake3.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Blake3.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L39)

计算 BLAKE3 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake3.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Blake3.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L41)

对 UTF-8 字符串计算 BLAKE3 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake3.HashLen(byte[],int)"></a>
### static byte[] HashLen(byte[] data, int outLen)

[`src/libraries/z42.crypto/src/Blake3.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L44)

BLAKE3 可扩展输出（XOF）：`outLen` ≥ 0，前 N 字节与更长输出的前 N 字节一致。

## ChaCha20

[`src/libraries/z42.crypto/src/ChaCha20.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L11)


```z42
public static class ChaCha20 {
    static byte[] Crypt(byte[] key, byte[] nonce, int counter, byte[] data)
    static byte[] Encrypt(byte[] key, byte[] nonce, byte[] data)
    static byte[] Decrypt(byte[] key, byte[] nonce, byte[] data)
    static byte[] Block(byte[] key, byte[] nonce, int counter)
}
```

成员：[`Crypt`](#Std.Crypto.ChaCha20.Crypt(byte[],byte[],int,byte[])) · [`Encrypt`](#Std.Crypto.ChaCha20.Encrypt(byte[],byte[],byte[])) · [`Decrypt`](#Std.Crypto.ChaCha20.Decrypt(byte[],byte[],byte[])) · [`Block`](#Std.Crypto.ChaCha20.Block(byte[],byte[],int))

<a id="Std.Crypto.ChaCha20.Crypt(byte[],byte[],int,byte[])"></a>
### static byte[] Crypt(byte[] key, byte[] nonce, int counter, byte[] data)

[`src/libraries/z42.crypto/src/ChaCha20.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L16)

加密 / 解密（对称，与密钥流 XOR）。独立使用时 `counter` 从 1 起（RFC 8439 §2.4）； 仅派生 Poly1305 一次性 key 时传 0。

<a id="Std.Crypto.ChaCha20.Encrypt(byte[],byte[],byte[])"></a>
### static byte[] Encrypt(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/ChaCha20.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L42)

便捷形式，默认 counter=1（RFC 8439 独立使用）。

<a id="Std.Crypto.ChaCha20.Decrypt(byte[],byte[],byte[])"></a>
### static byte[] Decrypt(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/ChaCha20.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L47)

解密——与加密同义。

<a id="Std.Crypto.ChaCha20.Block(byte[],byte[],int)"></a>
### static byte[] Block(byte[] key, byte[] nonce, int counter)

[`src/libraries/z42.crypto/src/ChaCha20.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L52)

产生 (key, nonce, counter) 的一个 64 字节密钥流块，供 Poly1305 key 派生（counter=0 路径）使用。

## ChaCha20Poly1305

[`src/libraries/z42.crypto/src/ChaCha20Poly1305.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20Poly1305.z42#L10)


```z42
public static class ChaCha20Poly1305 {
    static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)
    static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)
}
```

成员：[`Encrypt`](#Std.Crypto.ChaCha20Poly1305.Encrypt(byte[],byte[],byte[],byte[])) · [`Decrypt`](#Std.Crypto.ChaCha20Poly1305.Decrypt(byte[],byte[],byte[],byte[]))

<a id="Std.Crypto.ChaCha20Poly1305.Encrypt(byte[],byte[],byte[],byte[])"></a>
### static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)

[`src/libraries/z42.crypto/src/ChaCha20Poly1305.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20Poly1305.z42#L12)

加密 + 认证。输出 = ciphertext（与 `plaintext` 等长）|| tag（16 字节）。

<a id="Std.Crypto.ChaCha20Poly1305.Decrypt(byte[],byte[],byte[],byte[])"></a>
### static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)

[`src/libraries/z42.crypto/src/ChaCha20Poly1305.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20Poly1305.z42#L39)

解密 + 校验。输入 = ciphertext || 16 字节 tag；tag 不匹配抛 `ArgumentException` （勿吞——表示密文或 AAD 被篡改）。

## ConstantTime

[`src/libraries/z42.crypto/src/ConstantTime.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ConstantTime.z42#L7)


```z42
public static class ConstantTime {
    static bool Equals(byte[] a, byte[] b)
}
```

成员：[`Equals`](#Std.Crypto.ConstantTime.Equals(byte[],byte[]))

<a id="Std.Crypto.ConstantTime.Equals(byte[],byte[])"></a>
### static bool Equals(byte[] a, byte[] b)

[`src/libraries/z42.crypto/src/ConstantTime.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ConstantTime.z42#L10)

无提前退出的字节数组比较，运行时间只依赖长度（非秘密），从不依赖首个差异位置。 长度不同立即返回 false（只泄漏长度）；长度相同时始终逐字节检查，累计差异决定结果。

## EcdsaP256

[`src/libraries/z42.crypto/src/EcdsaP256.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaP256.z42#L11)


```z42
public static class EcdsaP256 {
    static byte[] GeneratePublicKey(byte[] privateScalar)
    static byte[] Sign(byte[] privateScalar, byte[] message)
    static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

成员：[`GeneratePublicKey`](#Std.Crypto.EcdsaP256.GeneratePublicKey(byte[])) · [`Sign`](#Std.Crypto.EcdsaP256.Sign(byte[],byte[])) · [`Verify`](#Std.Crypto.EcdsaP256.Verify(byte[],byte[],byte[]))

<a id="Std.Crypto.EcdsaP256.GeneratePublicKey(byte[])"></a>
### static byte[] GeneratePublicKey(byte[] privateScalar)

[`src/libraries/z42.crypto/src/EcdsaP256.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaP256.z42#L15)

从 32 字节私钥标量派生 64 字节未压缩公钥（X ‖ Y）。

<a id="Std.Crypto.EcdsaP256.Sign(byte[],byte[])"></a>
### static byte[] Sign(byte[] privateScalar, byte[] message)

[`src/libraries/z42.crypto/src/EcdsaP256.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaP256.z42#L34)

用 32 字节私钥标量对 `message` 签名（RFC 6979 确定性），返回 64 字节（r ‖ s，各 32 字节大端）。

<a id="Std.Crypto.EcdsaP256.Verify(byte[],byte[],byte[])"></a>
### static bool Verify(byte[] publicKey, byte[] message, byte[] signature)

[`src/libraries/z42.crypto/src/EcdsaP256.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaP256.z42#L73)

校验 64 字节签名；无效签名 / 非曲线上点 / 越界 r、s 返回 `false`，仅公钥非 64 字节抛 `ArgumentException`。

## EcdsaSecp256k1

[`src/libraries/z42.crypto/src/EcdsaSecp256k1.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaSecp256k1.z42#L10)


```z42
public static class EcdsaSecp256k1 {
    static byte[] GeneratePublicKey(byte[] privateScalar)
    static byte[] Sign(byte[] privateScalar, byte[] message)
    static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

成员：[`GeneratePublicKey`](#Std.Crypto.EcdsaSecp256k1.GeneratePublicKey(byte[])) · [`Sign`](#Std.Crypto.EcdsaSecp256k1.Sign(byte[],byte[])) · [`Verify`](#Std.Crypto.EcdsaSecp256k1.Verify(byte[],byte[],byte[]))

<a id="Std.Crypto.EcdsaSecp256k1.GeneratePublicKey(byte[])"></a>
### static byte[] GeneratePublicKey(byte[] privateScalar)

[`src/libraries/z42.crypto/src/EcdsaSecp256k1.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaSecp256k1.z42#L14)

从 32 字节私钥标量派生 64 字节未压缩公钥（X ‖ Y）。

<a id="Std.Crypto.EcdsaSecp256k1.Sign(byte[],byte[])"></a>
### static byte[] Sign(byte[] privateScalar, byte[] message)

[`src/libraries/z42.crypto/src/EcdsaSecp256k1.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaSecp256k1.z42#L33)

用 32 字节私钥标量对 `message` 签名（RFC 6979 确定性），返回 64 字节（r ‖ s，各 32 字节大端）。

<a id="Std.Crypto.EcdsaSecp256k1.Verify(byte[],byte[],byte[])"></a>
### static bool Verify(byte[] publicKey, byte[] message, byte[] signature)

[`src/libraries/z42.crypto/src/EcdsaSecp256k1.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaSecp256k1.z42#L72)

校验 64 字节签名；无效签名 / 非曲线上点 / 越界 r、s 返回 `false`，仅公钥非 64 字节抛 `ArgumentException`。

## Ed25519

[`src/libraries/z42.crypto/src/Ed25519.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Ed25519.z42#L10)


```z42
public static class Ed25519 {
    static byte[] GeneratePublicKey(byte[] secretKey)
    static byte[] Sign(byte[] secretKey, byte[] message)
    static bool Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

成员：[`GeneratePublicKey`](#Std.Crypto.Ed25519.GeneratePublicKey(byte[])) · [`Sign`](#Std.Crypto.Ed25519.Sign(byte[],byte[])) · [`Verify`](#Std.Crypto.Ed25519.Verify(byte[],byte[],byte[]))

<a id="Std.Crypto.Ed25519.GeneratePublicKey(byte[])"></a>
### static byte[] GeneratePublicKey(byte[] secretKey)

[`src/libraries/z42.crypto/src/Ed25519.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Ed25519.z42#L14)

从 32 字节私钥派生 32 字节 Ed25519 公钥。

<a id="Std.Crypto.Ed25519.Sign(byte[],byte[])"></a>
### static byte[] Sign(byte[] secretKey, byte[] message)

[`src/libraries/z42.crypto/src/Ed25519.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Ed25519.z42#L26)

用 32 字节私钥对 `message` 签名，输出 64 字节 `R || S`（RFC 8032 §5.1.6）。

<a id="Std.Crypto.Ed25519.Verify(byte[],byte[],byte[])"></a>
### static bool Verify(byte[] publicKey, byte[] message, byte[] signature)

[`src/libraries/z42.crypto/src/Ed25519.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Ed25519.z42#L64)

用 32 字节公钥校验 64 字节签名；有效返回 `true`。签名内容错误返回 `false`（仅长度错误抛 `ArgumentException`）。

## HkdfSha256

[`src/libraries/z42.crypto/src/Hkdf.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L8)


```z42
public static class HkdfSha256 {
    static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)
    static byte[] Extract(byte[] salt, byte[] ikm)
    static byte[] Expand(byte[] prk, byte[] info, int length)
    static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
}
```

成员：[`Derive`](#Std.Crypto.HkdfSha256.Derive(byte[],byte[],byte[],int)) · [`Extract`](#Std.Crypto.HkdfSha256.Extract(byte[],byte[])) · [`Expand`](#Std.Crypto.HkdfSha256.Expand(byte[],byte[],int)) · [`DeriveHex`](#Std.Crypto.HkdfSha256.DeriveHex(byte[],byte[],byte[],int))

<a id="Std.Crypto.HkdfSha256.Derive(byte[],byte[],byte[],int)"></a>
### static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L14)

一次性 Extract + Expand。两阶段单独暴露（`Extract` / `Expand`）， 供需要在其间穿插其他状态（如 TLS 1.3 密钥表）的调用方使用。

<a id="Std.Crypto.HkdfSha256.Extract(byte[],byte[])"></a>
### static byte[] Extract(byte[] salt, byte[] ikm)

[`src/libraries/z42.crypto/src/Hkdf.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L24)

HKDF-Extract（RFC 5869 §2.2）：PRK = HMAC-Hash(salt, IKM)。空 salt → 32 个零字节，输出 32 字节。

<a id="Std.Crypto.HkdfSha256.Expand(byte[],byte[],int)"></a>
### static byte[] Expand(byte[] prk, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L35)

HKDF-Expand（RFC 5869 §2.3）：把 PRK 扩展成 `length` 字节输出密钥材料，`info` 作上下文绑定。

<a id="Std.Crypto.HkdfSha256.DeriveHex(byte[],byte[],byte[],int)"></a>
### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L76)

十六进制便捷变体，用于文档 / 调试。

## HkdfSha512

[`src/libraries/z42.crypto/src/Hkdf.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L82)


```z42
public static class HkdfSha512 {
    static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)
    static byte[] Extract(byte[] salt, byte[] ikm)
    static byte[] Expand(byte[] prk, byte[] info, int length)
    static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
}
```

成员：[`Derive`](#Std.Crypto.HkdfSha512.Derive(byte[],byte[],byte[],int)) · [`Extract`](#Std.Crypto.HkdfSha512.Extract(byte[],byte[])) · [`Expand`](#Std.Crypto.HkdfSha512.Expand(byte[],byte[],int)) · [`DeriveHex`](#Std.Crypto.HkdfSha512.DeriveHex(byte[],byte[],byte[],int))

<a id="Std.Crypto.HkdfSha512.Derive(byte[],byte[],byte[],int)"></a>
### static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L87)

一次性 Extract + Expand（HKDF-SHA-512，HashLen 64 字节）。

<a id="Std.Crypto.HkdfSha512.Extract(byte[],byte[])"></a>
### static byte[] Extract(byte[] salt, byte[] ikm)

[`src/libraries/z42.crypto/src/Hkdf.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L97)

HKDF-Extract：PRK = HMAC-SHA512(salt, IKM)，输出 64 字节。

<a id="Std.Crypto.HkdfSha512.Expand(byte[],byte[],int)"></a>
### static byte[] Expand(byte[] prk, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L108)

HKDF-Expand：把 PRK 扩展成 `length` 字节输出密钥材料。

<a id="Std.Crypto.HkdfSha512.DeriveHex(byte[],byte[],byte[],int)"></a>
### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:142`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L142)

十六进制便捷变体。

## HkdfSha1

[`src/libraries/z42.crypto/src/Hkdf.z42:150`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L150)


```z42
public static class HkdfSha1 {
    static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)
    static byte[] Extract(byte[] salt, byte[] ikm)
    static byte[] Expand(byte[] prk, byte[] info, int length)
    static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
}
```

成员：[`Derive`](#Std.Crypto.HkdfSha1.Derive(byte[],byte[],byte[],int)) · [`Extract`](#Std.Crypto.HkdfSha1.Extract(byte[],byte[])) · [`Expand`](#Std.Crypto.HkdfSha1.Expand(byte[],byte[],int)) · [`DeriveHex`](#Std.Crypto.HkdfSha1.DeriveHex(byte[],byte[],byte[],int))

<a id="Std.Crypto.HkdfSha1.Derive(byte[],byte[],byte[],int)"></a>
### static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L155)

一次性 Extract + Expand（HKDF-SHA-1，HashLen 20 字节）。

<a id="Std.Crypto.HkdfSha1.Extract(byte[],byte[])"></a>
### static byte[] Extract(byte[] salt, byte[] ikm)

[`src/libraries/z42.crypto/src/Hkdf.z42:165`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L165)

HKDF-Extract：PRK = HMAC-SHA1(salt, IKM)，输出 20 字节。

<a id="Std.Crypto.HkdfSha1.Expand(byte[],byte[],int)"></a>
### static byte[] Expand(byte[] prk, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:176`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L176)

HKDF-Expand：把 PRK 扩展成 `length` 字节输出密钥材料。

<a id="Std.Crypto.HkdfSha1.DeriveHex(byte[],byte[],byte[],int)"></a>
### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:210`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L210)

十六进制便捷变体。

## HmacSha256

[`src/libraries/z42.crypto/src/Hmac.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L7)


```z42
public static class HmacSha256 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

成员：[`Compute`](#Std.Crypto.HmacSha256.Compute(byte[],byte[])) · [`ComputeString`](#Std.Crypto.HmacSha256.ComputeString(string,string)) · [`ComputeHex`](#Std.Crypto.HmacSha256.ComputeHex(byte[],byte[])) · [`ComputeStringHex`](#Std.Crypto.HmacSha256.ComputeStringHex(string,string))

<a id="Std.Crypto.HmacSha256.Compute(byte[],byte[])"></a>
### static byte[] Compute(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L11)

计算 `message` 在 `key` 下的 HMAC-SHA256，返回 32 字节摘要。

<a id="Std.Crypto.HmacSha256.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L44)

对 UTF-8 编码的字符串 key / message 计算 HMAC-SHA256（返回 32 字节）。

<a id="Std.Crypto.HmacSha256.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L51)

计算 HMAC-SHA256 并输出小写十六进制（64 字符）。

<a id="Std.Crypto.HmacSha256.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L56)

对 UTF-8 字符串 key / message 计算 HMAC-SHA256 并输出小写十六进制。

## HmacSha1

[`src/libraries/z42.crypto/src/Hmac.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L90)


```z42
public static class HmacSha1 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

成员：[`Compute`](#Std.Crypto.HmacSha1.Compute(byte[],byte[])) · [`ComputeString`](#Std.Crypto.HmacSha1.ComputeString(string,string)) · [`ComputeHex`](#Std.Crypto.HmacSha1.ComputeHex(byte[],byte[])) · [`ComputeStringHex`](#Std.Crypto.HmacSha1.ComputeStringHex(string,string))

<a id="Std.Crypto.HmacSha1.Compute(byte[],byte[])"></a>
### static byte[] Compute(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L92)

计算 `message` 在 `key` 下的 HMAC-SHA1，返回 20 字节摘要。

<a id="Std.Crypto.HmacSha1.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:129`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L129)

对 UTF-8 字符串 key / message 计算 HMAC-SHA1。

<a id="Std.Crypto.HmacSha1.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L136)

计算 HMAC-SHA1 并输出小写十六进制。

<a id="Std.Crypto.HmacSha1.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L141)

对 UTF-8 字符串 key / message 计算 HMAC-SHA1 并输出小写十六进制。

## HmacSha512

[`src/libraries/z42.crypto/src/Hmac.z42:169`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L169)


```z42
public static class HmacSha512 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

成员：[`Compute`](#Std.Crypto.HmacSha512.Compute(byte[],byte[])) · [`ComputeString`](#Std.Crypto.HmacSha512.ComputeString(string,string)) · [`ComputeHex`](#Std.Crypto.HmacSha512.ComputeHex(byte[],byte[])) · [`ComputeStringHex`](#Std.Crypto.HmacSha512.ComputeStringHex(string,string))

<a id="Std.Crypto.HmacSha512.Compute(byte[],byte[])"></a>
### static byte[] Compute(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:171`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L171)

计算 `message` 在 `key` 下的 HMAC-SHA512，返回 64 字节摘要。

<a id="Std.Crypto.HmacSha512.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:208`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L208)

对 UTF-8 字符串 key / message 计算 HMAC-SHA512。

<a id="Std.Crypto.HmacSha512.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:215`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L215)

计算 HMAC-SHA512 并输出小写十六进制。

<a id="Std.Crypto.HmacSha512.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:220`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L220)

对 UTF-8 字符串 key / message 计算 HMAC-SHA512 并输出小写十六进制。

## HmacSha384

[`src/libraries/z42.crypto/src/Hmac.z42:249`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L249)


```z42
public static class HmacSha384 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

成员：[`Compute`](#Std.Crypto.HmacSha384.Compute(byte[],byte[])) · [`ComputeString`](#Std.Crypto.HmacSha384.ComputeString(string,string)) · [`ComputeHex`](#Std.Crypto.HmacSha384.ComputeHex(byte[],byte[])) · [`ComputeStringHex`](#Std.Crypto.HmacSha384.ComputeStringHex(string,string))

<a id="Std.Crypto.HmacSha384.Compute(byte[],byte[])"></a>
### static byte[] Compute(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:251`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L251)

计算 `message` 在 `key` 下的 HMAC-SHA384，返回 48 字节摘要。

<a id="Std.Crypto.HmacSha384.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:287`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L287)

对 UTF-8 字符串 key / message 计算 HMAC-SHA384。

<a id="Std.Crypto.HmacSha384.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:294`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L294)

计算 HMAC-SHA384 并输出小写十六进制。

<a id="Std.Crypto.HmacSha384.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:299`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L299)

对 UTF-8 字符串 key / message 计算 HMAC-SHA384 并输出小写十六进制。

## HmacMd5

[`src/libraries/z42.crypto/src/Hmac.z42:329`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L329)


```z42
public static class HmacMd5 {
    static byte[] Compute(byte[] key, byte[] message)
    static byte[] ComputeString(string key, string message)
    static string ComputeHex(byte[] key, byte[] message)
    static string ComputeStringHex(string key, string message)
}
```

成员：[`Compute`](#Std.Crypto.HmacMd5.Compute(byte[],byte[])) · [`ComputeString`](#Std.Crypto.HmacMd5.ComputeString(string,string)) · [`ComputeHex`](#Std.Crypto.HmacMd5.ComputeHex(byte[],byte[])) · [`ComputeStringHex`](#Std.Crypto.HmacMd5.ComputeStringHex(string,string))

<a id="Std.Crypto.HmacMd5.Compute(byte[],byte[])"></a>
### static byte[] Compute(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:331`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L331)

计算 `message` 在 `key` 下的 HMAC-MD5，返回 16 字节摘要。

<a id="Std.Crypto.HmacMd5.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:368`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L368)

对 UTF-8 字符串 key / message 计算 HMAC-MD5。

<a id="Std.Crypto.HmacMd5.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:375`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L375)

计算 HMAC-MD5 并输出小写十六进制。

<a id="Std.Crypto.HmacMd5.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:380`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L380)

对 UTF-8 字符串 key / message 计算 HMAC-MD5 并输出小写十六进制。

## Md5

[`src/libraries/z42.crypto/src/Md5.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L9)


```z42
public static class Md5 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

成员：[`Hash`](#Std.Crypto.Md5.Hash(byte[])) · [`HashString`](#Std.Crypto.Md5.HashString(string)) · [`HashHex`](#Std.Crypto.Md5.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Md5.HashStringHex(string))

<a id="Std.Crypto.Md5.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Md5.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L13)

计算 MD5 原始摘要（16 字节）。

<a id="Std.Crypto.Md5.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Md5.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L37)

对 UTF-8 编码后的字符串计算 MD5 摘要。

<a id="Std.Crypto.Md5.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Md5.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L42)

计算 MD5 摘要并输出小写十六进制。

<a id="Std.Crypto.Md5.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Md5.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L47)

对 UTF-8 字符串计算 MD5 摘要并输出小写十六进制。

## Pbkdf2

[`src/libraries/z42.crypto/src/Pbkdf2.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L9)


```z42
public static class Pbkdf2 {
    static byte[] DeriveKey(byte[] password, byte[] salt, int iterations, int dkLen)
    static byte[] DeriveKeyString(string password, string salt, int iterations, int dkLen)
    static string DeriveKeyHex(byte[] password, byte[] salt, int iterations, int dkLen)
    static string DeriveKeyStringHex(string password, string salt, int iterations, int dkLen)
}
```

成员：[`DeriveKey`](#Std.Crypto.Pbkdf2.DeriveKey(byte[],byte[],int,int)) · [`DeriveKeyString`](#Std.Crypto.Pbkdf2.DeriveKeyString(string,string,int,int)) · [`DeriveKeyHex`](#Std.Crypto.Pbkdf2.DeriveKeyHex(byte[],byte[],int,int)) · [`DeriveKeyStringHex`](#Std.Crypto.Pbkdf2.DeriveKeyStringHex(string,string,int,int))

<a id="Std.Crypto.Pbkdf2.DeriveKey(byte[],byte[],int,int)"></a>
### static byte[] DeriveKey(byte[] password, byte[] salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L13)

PBKDF2-HMAC-SHA256 派生，返回 `dkLen` 字节。`iterations < 1` 或 `dkLen < 1` 抛 `ArgumentException`。

<a id="Std.Crypto.Pbkdf2.DeriveKeyString(string,string,int,int)"></a>
### static byte[] DeriveKeyString(string password, string salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L49)

字符串 password / salt 便捷形式，按 UTF-8 编码。

<a id="Std.Crypto.Pbkdf2.DeriveKeyHex(byte[],byte[],int,int)"></a>
### static string DeriveKeyHex(byte[] password, byte[] salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L55)

派生并输出小写十六进制（长度 2 × dkLen）。

<a id="Std.Crypto.Pbkdf2.DeriveKeyStringHex(string,string,int,int)"></a>
### static string DeriveKeyStringHex(string password, string salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L61)

对字符串 password / salt 派生并输出小写十六进制。

## Poly1305

[`src/libraries/z42.crypto/src/Poly1305.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Poly1305.z42#L13)


```z42
public static class Poly1305 {
    static byte[] Mac(byte[] key, byte[] message)
    static string MacHex(byte[] key, byte[] message)
}
```

成员：[`Mac`](#Std.Crypto.Poly1305.Mac(byte[],byte[])) · [`MacHex`](#Std.Crypto.Poly1305.MacHex(byte[],byte[]))

<a id="Std.Crypto.Poly1305.Mac(byte[],byte[])"></a>
### static byte[] Mac(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Poly1305.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Poly1305.z42#L15)

在 32 字节一次性 key 下计算 `message` 的 16 字节认证 tag，key 长度错误抛 `ArgumentException`。

<a id="Std.Crypto.Poly1305.MacHex(byte[],byte[])"></a>
### static string MacHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Poly1305.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Poly1305.z42#L70)

计算 tag 并输出十六进制。

## RsaPublicKey

[`src/libraries/z42.crypto/src/Rsa.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L11)

RSA 公钥（模数 + 公钥指数）。

```z42
public class RsaPublicKey {
    BigInt n
    BigInt e
    RsaPublicKey(BigInt n, BigInt e)
}
```

成员：[`n`](#Std.Crypto.RsaPublicKey.n) · [`e`](#Std.Crypto.RsaPublicKey.e) · [`RsaPublicKey`](#Std.Crypto.RsaPublicKey.ctor(BigInt,BigInt))

<a id="Std.Crypto.RsaPublicKey.n"></a>
### BigInt n

[`src/libraries/z42.crypto/src/Rsa.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L13)

模数。

<a id="Std.Crypto.RsaPublicKey.e"></a>
### BigInt e

[`src/libraries/z42.crypto/src/Rsa.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L15)

公钥指数。

<a id="Std.Crypto.RsaPublicKey.ctor(BigInt,BigInt)"></a>
### RsaPublicKey(BigInt n, BigInt e)

[`src/libraries/z42.crypto/src/Rsa.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L17)

用模数 n 与公钥指数 e 构造。

## RsaPrivateKey

[`src/libraries/z42.crypto/src/Rsa.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L24)

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

成员：[`n`](#Std.Crypto.RsaPrivateKey.n) · [`e`](#Std.Crypto.RsaPrivateKey.e) · [`d`](#Std.Crypto.RsaPrivateKey.d) · [`RsaPrivateKey`](#Std.Crypto.RsaPrivateKey.ctor(BigInt,BigInt,BigInt)) · [`GetPublicKey`](#Std.Crypto.RsaPrivateKey.GetPublicKey())

<a id="Std.Crypto.RsaPrivateKey.n"></a>
### BigInt n

[`src/libraries/z42.crypto/src/Rsa.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L26)

模数。

<a id="Std.Crypto.RsaPrivateKey.e"></a>
### BigInt e

[`src/libraries/z42.crypto/src/Rsa.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L28)

公钥指数。

<a id="Std.Crypto.RsaPrivateKey.d"></a>
### BigInt d

[`src/libraries/z42.crypto/src/Rsa.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L30)

私钥指数。

<a id="Std.Crypto.RsaPrivateKey.ctor(BigInt,BigInt,BigInt)"></a>
### RsaPrivateKey(BigInt n, BigInt e, BigInt d)

[`src/libraries/z42.crypto/src/Rsa.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L32)

用模数 n、公钥指数 e、私钥指数 d 构造。

<a id="Std.Crypto.RsaPrivateKey.GetPublicKey()"></a>
### RsaPublicKey GetPublicKey()

[`src/libraries/z42.crypto/src/Rsa.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L39)

派生匹配的公钥。

## Rsa

[`src/libraries/z42.crypto/src/Rsa.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L44)


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

成员：[`SignPkcs1v15Sha256`](#Std.Crypto.Rsa.SignPkcs1v15Sha256(RsaPrivateKey,byte[])) · [`VerifyPkcs1v15Sha256`](#Std.Crypto.Rsa.VerifyPkcs1v15Sha256(RsaPublicKey,byte[],byte[])) · [`SignPssSha256`](#Std.Crypto.Rsa.SignPssSha256(RsaPrivateKey,byte[],byte[])) · [`VerifyPssSha256`](#Std.Crypto.Rsa.VerifyPssSha256(RsaPublicKey,byte[],byte[],int)) · [`EncryptOaepSha256`](#Std.Crypto.Rsa.EncryptOaepSha256(RsaPublicKey,byte[],byte[],byte[])) · [`DecryptOaepSha256`](#Std.Crypto.Rsa.DecryptOaepSha256(RsaPrivateKey,byte[],byte[])) · [`RawPublicOp`](#Std.Crypto.Rsa.RawPublicOp(RsaPublicKey,byte[])) · [`RawPrivateOp`](#Std.Crypto.Rsa.RawPrivateOp(RsaPrivateKey,byte[]))

<a id="Std.Crypto.Rsa.SignPkcs1v15Sha256(RsaPrivateKey,byte[])"></a>
### static byte[] SignPkcs1v15Sha256(RsaPrivateKey privateKey, byte[] message)

[`src/libraries/z42.crypto/src/Rsa.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L58)

用 SHA-256 + PKCS#1 v1.5 填充对 `message` 签名，输出恰好 `n.byteLength()` 字节。

<a id="Std.Crypto.Rsa.VerifyPkcs1v15Sha256(RsaPublicKey,byte[],byte[])"></a>
### static bool VerifyPkcs1v15Sha256(RsaPublicKey publicKey, byte[] message, byte[] signature)

[`src/libraries/z42.crypto/src/Rsa.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L68)

校验 SHA-256(message) 的 PKCS#1 v1.5 签名；有效返回 true。签名内容错误返回 false（仅长度错误抛异常）。

<a id="Std.Crypto.Rsa.SignPssSha256(RsaPrivateKey,byte[],byte[])"></a>
### static byte[] SignPssSha256(RsaPrivateKey privateKey, byte[] message, byte[] salt)

[`src/libraries/z42.crypto/src/Rsa.z42:137`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L137)

RSASSA-PSS / SHA-256 / MGF1-SHA-256 签名，`salt` 长度由调用方定（推荐 32 字节），输出 n 的字节长度。

<a id="Std.Crypto.Rsa.VerifyPssSha256(RsaPublicKey,byte[],byte[],int)"></a>
### static bool VerifyPssSha256(RsaPublicKey publicKey, byte[] message, byte[] signature, int saltLen)

[`src/libraries/z42.crypto/src/Rsa.z42:161`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L161)

校验 RSASSA-PSS / SHA-256 签名，`saltLen` 必须与签名时一致（RFC 8017 §9.1.2 步骤 11——验签方知道 salt 长度而非 salt 本身）。

<a id="Std.Crypto.Rsa.EncryptOaepSha256(RsaPublicKey,byte[],byte[],byte[])"></a>
### static byte[] EncryptOaepSha256(RsaPublicKey publicKey, byte[] message, byte[] label, byte[] seed)

[`src/libraries/z42.crypto/src/Rsa.z42:301`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L301)

RSA-OAEP / SHA-256 / MGF1-SHA-256 加密。`label` 可为空（用于上下文绑定），`seed` 必须 32 字节。

<a id="Std.Crypto.Rsa.DecryptOaepSha256(RsaPrivateKey,byte[],byte[])"></a>
### static byte[] DecryptOaepSha256(RsaPrivateKey privateKey, byte[] ciphertext, byte[] label)

[`src/libraries/z42.crypto/src/Rsa.z42:350`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L350)

RSA-OAEP / SHA-256 / MGF1-SHA-256 解密，返回明文。填充非法抛 `ArgumentException` （勿在调用层区分错误——区分 padding 错误与值错误的计时泄漏风险正是 OAEP 要缓解的）。

<a id="Std.Crypto.Rsa.RawPublicOp(RsaPublicKey,byte[])"></a>
### static byte[] RawPublicOp(RsaPublicKey publicKey, byte[] message)

[`src/libraries/z42.crypto/src/Rsa.z42:458`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L458)

裸 `message^e mod n` 模幂（RSAEP，RFC 8017 §5.1.1），无 padding。输入须在 [0, n)，返回 n.byteLength() 字节。

<a id="Std.Crypto.Rsa.RawPrivateOp(RsaPrivateKey,byte[])"></a>
### static byte[] RawPrivateOp(RsaPrivateKey privateKey, byte[] ciphertext)

[`src/libraries/z42.crypto/src/Rsa.z42:469`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L469)

裸 `ciphertext^d mod n`（RSADP，RFC 8017 §5.1.2），无 padding 剥离，输入约束同 RawPublicOp。

## Scrypt

[`src/libraries/z42.crypto/src/Scrypt.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Scrypt.z42#L8)


```z42
public static class Scrypt {
    static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)
}
```

成员：[`Derive`](#Std.Crypto.Scrypt.Derive(byte[],byte[],int,int,int,int))

<a id="Std.Crypto.Scrypt.Derive(byte[],byte[],int,int,int,int)"></a>
### static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)

[`src/libraries/z42.crypto/src/Scrypt.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Scrypt.z42#L11)

用 scrypt(N, r, p) 从 `password` + `salt` 派生 `dkLen` 字节。 约束：N 为 2 的幂且 ≥ 2，r ≥ 1，p ≥ 1，r × p < 2^30，dkLen ≥ 0。

## SecureRandom

[`src/libraries/z42.crypto/src/SecureRandom.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L10)


```z42
public static class SecureRandom {
    static byte[] GetBytes(int n)
    static int NextInt()
    static long NextLong()
    static int NextU32Bounded(int bound)
}
```

成员：[`GetBytes`](#Std.Crypto.SecureRandom.GetBytes(int)) · [`NextInt`](#Std.Crypto.SecureRandom.NextInt()) · [`NextLong`](#Std.Crypto.SecureRandom.NextLong()) · [`NextU32Bounded`](#Std.Crypto.SecureRandom.NextU32Bounded(int))

<a id="Std.Crypto.SecureRandom.GetBytes(int)"></a>
### static byte[] GetBytes(int n)

[`src/libraries/z42.crypto/src/SecureRandom.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L12)

从 OS CSPRNG 填充 n 字节（n ≥ 0）。

<a id="Std.Crypto.SecureRandom.NextInt()"></a>
### static int NextInt()

[`src/libraries/z42.crypto/src/SecureRandom.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L20)

整个 i32 范围均匀分布的随机 int（含负数）。由 4 个 CSPRNG 字节大端构建。

<a id="Std.Crypto.SecureRandom.NextLong()"></a>
### static long NextLong()

[`src/libraries/z42.crypto/src/SecureRandom.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L30)

整个 i64 范围均匀分布的随机 long，由 8 个 CSPRNG 字节大端构建。

<a id="Std.Crypto.SecureRandom.NextU32Bounded(int)"></a>
### static int NextU32Bounded(int bound)

[`src/libraries/z42.crypto/src/SecureRandom.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L42)

[0, bound) 均匀分布的随机 int，用 rejection sampling 避免模偏差；bound ≤ 0 抛 `ArgumentException`。

## Sha1

[`src/libraries/z42.crypto/src/Sha1.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L9)


```z42
public static class Sha1 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

成员：[`Hash`](#Std.Crypto.Sha1.Hash(byte[])) · [`HashString`](#Std.Crypto.Sha1.HashString(string)) · [`HashHex`](#Std.Crypto.Sha1.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Sha1.HashStringHex(string))

<a id="Std.Crypto.Sha1.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Sha1.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L13)

计算 SHA-1 原始摘要（20 字节）。

<a id="Std.Crypto.Sha1.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha1.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L36)

对 UTF-8 编码后的字符串计算 SHA-1 摘要。

<a id="Std.Crypto.Sha1.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha1.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L41)

计算 SHA-1 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha1.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha1.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L46)

对 UTF-8 字符串计算 SHA-1 摘要并输出小写十六进制。

## Sha256

[`src/libraries/z42.crypto/src/Sha256.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L35)


```z42
public static class Sha256 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

成员：[`Hash`](#Std.Crypto.Sha256.Hash(byte[])) · [`HashString`](#Std.Crypto.Sha256.HashString(string)) · [`HashHex`](#Std.Crypto.Sha256.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Sha256.HashStringHex(string))

<a id="Std.Crypto.Sha256.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Sha256.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L39)

计算 SHA-256 原始摘要（32 字节）。

<a id="Std.Crypto.Sha256.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha256.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L62)

对 UTF-8 编码后的字符串计算 SHA-256 摘要。

<a id="Std.Crypto.Sha256.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha256.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L67)

计算 SHA-256 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha256.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha256.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L72)

对 UTF-8 字符串计算 SHA-256 摘要并输出小写十六进制。

## Sha3

[`src/libraries/z42.crypto/src/Sha3.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L14)


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

成员：[`Hash224`](#Std.Crypto.Sha3.Hash224(byte[])) · [`Hash256`](#Std.Crypto.Sha3.Hash256(byte[])) · [`Hash384`](#Std.Crypto.Sha3.Hash384(byte[])) · [`Hash512`](#Std.Crypto.Sha3.Hash512(byte[])) · [`Hash224String`](#Std.Crypto.Sha3.Hash224String(string)) · [`Hash256String`](#Std.Crypto.Sha3.Hash256String(string)) · [`Hash384String`](#Std.Crypto.Sha3.Hash384String(string)) · [`Hash512String`](#Std.Crypto.Sha3.Hash512String(string)) · [`Hash224Hex`](#Std.Crypto.Sha3.Hash224Hex(byte[])) · [`Hash256Hex`](#Std.Crypto.Sha3.Hash256Hex(byte[])) · [`Hash384Hex`](#Std.Crypto.Sha3.Hash384Hex(byte[])) · [`Hash512Hex`](#Std.Crypto.Sha3.Hash512Hex(byte[])) · [`Hash224StringHex`](#Std.Crypto.Sha3.Hash224StringHex(string)) · [`Hash256StringHex`](#Std.Crypto.Sha3.Hash256StringHex(string)) · [`Hash384StringHex`](#Std.Crypto.Sha3.Hash384StringHex(string)) · [`Hash512StringHex`](#Std.Crypto.Sha3.Hash512StringHex(string)) · [`KeccakLegacy256`](#Std.Crypto.Sha3.KeccakLegacy256(byte[])) · [`KeccakLegacy512`](#Std.Crypto.Sha3.KeccakLegacy512(byte[])) · [`KeccakLegacy256String`](#Std.Crypto.Sha3.KeccakLegacy256String(string)) · [`KeccakLegacy512String`](#Std.Crypto.Sha3.KeccakLegacy512String(string)) · [`KeccakLegacy256Hex`](#Std.Crypto.Sha3.KeccakLegacy256Hex(byte[])) · [`KeccakLegacy512Hex`](#Std.Crypto.Sha3.KeccakLegacy512Hex(byte[])) · [`KeccakLegacy256StringHex`](#Std.Crypto.Sha3.KeccakLegacy256StringHex(string)) · [`KeccakLegacy512StringHex`](#Std.Crypto.Sha3.KeccakLegacy512StringHex(string)) · [`Shake128`](#Std.Crypto.Sha3.Shake128(byte[],int)) · [`Shake256`](#Std.Crypto.Sha3.Shake256(byte[],int)) · [`Shake128String`](#Std.Crypto.Sha3.Shake128String(string,int)) · [`Shake256String`](#Std.Crypto.Sha3.Shake256String(string,int)) · [`Shake128Hex`](#Std.Crypto.Sha3.Shake128Hex(byte[],int)) · [`Shake256Hex`](#Std.Crypto.Sha3.Shake256Hex(byte[],int)) · [`Shake128StringHex`](#Std.Crypto.Sha3.Shake128StringHex(string,int)) · [`Shake256StringHex`](#Std.Crypto.Sha3.Shake256StringHex(string,int))

<a id="Std.Crypto.Sha3.Hash224(byte[])"></a>
### static byte[] Hash224(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L18)

计算 SHA3-224 原始摘要（28 字节）。

<a id="Std.Crypto.Sha3.Hash256(byte[])"></a>
### static byte[] Hash256(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L20)

计算 SHA3-256 原始摘要（32 字节）。

<a id="Std.Crypto.Sha3.Hash384(byte[])"></a>
### static byte[] Hash384(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L22)

计算 SHA3-384 原始摘要（48 字节）。

<a id="Std.Crypto.Sha3.Hash512(byte[])"></a>
### static byte[] Hash512(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L24)

计算 SHA3-512 原始摘要（64 字节）。

<a id="Std.Crypto.Sha3.Hash224String(string)"></a>
### static byte[] Hash224String(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L27)

对 UTF-8 字符串计算 SHA3-224 摘要。

<a id="Std.Crypto.Sha3.Hash256String(string)"></a>
### static byte[] Hash256String(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L29)

对 UTF-8 字符串计算 SHA3-256 摘要。

<a id="Std.Crypto.Sha3.Hash384String(string)"></a>
### static byte[] Hash384String(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L31)

对 UTF-8 字符串计算 SHA3-384 摘要。

<a id="Std.Crypto.Sha3.Hash512String(string)"></a>
### static byte[] Hash512String(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L33)

对 UTF-8 字符串计算 SHA3-512 摘要。

<a id="Std.Crypto.Sha3.Hash224Hex(byte[])"></a>
### static string Hash224Hex(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L36)

计算 SHA3-224 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash256Hex(byte[])"></a>
### static string Hash256Hex(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L38)

计算 SHA3-256 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash384Hex(byte[])"></a>
### static string Hash384Hex(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L40)

计算 SHA3-384 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash512Hex(byte[])"></a>
### static string Hash512Hex(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L42)

计算 SHA3-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash224StringHex(string)"></a>
### static string Hash224StringHex(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L45)

对 UTF-8 字符串计算 SHA3-224 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash256StringHex(string)"></a>
### static string Hash256StringHex(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L47)

对 UTF-8 字符串计算 SHA3-256 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash384StringHex(string)"></a>
### static string Hash384StringHex(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L49)

对 UTF-8 字符串计算 SHA3-384 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Hash512StringHex(string)"></a>
### static string Hash512StringHex(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L51)

对 UTF-8 字符串计算 SHA3-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.KeccakLegacy256(byte[])"></a>
### static byte[] KeccakLegacy256(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L60)

计算 legacy Keccak-256 原始摘要（32 字节，域分隔字节 0x01）。

<a id="Std.Crypto.Sha3.KeccakLegacy512(byte[])"></a>
### static byte[] KeccakLegacy512(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L62)

计算 legacy Keccak-512 原始摘要（64 字节，域分隔字节 0x01）。

<a id="Std.Crypto.Sha3.KeccakLegacy256String(string)"></a>
### static byte[] KeccakLegacy256String(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L64)

对 UTF-8 字符串计算 legacy Keccak-256 摘要。

<a id="Std.Crypto.Sha3.KeccakLegacy512String(string)"></a>
### static byte[] KeccakLegacy512String(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L66)

对 UTF-8 字符串计算 legacy Keccak-512 摘要。

<a id="Std.Crypto.Sha3.KeccakLegacy256Hex(byte[])"></a>
### static string KeccakLegacy256Hex(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L68)

计算 legacy Keccak-256 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.KeccakLegacy512Hex(byte[])"></a>
### static string KeccakLegacy512Hex(byte[] data)

[`src/libraries/z42.crypto/src/Sha3.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L70)

计算 legacy Keccak-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.KeccakLegacy256StringHex(string)"></a>
### static string KeccakLegacy256StringHex(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L72)

对 UTF-8 字符串计算 legacy Keccak-256 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.KeccakLegacy512StringHex(string)"></a>
### static string KeccakLegacy512StringHex(string s)

[`src/libraries/z42.crypto/src/Sha3.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L74)

对 UTF-8 字符串计算 legacy Keccak-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha3.Shake128(byte[],int)"></a>
### static byte[] Shake128(byte[] data, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L83)

SHAKE128 可扩展输出（`outputLen` ≥ 0）。

<a id="Std.Crypto.Sha3.Shake256(byte[],int)"></a>
### static byte[] Shake256(byte[] data, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L90)

SHAKE256 可扩展输出（`outputLen` ≥ 0）。

<a id="Std.Crypto.Sha3.Shake128String(string,int)"></a>
### static byte[] Shake128String(string s, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L97)

对 UTF-8 字符串做 SHAKE128 可扩展输出。

<a id="Std.Crypto.Sha3.Shake256String(string,int)"></a>
### static byte[] Shake256String(string s, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L99)

对 UTF-8 字符串做 SHAKE256 可扩展输出。

<a id="Std.Crypto.Sha3.Shake128Hex(byte[],int)"></a>
### static string Shake128Hex(byte[] data, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L101)

SHAKE128 可扩展输出并输出小写十六进制。

<a id="Std.Crypto.Sha3.Shake256Hex(byte[],int)"></a>
### static string Shake256Hex(byte[] data, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L103)

SHAKE256 可扩展输出并输出小写十六进制。

<a id="Std.Crypto.Sha3.Shake128StringHex(string,int)"></a>
### static string Shake128StringHex(string s, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L105)

对 UTF-8 字符串做 SHAKE128 可扩展输出并输出小写十六进制。

<a id="Std.Crypto.Sha3.Shake256StringHex(string,int)"></a>
### static string Shake256StringHex(string s, int outputLen)

[`src/libraries/z42.crypto/src/Sha3.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L107)

对 UTF-8 字符串做 SHAKE256 可扩展输出并输出小写十六进制。

## Sha384

[`src/libraries/z42.crypto/src/Sha384.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L7)


```z42
public static class Sha384 {
    static byte[] Hash(byte[] data)
    static byte[] HashString(string s)
    static string HashHex(byte[] data)
    static string HashStringHex(string s)
}
```

成员：[`Hash`](#Std.Crypto.Sha384.Hash(byte[])) · [`HashString`](#Std.Crypto.Sha384.HashString(string)) · [`HashHex`](#Std.Crypto.Sha384.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Sha384.HashStringHex(string))

<a id="Std.Crypto.Sha384.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Sha384.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L9)

计算 SHA-384 原始摘要（48 字节）。

<a id="Std.Crypto.Sha384.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha384.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L29)

对 UTF-8 编码后的字符串计算 SHA-384 摘要。

<a id="Std.Crypto.Sha384.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha384.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L31)

计算 SHA-384 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha384.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha384.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L33)

对 UTF-8 字符串计算 SHA-384 摘要并输出小写十六进制。

## Sha512

[`src/libraries/z42.crypto/src/Sha512.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L9)


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

成员：[`Hash`](#Std.Crypto.Sha512.Hash(byte[])) · [`HashString`](#Std.Crypto.Sha512.HashString(string)) · [`HashHex`](#Std.Crypto.Sha512.HashHex(byte[])) · [`HashStringHex`](#Std.Crypto.Sha512.HashStringHex(string)) · [`_initialHash`](#Std.Crypto.Sha512._initialHash()) · [`_pad`](#Std.Crypto.Sha512._pad(byte[])) · [`_compressBlock`](#Std.Crypto.Sha512._compressBlock(long[],byte[],int)) · [`_lshr64`](#Std.Crypto.Sha512._lshr64(long,int)) · [`_rotr64`](#Std.Crypto.Sha512._rotr64(long,int)) · [`_readBE64`](#Std.Crypto.Sha512._readBE64(byte[],int)) · [`_writeBE64`](#Std.Crypto.Sha512._writeBE64(byte[],int,long))

<a id="Std.Crypto.Sha512.Hash(byte[])"></a>
### static byte[] Hash(byte[] data)

[`src/libraries/z42.crypto/src/Sha512.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L13)

计算 SHA-512 原始摘要（64 字节）。

<a id="Std.Crypto.Sha512.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha512.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L32)

对 UTF-8 编码后的字符串计算 SHA-512 摘要。

<a id="Std.Crypto.Sha512.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha512.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L34)

计算 SHA-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha512.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha512.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L36)

对 UTF-8 字符串计算 SHA-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha512._initialHash()"></a>
### static long[] _initialHash()

[`src/libraries/z42.crypto/src/Sha512.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L43)


<a id="Std.Crypto.Sha512._pad(byte[])"></a>
### static byte[] _pad(byte[] data)

[`src/libraries/z42.crypto/src/Sha512.z42:102`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L102)


<a id="Std.Crypto.Sha512._compressBlock(long[],byte[],int)"></a>
### static void _compressBlock(long[] h, byte[] block, int offset)

[`src/libraries/z42.crypto/src/Sha512.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L121)


<a id="Std.Crypto.Sha512._lshr64(long,int)"></a>
### static long _lshr64(long x, int n)

[`src/libraries/z42.crypto/src/Sha512.z42:184`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L184)


<a id="Std.Crypto.Sha512._rotr64(long,int)"></a>
### static long _rotr64(long x, int n)

[`src/libraries/z42.crypto/src/Sha512.z42:191`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L191)


<a id="Std.Crypto.Sha512._readBE64(byte[],int)"></a>
### static long _readBE64(byte[] data, int offset)

[`src/libraries/z42.crypto/src/Sha512.z42:197`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L197)


<a id="Std.Crypto.Sha512._writeBE64(byte[],int,long)"></a>
### static void _writeBE64(byte[] data, int offset, long v)

[`src/libraries/z42.crypto/src/Sha512.z42:210`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L210)


## X25519

[`src/libraries/z42.crypto/src/X25519.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L11)


```z42
public static class X25519 {
    static int U_BASE
    static byte[] Clamp(byte[] scalar)
    static byte[] ScalarMult(byte[] scalar, byte[] point)
    static byte[] ScalarMultBase(byte[] scalar)
}
```

成员：[`U_BASE`](#Std.Crypto.X25519.U_BASE) · [`Clamp`](#Std.Crypto.X25519.Clamp(byte[])) · [`ScalarMult`](#Std.Crypto.X25519.ScalarMult(byte[],byte[])) · [`ScalarMultBase`](#Std.Crypto.X25519.ScalarMultBase(byte[]))

<a id="Std.Crypto.X25519.U_BASE"></a>
### static int U_BASE

[`src/libraries/z42.crypto/src/X25519.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L15)

Curve25519 基点 u 坐标（RFC 7748 §4.1）。

<a id="Std.Crypto.X25519.Clamp(byte[])"></a>
### static byte[] Clamp(byte[] scalar)

[`src/libraries/z42.crypto/src/X25519.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L21)

按 RFC 7748 §5 clamp 32 字节标量：清 byte0 低 3 位、清 byte31 最高位、置 byte31 次高位， 返回新的 clamp 副本（不改入参）。

<a id="Std.Crypto.X25519.ScalarMult(byte[],byte[])"></a>
### static byte[] ScalarMult(byte[] scalar, byte[] point)

[`src/libraries/z42.crypto/src/X25519.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L37)

X25519 标量乘：`scalar × point`（各 32 字节小端编码）。`scalar` 内部自动 clamp（RFC 7748 §5）， 调用方不必预 clamp。

<a id="Std.Crypto.X25519.ScalarMultBase(byte[])"></a>
### static byte[] ScalarMultBase(byte[] scalar)

[`src/libraries/z42.crypto/src/X25519.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L60)

便捷形式：`scalar × 基点`（u=9），即由私钥派生公钥。

