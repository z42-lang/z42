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

AES（FIPS 197）——Rijndael 128 位块、128 / 192 / 256 位 key 的纯脚本实现（无 FFI）。

key 长度决定变体：16 字节 = AES-128，24 字节 = AES-192，32 字节 = AES-256。

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

[`src/libraries/z42.crypto/src/Aes.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L17)

加密一个 16 字节块。

`key` 为 16 / 24 / 32 字节，分别选择 AES-128 / 192 / 256；key 尺寸或明文
非 16 字节时抛 `ArgumentException`。

<a id="Std.Crypto.Aes.DecryptBlock(byte[],byte[])"></a>
### static byte[] DecryptBlock(byte[] key, byte[] ciphertext)

[`src/libraries/z42.crypto/src/Aes.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L40)

解密一个 16 字节块。key 尺寸规则同 EncryptBlock。

<a id="Std.Crypto.Aes.EncryptCtr(byte[],byte[],byte[])"></a>
### static byte[] EncryptCtr(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L66)

AES-CTR 加解密（CTR 模式下加密与解密同操作——与密钥流 XOR）。

IV 布局：8 字节 nonce || 8 字节大端 counter（从 0 起），逐块递增。同一 key + nonce
解密时传相同参数即可（密钥流确定）。

<a id="Std.Crypto.Aes.DecryptCtr(byte[],byte[],byte[])"></a>
### static byte[] DecryptCtr(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L104)

CTR 是对称的——解密与加密相同。

<a id="Std.Crypto.Aes.EncryptCbcPkcs7(byte[],byte[],byte[])"></a>
### static byte[] EncryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:111`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L111)

AES-CBC 加密，PKCS#7 填充（RFC 5652 §6.3——补到下一个 16 字节边界，已对齐也追加一整块填充）。

`iv` 必须 16 字节；输出长度恒为 ((data.Length / 16) + 1) * 16。

<a id="Std.Crypto.Aes.DecryptCbcPkcs7(byte[],byte[],byte[])"></a>
### static byte[] DecryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

[`src/libraries/z42.crypto/src/Aes.z42:149`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L149)

AES-CBC 解密并校验 / 剥离 PKCS#7 填充。

输入非 16 的倍数或填充非法抛 `ArgumentException`（通常表示 key / IV 错误或密文损坏）。

<a id="Std.Crypto.Aes.EncryptCcm(byte[],byte[],byte[],byte[],int)"></a>
### static byte[] EncryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext, int tagLen)

[`src/libraries/z42.crypto/src/Aes.z42:206`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L206)

AES-CCM 加密 + 认证。`nonce` 长度决定载荷上限，`tagLen` 控制认证 tag 截断。

<a id="Std.Crypto.Aes.DecryptCcm(byte[],byte[],byte[],byte[],int)"></a>
### static byte[] DecryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag, int tagLen)

[`src/libraries/z42.crypto/src/Aes.z42:232`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L232)

AES-CCM 解密 + 校验。输入 = ciphertext || encTag（tagLen 字节）。

tag 不匹配抛 `ArgumentException`（勿吞——篡改信号）。

<a id="Std.Crypto.Aes.EncryptGcm(byte[],byte[],byte[],byte[])"></a>
### static byte[] EncryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] plaintext)

[`src/libraries/z42.crypto/src/Aes.z42:439`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L439)

加密 + 认证。输出 = ciphertext（与 `plaintext` 等长）|| tag（16 字节）。

IV 12 字节（96 位）走快路径 J0 = IV || 0x00000001；其它长度按 NIST SP 800-38D §7.1 走
GHASH 推导。IV.Length == 0 拒绝（全零 J0 是安全隐患——同 key 多消息会复用同一 counter 流）。

<a id="Std.Crypto.Aes.DecryptGcm(byte[],byte[],byte[],byte[])"></a>
### static byte[] DecryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] ctAndTag)

[`src/libraries/z42.crypto/src/Aes.z42:464`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Aes.z42#L464)

解密 + 校验。输入 = ciphertext || 16 字节 tag。

tag 不匹配抛 `ArgumentException`（勿吞——tag 失败意味着密文或 AAD 被篡改）。

## Blake2b

[`src/libraries/z42.crypto/src/Blake2b.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L11)

BLAKE2b 哈希（RFC 7693）——输出 1..64 字节可配、支持 keyed（MAC）模式的快速哈希。

为软件速度设计的 SHA-3 候选 BLAKE 后继：比 SHA-2 / SHA-3 更简单、可并行。用于
Argon2、IPFS、WireGuard、Zcash、libsodium（`crypto_generichash`）等。16 字（1024 位）
工作向量上做 12 轮 `G` 混合，由 8 字状态做 key；块大小 128 字节、64 位字（z42 的
`long` 天然契合，逻辑右移用 `_lshr64`）。

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

[`src/libraries/z42.crypto/src/Blake2b.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L15)

计算默认 64 字节（512 位）BLAKE2b 摘要。

<a id="Std.Crypto.Blake2b.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L19)

对 UTF-8 字符串计算 BLAKE2b 摘要。

<a id="Std.Crypto.Blake2b.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L21)

计算 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L23)

对 UTF-8 字符串计算 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.Hash256(byte[])"></a>
### static byte[] Hash256(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L26)

计算 32 字节（256 位）BLAKE2b 摘要（常用于 IPFS / 区块链等内容寻址）。

<a id="Std.Crypto.Blake2b.Hash256String(string)"></a>
### static byte[] Hash256String(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L30)

对 UTF-8 字符串计算 256 位 BLAKE2b 摘要。

<a id="Std.Crypto.Blake2b.Hash256Hex(byte[])"></a>
### static string Hash256Hex(byte[] data)

[`src/libraries/z42.crypto/src/Blake2b.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L32)

计算 256 位 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.Hash256StringHex(string)"></a>
### static string Hash256StringHex(string s)

[`src/libraries/z42.crypto/src/Blake2b.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L34)

对 UTF-8 字符串计算 256 位 BLAKE2b 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2b.HashLen(byte[],byte[],int)"></a>
### static byte[] HashLen(byte[] data, byte[] key, int outLen)

[`src/libraries/z42.crypto/src/Blake2b.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2b.z42#L39)

BLAKE2b 变长 + keyed（MAC）模式。

`key.Length` ∈ [0, 64]、`outLen` ∈ [1, 64]，无 key 传 `new byte[0]`。

## Blake2s

[`src/libraries/z42.crypto/src/Blake2s.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L11)

BLAKE2s 哈希（RFC 7693 §B）——BLAKE2 的 32 位字变体，输出 1..32 字节、支持 keyed-MAC。

算法与 BLAKE2b 相同，但用 32 位字、64 字节块、10 轮（BLAKE2b 为 64 位 / 128 字节 /
12 轮）。key 0..32 字节。用于 Argon2i / Argon2id 内层压缩等 32 位友好场景。状态用
`int[8]`（i32），工作向量 `int[16]`；z42 的 `int` 是有符号 i32 且溢出回绕，32 位算术
直接成立。

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

[`src/libraries/z42.crypto/src/Blake2s.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L15)

计算默认 32 字节（256 位）BLAKE2s 摘要。

<a id="Std.Crypto.Blake2s.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Blake2s.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L19)

对 UTF-8 字符串计算 BLAKE2s 摘要。

<a id="Std.Crypto.Blake2s.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Blake2s.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L21)

计算 BLAKE2s 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2s.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Blake2s.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L23)

对 UTF-8 字符串计算 BLAKE2s 摘要并输出小写十六进制。

<a id="Std.Crypto.Blake2s.HashLen(byte[],byte[],int)"></a>
### static byte[] HashLen(byte[] data, byte[] key, int outLen)

[`src/libraries/z42.crypto/src/Blake2s.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake2s.z42#L28)

BLAKE2s 变长 + keyed（MAC）模式。

`key.Length` ∈ [0, 32]、`outLen` ∈ [1, 32]，无 key 传 `new byte[0]`。

## Blake3

[`src/libraries/z42.crypto/src/Blake3.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Blake3.z42#L12)

BLAKE3 哈希函数——BLAKE2 的 Merkle 树后继，默认 32 字节输出，XOF 模式任意长。

面向并行硬件（SIMD / 多核）设计。此串行纯脚本实现与官方 Rust 实现逐字节一致，但没有
并行加速（用于 Cargo crate 校验、IPFS 内容寻址等）。结构：输入按 1024 字节分 chunk，
每个 chunk 经 16 个 64 字节子块的压缩函数迭代成 32 字节链值（CV），chunk CV 按二叉树
两两合并，根节点带 ROOT 标志产出最终输出。压缩函数与 BLAKE2s 同形（32 位字、16 字
工作向量上的 G 混合），但减为 7 轮、换用不同 SIGMA 置换表并注入 counter / flags。

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

[`src/libraries/z42.crypto/src/ChaCha20.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L12)

ChaCha20 流密码（RFC 8439）——key 32 字节、nonce 12 字节、counter 32 位的 Salsa20 改进。

Daniel Bernstein 对 Salsa20 的 20 轮改进，用于 TLS 1.3、WireGuard、age、Signal（无
AES-NI 环境下的 AES 替代）。RFC 8439 将 8 字节 nonce + 8 字节 counter 改为 12+4 以
对齐 AES-GCM；counter 从 1 起（counter=0 留给 AEAD 里 Poly1305 一次性 key 派生）。
状态为 4×4 的 32 位字矩阵：常量 "expand 32-byte k"、8 个小端 key 字、counter + 3 个
nonce 字。20 轮 = 10 列轮 + 10 对角轮，每轮 4 个 quarter-round；最终密钥流块 =
working_state + original_state（逐字模 2^32），小端 64 字节。

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

[`src/libraries/z42.crypto/src/ChaCha20.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L19)

加密 / 解密（对称，与密钥流 XOR）。

独立使用时 `counter` 从 1 起（RFC 8439 §2.4）；仅派生 Poly1305 一次性 key
时传 0。

<a id="Std.Crypto.ChaCha20.Encrypt(byte[],byte[],byte[])"></a>
### static byte[] Encrypt(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/ChaCha20.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L45)

便捷形式，默认 counter=1（RFC 8439 独立使用）。

<a id="Std.Crypto.ChaCha20.Decrypt(byte[],byte[],byte[])"></a>
### static byte[] Decrypt(byte[] key, byte[] nonce, byte[] data)

[`src/libraries/z42.crypto/src/ChaCha20.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L50)

解密——与加密同义。

<a id="Std.Crypto.ChaCha20.Block(byte[],byte[],int)"></a>
### static byte[] Block(byte[] key, byte[] nonce, int counter)

[`src/libraries/z42.crypto/src/ChaCha20.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20.z42#L55)

产生 (key, nonce, counter) 的一个 64 字节密钥流块，供 Poly1305 key 派生（counter=0 路径）使用。

## ChaCha20Poly1305

[`src/libraries/z42.crypto/src/ChaCha20Poly1305.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20Poly1305.z42#L11)

ChaCha20-Poly1305 AEAD（RFC 8439 §2.8）——输出 = ciphertext || 16 字节 Poly1305 tag。

构造：① 由 ChaCha20(key, nonce, counter=0) 派生 Poly1305 一次性 key（otk，只用 64 字节块的
前 32 字节）；② 从 counter=1 起用 ChaCha20 加密；③ tag = Poly1305(otk, aad || pad16(aad) ||
ciphertext || pad16(ciphertext) || len(aad)_8LE || len(ciphertext)_8LE)；④ 输出拼接。
`pad16(x)` 把 x 补零到下一个 16 字节边界。用于 TLS 1.3、WireGuard、age、NaCl
`crypto_aead_*`，是 AES-256-GCM 的软件友好替代。

```z42
public static class ChaCha20Poly1305 {
    static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)
    static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)
}
```

成员：[`Encrypt`](#Std.Crypto.ChaCha20Poly1305.Encrypt(byte[],byte[],byte[],byte[])) · [`Decrypt`](#Std.Crypto.ChaCha20Poly1305.Decrypt(byte[],byte[],byte[],byte[]))

<a id="Std.Crypto.ChaCha20Poly1305.Encrypt(byte[],byte[],byte[],byte[])"></a>
### static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)

[`src/libraries/z42.crypto/src/ChaCha20Poly1305.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20Poly1305.z42#L13)

加密 + 认证。输出 = ciphertext（与 `plaintext` 等长）|| tag（16 字节）。

<a id="Std.Crypto.ChaCha20Poly1305.Decrypt(byte[],byte[],byte[],byte[])"></a>
### static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)

[`src/libraries/z42.crypto/src/ChaCha20Poly1305.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ChaCha20Poly1305.z42#L41)

解密 + 校验。输入 = ciphertext || 16 字节 tag。

tag 不匹配抛 `ArgumentException`（勿吞——表示密文或 AAD 被篡改）。

## ConstantTime

[`src/libraries/z42.crypto/src/ConstantTime.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ConstantTime.z42#L9)

常数时间字节数组比较（公开原语）。

没有它，校验 MAC / 认证令牌 / 密码哈希的调用方只能用 `==` 或带提前退出的普通循环，
后者在首个差异字节处短路、经计时泄漏「前导匹配了几个字节」，可逐字节伪造。本包 AEAD
（ChaCha20Poly1305 / AES-GCM/CCM 的 tag 校验）内部已用同样手法；此原语暴露给用户安全比较 secret。

```z42
public static class ConstantTime {
    static bool Equals(byte[] a, byte[] b)
}
```

成员：[`Equals`](#Std.Crypto.ConstantTime.Equals(byte[],byte[]))

<a id="Std.Crypto.ConstantTime.Equals(byte[],byte[])"></a>
### static bool Equals(byte[] a, byte[] b)

[`src/libraries/z42.crypto/src/ConstantTime.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/ConstantTime.z42#L14)

无提前退出的字节数组比较。

运行时间只依赖长度（非秘密），从不依赖首个差异位置。长度不同立即返回 false
（只泄漏长度）；长度相同时始终逐字节检查，累计差异决定结果。

## EcdsaP256

[`src/libraries/z42.crypto/src/EcdsaP256.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/EcdsaP256.z42#L11)

NIST P-256（secp256r1）曲线上的 ECDSA（FIPS 186-4 + RFC 6979 确定性 nonce）。

P-256 是 JWT ES256、TLS 1.3 默认密钥共享、X.509 证书 PKI 的默认曲线。曲线为短
Weierstrass `y² = x³ - 3x + b mod p`（a = -3）。接口与 `EcdsaSecp256k1` 完全相同，仅
曲线常数与加倍公式不同。标量乘用 Jacobian 坐标（入口 Z=1 抬升、出口单次 ModInverse），
a=-3 的加倍用 `M = 3·(X − Z²)·(X + Z²)` 分解（≡ 3X² + a·Z⁴），避免显式 `-3·Z⁴` 项。

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

secp256k1 曲线上的 ECDSA（SEC2 + RFC 6979 确定性 nonce），用于 Bitcoin 等区块链协议。

曲线为短 Weierstrass `y² = x³ + 7 mod p`（a = 0，Bitcoin 选的 "Koblitz" 形式）。
接口与 `EcdsaP256` 完全相同，仅曲线常数与加倍公式不同（a = 0 使 M = 3X²）。标量乘用
Jacobian 坐标（入口 Z=1 抬升、出口单次 ModInverse），曲线常数 p / n 每次公共入口解析一次。

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

Ed25519 数字签名（RFC 8032 §5.1），基于 `Std.Numerics.BigInt` 的纯脚本实现。

在 GF(2^255 - 19) 上的扭曲 Edwards曲线 -x² + y² = 1 + d·x²·y²（d = -121665 / 121666 mod p）
上，曲线阶 ℓ = 2^252 + 27742317777372353535851937790883648493。用于 SSH `ssh-ed25519`、
age、JWT EdDSA、libsodium 等（性能同 X25519 的说明）。

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

[`src/libraries/z42.crypto/src/Hkdf.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L9)

HKDF-SHA-256（RFC 5869）——基于 HMAC-SHA-256 的 Extract-then-Expand 密钥派生函数。

用于 TLS 1.3、Signal、Noise、JWT 等。纯脚本实现于 HmacSha256 之上。`salt` 可为空
（按 RFC 5869 §2.2 替换为 HashLen 个零字节），`info` 可为空，`length` ≤ 255 × HashLen。

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

[`src/libraries/z42.crypto/src/Hkdf.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L17)

一次性 Extract + Expand。

两阶段单独暴露（`Extract` / `Expand`），供需要在其间穿插其他状态（如
TLS 1.3 密钥表）的调用方使用。

<a id="Std.Crypto.HkdfSha256.Extract(byte[],byte[])"></a>
### static byte[] Extract(byte[] salt, byte[] ikm)

[`src/libraries/z42.crypto/src/Hkdf.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L29)

HKDF-Extract（RFC 5869 §2.2）：PRK = HMAC-Hash(salt, IKM)。

空 salt → 32 个零字节，输出 32 字节。

<a id="Std.Crypto.HkdfSha256.Expand(byte[],byte[],int)"></a>
### static byte[] Expand(byte[] prk, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L40)

HKDF-Expand（RFC 5869 §2.3）：把 PRK 扩展成 `length` 字节输出密钥材料，`info` 作上下文绑定。

<a id="Std.Crypto.HkdfSha256.DeriveHex(byte[],byte[],byte[],int)"></a>
### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L81)

十六进制便捷变体，用于文档 / 调试。

## HkdfSha512

[`src/libraries/z42.crypto/src/Hkdf.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L87)

HKDF-SHA-512（RFC 5869）——同 `HkdfSha256` 算法，HashLen 64 字节，建于 HmacSha512 之上。

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

[`src/libraries/z42.crypto/src/Hkdf.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L92)

一次性 Extract + Expand（HKDF-SHA-512，HashLen 64 字节）。

<a id="Std.Crypto.HkdfSha512.Extract(byte[],byte[])"></a>
### static byte[] Extract(byte[] salt, byte[] ikm)

[`src/libraries/z42.crypto/src/Hkdf.z42:102`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L102)

HKDF-Extract：PRK = HMAC-SHA512(salt, IKM)，输出 64 字节。

<a id="Std.Crypto.HkdfSha512.Expand(byte[],byte[],int)"></a>
### static byte[] Expand(byte[] prk, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L113)

HKDF-Expand：把 PRK 扩展成 `length` 字节输出密钥材料。

<a id="Std.Crypto.HkdfSha512.DeriveHex(byte[],byte[],byte[],int)"></a>
### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:147`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L147)

十六进制便捷变体。

## HkdfSha1

[`src/libraries/z42.crypto/src/Hkdf.z42:156`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L156)

HKDF-SHA-1（RFC 5869）——同算法，HashLen 20 字节，仅用于 SHA-1 仍在线的旧协议 / IoT。

用于部分 TLS 1.2 PRF、旧移动端密钥派生、ANSI X9.63 KDF-1/2。新设计用 HkdfSha256 /
HkdfSha512——SHA-1 碰撞抵抗已破，但 HKDF 派生只需 PRF 安全性（SHA-1 仍具备）。

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

[`src/libraries/z42.crypto/src/Hkdf.z42:161`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L161)

一次性 Extract + Expand（HKDF-SHA-1，HashLen 20 字节）。

<a id="Std.Crypto.HkdfSha1.Extract(byte[],byte[])"></a>
### static byte[] Extract(byte[] salt, byte[] ikm)

[`src/libraries/z42.crypto/src/Hkdf.z42:171`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L171)

HKDF-Extract：PRK = HMAC-SHA1(salt, IKM)，输出 20 字节。

<a id="Std.Crypto.HkdfSha1.Expand(byte[],byte[],int)"></a>
### static byte[] Expand(byte[] prk, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:182`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L182)

HKDF-Expand：把 PRK 扩展成 `length` 字节输出密钥材料。

<a id="Std.Crypto.HkdfSha1.DeriveHex(byte[],byte[],byte[],int)"></a>
### static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)

[`src/libraries/z42.crypto/src/Hkdf.z42:216`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hkdf.z42#L216)

十六进制便捷变体。

## HmacSha256

[`src/libraries/z42.crypto/src/Hmac.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L9)

HMAC-SHA256（RFC 2104），纯脚本实现于 `Sha256` 之上。

算法：K' = len(K)>64 时 SHA256(K)，否则 K 右补 0x00 到 64 字节；
inner = SHA256((K' XOR 0x36×64) || msg)；HMAC = SHA256((K' XOR 0x5C×64) || inner)。

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

[`src/libraries/z42.crypto/src/Hmac.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L13)

计算 `message` 在 `key` 下的 HMAC-SHA256，返回 32 字节摘要。

<a id="Std.Crypto.HmacSha256.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L46)

对 UTF-8 编码的字符串 key / message 计算 HMAC-SHA256（返回 32 字节）。

<a id="Std.Crypto.HmacSha256.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L53)

计算 HMAC-SHA256 并输出小写十六进制（64 字符）。

<a id="Std.Crypto.HmacSha256.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L58)

对 UTF-8 字符串 key / message 计算 HMAC-SHA256 并输出小写十六进制。

## HmacSha1

[`src/libraries/z42.crypto/src/Hmac.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L93)

HMAC-SHA-1（RFC 2104），布局与 HmacSha256 相同。块大小 64（与 SHA-256 同），摘要 20 字节。

⚠️ 仅当旧系统互操作要求时使用（TOTP RFC 6238 默认、AWS Signature V2 等），新设计用 HMAC-SHA-256。

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

[`src/libraries/z42.crypto/src/Hmac.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L95)

计算 `message` 在 `key` 下的 HMAC-SHA1，返回 20 字节摘要。

<a id="Std.Crypto.HmacSha1.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L132)

对 UTF-8 字符串 key / message 计算 HMAC-SHA1。

<a id="Std.Crypto.HmacSha1.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L139)

计算 HMAC-SHA1 并输出小写十六进制。

<a id="Std.Crypto.HmacSha1.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:144`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L144)

对 UTF-8 字符串 key / message 计算 HMAC-SHA1 并输出小写十六进制。

## HmacSha512

[`src/libraries/z42.crypto/src/Hmac.z42:172`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L172)

HMAC-SHA-512（RFC 2104）。块大小 128（SHA-256 为 64），摘要 64 字节。

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

[`src/libraries/z42.crypto/src/Hmac.z42:174`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L174)

计算 `message` 在 `key` 下的 HMAC-SHA512，返回 64 字节摘要。

<a id="Std.Crypto.HmacSha512.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:211`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L211)

对 UTF-8 字符串 key / message 计算 HMAC-SHA512。

<a id="Std.Crypto.HmacSha512.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:218`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L218)

计算 HMAC-SHA512 并输出小写十六进制。

<a id="Std.Crypto.HmacSha512.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:223`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L223)

对 UTF-8 字符串 key / message 计算 HMAC-SHA512 并输出小写十六进制。

## HmacSha384

[`src/libraries/z42.crypto/src/Hmac.z42:252`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L252)

HMAC-SHA-384（RFC 2104）。块大小 128（与 HMAC-SHA-512 同），内部用 SHA-384 所以摘要 48 字节。

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

[`src/libraries/z42.crypto/src/Hmac.z42:254`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L254)

计算 `message` 在 `key` 下的 HMAC-SHA384，返回 48 字节摘要。

<a id="Std.Crypto.HmacSha384.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:290`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L290)

对 UTF-8 字符串 key / message 计算 HMAC-SHA384。

<a id="Std.Crypto.HmacSha384.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:297`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L297)

计算 HMAC-SHA384 并输出小写十六进制。

<a id="Std.Crypto.HmacSha384.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:302`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L302)

对 UTF-8 字符串 key / message 计算 HMAC-SHA384 并输出小写十六进制。

## HmacMd5

[`src/libraries/z42.crypto/src/Hmac.z42:333`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L333)

HMAC-MD5（RFC 2104），HmacSha1 的纯脚本镜像——同为 64 字节块，MD5 摘要 16 字节。

⚠️ HMAC-MD5 作为 MAC 仍基本可用（Wang 碰撞攻击不直接扩展到 HMAC，外层 key 随机化了 H 状态），
但仅当协议强制（HTTP Digest-MD5 / CRAM-MD5 / APR1 / legacy SASL）时使用；新设计用 HMAC-SHA-256。

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

[`src/libraries/z42.crypto/src/Hmac.z42:335`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L335)

计算 `message` 在 `key` 下的 HMAC-MD5，返回 16 字节摘要。

<a id="Std.Crypto.HmacMd5.ComputeString(string,string)"></a>
### static byte[] ComputeString(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:372`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L372)

对 UTF-8 字符串 key / message 计算 HMAC-MD5。

<a id="Std.Crypto.HmacMd5.ComputeHex(byte[],byte[])"></a>
### static string ComputeHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Hmac.z42:379`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L379)

计算 HMAC-MD5 并输出小写十六进制。

<a id="Std.Crypto.HmacMd5.ComputeStringHex(string,string)"></a>
### static string ComputeStringHex(string key, string message)

[`src/libraries/z42.crypto/src/Hmac.z42:384`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Hmac.z42#L384)

对 UTF-8 字符串 key / message 计算 HMAC-MD5 并输出小写十六进制。

## Md5

[`src/libraries/z42.crypto/src/Md5.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L10)

MD5 哈希（RFC 1321），纯脚本实现。⚠️ 碰撞已破，勿用于新设计，仅限旧格式互操作。

结构与 `Sha1` 相同。状态用 `long`（i64）保存，每一步运算边界掩码到 32 位；全程小端
字节序（RFC 1321 §3.4，与 SHA-1 相反）。碰撞（Wang et al 2004；前缀碰撞 Stevens 2007）
意味着仅用于 HTTP Digest 认证、ETag、CRAM-MD5、BitTorrent v1 等协议强制的旧格式互操作。

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

[`src/libraries/z42.crypto/src/Md5.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L14)

计算 MD5 原始摘要（16 字节）。

<a id="Std.Crypto.Md5.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Md5.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L38)

对 UTF-8 编码后的字符串计算 MD5 摘要。

<a id="Std.Crypto.Md5.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Md5.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L43)

计算 MD5 摘要并输出小写十六进制。

<a id="Std.Crypto.Md5.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Md5.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Md5.z42#L48)

对 UTF-8 字符串计算 MD5 摘要并输出小写十六进制。

## Pbkdf2

[`src/libraries/z42.crypto/src/Pbkdf2.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L10)

PBKDF2-HMAC-SHA256（RFC 8018 §5.2），用于密码哈希 / KDF。

比直接 SHA-256 慢约 `iterations` 倍，典型生产值 100000。算法（RFC 8018）：
T(i) = U1 XOR U2 XOR ... XOR Uc（c = iterations），U1 = HMAC(P, S || INT(i))（INT 为
4 字节大端），Uj = HMAC(P, Uj-1)；DK = T(1) || T(2) || ...，截断到 `dkLen` 字节。

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

[`src/libraries/z42.crypto/src/Pbkdf2.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L16)

PBKDF2-HMAC-SHA256 派生，返回 `dkLen` 字节。

`iterations < 1` 或 `dkLen < 1` 抛 `ArgumentException`。

<a id="Std.Crypto.Pbkdf2.DeriveKeyString(string,string,int,int)"></a>
### static byte[] DeriveKeyString(string password, string salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L52)

字符串 password / salt 便捷形式，按 UTF-8 编码。

<a id="Std.Crypto.Pbkdf2.DeriveKeyHex(byte[],byte[],int,int)"></a>
### static string DeriveKeyHex(byte[] password, byte[] salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L58)

派生并输出小写十六进制（长度 2 × dkLen）。

<a id="Std.Crypto.Pbkdf2.DeriveKeyStringHex(string,string,int,int)"></a>
### static string DeriveKeyStringHex(string password, string salt, int iterations, int dkLen)

[`src/libraries/z42.crypto/src/Pbkdf2.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Pbkdf2.z42#L64)

对字符串 password / salt 派生并输出小写十六进制。

## Poly1305

[`src/libraries/z42.crypto/src/Poly1305.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Poly1305.z42#L14)

Poly1305 一次性 MAC（RFC 8439 §2.5），GF(2^130 - 5) 上的认证器。

用于 ChaCha20-Poly1305 AEAD 与 NaCl / libsodium。⚠️ 一次性：32 字节 key 对每条消息必须
全新——复用会泄漏多项式系数、彻底失去安全性；AEAD 中 key 每消息由
ChaCha20(key, nonce, counter=0)[0..32] 派生。算法：key = r（16 字节，clamp）|| s（16 字节）；
对 (message || 0x01 填充) 的每个 17 字节块，acc = ((acc + block) * r) mod (2^130 - 5)；
tag = (acc + s) mod 2^128（16 字节小端输出）。基于 `Std.Numerics.BigInt` 实现
（z42 无原生 128/160 位整数）。

```z42
public static class Poly1305 {
    static byte[] Mac(byte[] key, byte[] message)
    static string MacHex(byte[] key, byte[] message)
}
```

成员：[`Mac`](#Std.Crypto.Poly1305.Mac(byte[],byte[])) · [`MacHex`](#Std.Crypto.Poly1305.MacHex(byte[],byte[]))

<a id="Std.Crypto.Poly1305.Mac(byte[],byte[])"></a>
### static byte[] Mac(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Poly1305.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Poly1305.z42#L16)

在 32 字节一次性 key 下计算 `message` 的 16 字节认证 tag，key 长度错误抛 `ArgumentException`。

<a id="Std.Crypto.Poly1305.MacHex(byte[],byte[])"></a>
### static string MacHex(byte[] key, byte[] message)

[`src/libraries/z42.crypto/src/Poly1305.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Poly1305.z42#L71)

计算 tag 并输出十六进制。

## RsaPublicKey

[`src/libraries/z42.crypto/src/Rsa.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L6)

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

[`src/libraries/z42.crypto/src/Rsa.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L8)

模数。

<a id="Std.Crypto.RsaPublicKey.e"></a>
### BigInt e

[`src/libraries/z42.crypto/src/Rsa.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L10)

公钥指数。

<a id="Std.Crypto.RsaPublicKey.ctor(BigInt,BigInt)"></a>
### RsaPublicKey(BigInt n, BigInt e)

[`src/libraries/z42.crypto/src/Rsa.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L12)

用模数 n 与公钥指数 e 构造。

## RsaPrivateKey

[`src/libraries/z42.crypto/src/Rsa.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L19)

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

[`src/libraries/z42.crypto/src/Rsa.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L21)

模数。

<a id="Std.Crypto.RsaPrivateKey.e"></a>
### BigInt e

[`src/libraries/z42.crypto/src/Rsa.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L23)

公钥指数。

<a id="Std.Crypto.RsaPrivateKey.d"></a>
### BigInt d

[`src/libraries/z42.crypto/src/Rsa.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L25)

私钥指数。

<a id="Std.Crypto.RsaPrivateKey.ctor(BigInt,BigInt,BigInt)"></a>
### RsaPrivateKey(BigInt n, BigInt e, BigInt d)

[`src/libraries/z42.crypto/src/Rsa.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L27)

用模数 n、公钥指数 e、私钥指数 d 构造。

<a id="Std.Crypto.RsaPrivateKey.GetPublicKey()"></a>
### RsaPublicKey GetPublicKey()

[`src/libraries/z42.crypto/src/Rsa.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L34)

派生匹配的公钥。

## Rsa

[`src/libraries/z42.crypto/src/Rsa.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L45)

RSA 原语（RFC 8017 / PKCS#1 v2.2）——签名 / 验签 / 加解密与裸模幂。

纯脚本实现于 `Std.Numerics.BigInt.ModPow`。密钥用原始 n / e / d 的 BigInt 构造，字段
公开可写。支持：RSASSA-PKCS1-v1_5 与 RSASSA-PSS 签名验签（SHA-256）、RSAES-OAEP
加解密（SHA-256）、以及裸模幂 RawPublicOp / RawPrivateOp。纯脚本 RSA-2048 签名约数秒级，
验签快（e=65537 仅 17 位）。

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

[`src/libraries/z42.crypto/src/Rsa.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L59)

用 SHA-256 + PKCS#1 v1.5 填充对 `message` 签名，输出恰好 `n.byteLength()` 字节。

<a id="Std.Crypto.Rsa.VerifyPkcs1v15Sha256(RsaPublicKey,byte[],byte[])"></a>
### static bool VerifyPkcs1v15Sha256(RsaPublicKey publicKey, byte[] message, byte[] signature)

[`src/libraries/z42.crypto/src/Rsa.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L69)

校验 SHA-256(message) 的 PKCS#1 v1.5 签名；有效返回 true。签名内容错误返回 false（仅长度错误抛异常）。

<a id="Std.Crypto.Rsa.SignPssSha256(RsaPrivateKey,byte[],byte[])"></a>
### static byte[] SignPssSha256(RsaPrivateKey privateKey, byte[] message, byte[] salt)

[`src/libraries/z42.crypto/src/Rsa.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L138)

RSASSA-PSS / SHA-256 / MGF1-SHA-256 签名，`salt` 长度由调用方定（推荐 32 字节），输出 n 的字节长度。

<a id="Std.Crypto.Rsa.VerifyPssSha256(RsaPublicKey,byte[],byte[],int)"></a>
### static bool VerifyPssSha256(RsaPublicKey publicKey, byte[] message, byte[] signature, int saltLen)

[`src/libraries/z42.crypto/src/Rsa.z42:165`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L165)

校验 RSASSA-PSS / SHA-256 签名。

`saltLen` 必须与签名时一致（RFC 8017 §9.1.2 步骤 11——验签方知道 salt 长度
而非 salt 本身）。

<a id="Std.Crypto.Rsa.EncryptOaepSha256(RsaPublicKey,byte[],byte[],byte[])"></a>
### static byte[] EncryptOaepSha256(RsaPublicKey publicKey, byte[] message, byte[] label, byte[] seed)

[`src/libraries/z42.crypto/src/Rsa.z42:305`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L305)

RSA-OAEP / SHA-256 / MGF1-SHA-256 加密。`label` 可为空（用于上下文绑定），`seed` 必须 32 字节。

<a id="Std.Crypto.Rsa.DecryptOaepSha256(RsaPrivateKey,byte[],byte[])"></a>
### static byte[] DecryptOaepSha256(RsaPrivateKey privateKey, byte[] ciphertext, byte[] label)

[`src/libraries/z42.crypto/src/Rsa.z42:357`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L357)

RSA-OAEP / SHA-256 / MGF1-SHA-256 解密，返回明文。

填充非法抛 `ArgumentException`（勿在调用层区分错误——区分 padding 错误与
值错误的计时泄漏风险正是 OAEP 要缓解的）。

<a id="Std.Crypto.Rsa.RawPublicOp(RsaPublicKey,byte[])"></a>
### static byte[] RawPublicOp(RsaPublicKey publicKey, byte[] message)

[`src/libraries/z42.crypto/src/Rsa.z42:467`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L467)

裸 `message^e mod n` 模幂（RSAEP，RFC 8017 §5.1.1），无 padding。

输入须在 [0, n)，返回 n.byteLength() 字节。

<a id="Std.Crypto.Rsa.RawPrivateOp(RsaPrivateKey,byte[])"></a>
### static byte[] RawPrivateOp(RsaPrivateKey privateKey, byte[] ciphertext)

[`src/libraries/z42.crypto/src/Rsa.z42:478`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Rsa.z42#L478)

裸 `ciphertext^d mod n`（RSADP，RFC 8017 §5.1.2），无 padding 剥离，输入约束同 RawPublicOp。

## Scrypt

[`src/libraries/z42.crypto/src/Scrypt.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Scrypt.z42#L10)

scrypt 口令哈希 KDF（RFC 7914 / Colin Percival 2009），PBKDF2 的内存硬替代。

暴力破解每次尝试需 O(N × r) 内存，令 GPU / ASIC 攻击远比单纯并行昂贵。流程：
① PBKDF2-HMAC-SHA256(password, salt, 1, p × 128 × r) → B[0..p-1]；② 每个 B[i] 做
ROMix(B[i], N)（内存硬步骤）；③ 再做 PBKDF2 得 DK。ROMix 中 BlockMix 用 Salsa20/8
（8 轮变体）作混合原语（RFC 7914 §3）。

```z42
public static class Scrypt {
    static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)
}
```

成员：[`Derive`](#Std.Crypto.Scrypt.Derive(byte[],byte[],int,int,int,int))

<a id="Std.Crypto.Scrypt.Derive(byte[],byte[],int,int,int,int)"></a>
### static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)

[`src/libraries/z42.crypto/src/Scrypt.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Scrypt.z42#L14)

用 scrypt(N, r, p) 从 `password` + `salt` 派生 `dkLen` 字节。

约束：N 为 2 的幂且 ≥ 2，r ≥ 1，p ≥ 1，r × p < 2^30，dkLen ≥ 0。

## SecureRandom

[`src/libraries/z42.crypto/src/SecureRandom.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L11)

OS 支撑的密码学安全随机数发生器，安全语义门面（rejection sampling、有界整数）。

原始熵抽头 `__crypto_random_bytes` 已迁移到核心 `Std.Runtime.Entropy`（跨切面原语，
供 core 的 Guid.NewGuid 复用）；SecureRandom 委托到那里（单一声明点）。
**用这里生成** session token、CSRF nonce、KDF salt、掩码密钥、口令等一切需要不可预测性的
场合。**勿用 `Std.Random`**（PCG，确定性 PRNG，用于 fixture / 仿真 / 临时 ID）做安全用途。
wasm32 抛 `Std.NotSupportedException`。

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

[`src/libraries/z42.crypto/src/SecureRandom.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L13)

从 OS CSPRNG 填充 n 字节（n ≥ 0）。

<a id="Std.Crypto.SecureRandom.NextInt()"></a>
### static int NextInt()

[`src/libraries/z42.crypto/src/SecureRandom.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L21)

整个 i32 范围均匀分布的随机 int（含负数）。由 4 个 CSPRNG 字节大端构建。

<a id="Std.Crypto.SecureRandom.NextLong()"></a>
### static long NextLong()

[`src/libraries/z42.crypto/src/SecureRandom.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L31)

整个 i64 范围均匀分布的随机 long，由 8 个 CSPRNG 字节大端构建。

<a id="Std.Crypto.SecureRandom.NextU32Bounded(int)"></a>
### static int NextU32Bounded(int bound)

[`src/libraries/z42.crypto/src/SecureRandom.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/SecureRandom.z42#L45)

[0, bound) 均匀分布的随机 int，用 rejection sampling 避免模偏差。

bound ≤ 0 抛 `ArgumentException`。

## Sha1

[`src/libraries/z42.crypto/src/Sha1.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L10)

SHA-1 哈希（FIPS 180-4 §6.1），纯脚本实现。⚠️ 碰撞已破（SHAttered），仅限既有协议互操作。

结构与 `Sha256` 相同。状态用 `long`（i64）保存，每一步运算边界掩码到 32 位——z42 的
`int` 是有符号 i32，消息表加法会溢出。自 SHAttered（2017）起仅用于 HMAC-SHA1、git
对象哈希、Sec-WebSocket-Accept（RFC 6455 §4.2.2）等既有协议互操作，勿用于新设计。

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

[`src/libraries/z42.crypto/src/Sha1.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L14)

计算 SHA-1 原始摘要（20 字节）。

<a id="Std.Crypto.Sha1.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha1.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L37)

对 UTF-8 编码后的字符串计算 SHA-1 摘要。

<a id="Std.Crypto.Sha1.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha1.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L42)

计算 SHA-1 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha1.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha1.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha1.z42#L47)

对 UTF-8 字符串计算 SHA-1 摘要并输出小写十六进制。

## Sha256

[`src/libraries/z42.crypto/src/Sha256.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L37)

SHA-256 哈希（FIPS 180-4），纯脚本实现。

状态用 `long`（i64）保存，每一步运算边界都掩码到 32 位——z42 的 `int` 是有符号 i32，
消息表加法会溢出。

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

[`src/libraries/z42.crypto/src/Sha256.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L41)

计算 SHA-256 原始摘要（32 字节）。

<a id="Std.Crypto.Sha256.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha256.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L64)

对 UTF-8 编码后的字符串计算 SHA-256 摘要。

<a id="Std.Crypto.Sha256.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha256.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L69)

计算 SHA-256 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha256.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha256.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha256.z42#L74)

对 UTF-8 字符串计算 SHA-256 摘要并输出小写十六进制。

## Sha3

[`src/libraries/z42.crypto/src/Sha3.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha3.z42#L14)

SHA-3（FIPS 202）哈希族，基于 Keccak-f[1600] 海绵结构的纯脚本实现。

海绵结构（区别于 SHA-2 的 Merkle-Damgård）：1600 位（25 × 64 位）状态经 Keccak-f 置换，
「吸收」阶段把输入 XOR 进 rate 部分，「挤压」阶段从同一 rate 部分读出输出字节。状态布局
为扁平 `long[25]`，索引 `state[x + 5*y]`，x、y ∈ 0..4 为 FIPS 202 §3.1.2 的列/行坐标。
四个变体（FIPS 202 §6.1）共用 Keccak-f[1600]，仅 rate `r` 不同：SHA3-224（r=144 字节）、
SHA3-256（136）、SHA3-384（104）、SHA3-512（72）。域分隔字节 = 0x06；填充最后一字节高位
（0x80）置位。注意本实现是 FIPS 202 SHA-3，不是以太坊用的 legacy Keccak（后者域分隔
字节为 0x01）。

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

[`src/libraries/z42.crypto/src/Sha384.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L8)

SHA-384 哈希（FIPS 180-4 §6.5）——SHA-512 换初始哈希值并把输出截断为前 48 字节。

输出取最终状态的前 6 个 64 位字。压缩函数通过 `Sha512._compressBlock` 与 Sha512 共用。

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

[`src/libraries/z42.crypto/src/Sha384.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L10)

计算 SHA-384 原始摘要（48 字节）。

<a id="Std.Crypto.Sha384.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha384.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L30)

对 UTF-8 编码后的字符串计算 SHA-384 摘要。

<a id="Std.Crypto.Sha384.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha384.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L32)

计算 SHA-384 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha384.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha384.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha384.z42#L34)

对 UTF-8 字符串计算 SHA-384 摘要并输出小写十六进制。

## Sha512

[`src/libraries/z42.crypto/src/Sha512.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L10)

SHA-512 哈希（FIPS 180-4），纯脚本实现；块 128 字节、输出 64 字节、80 轮。

状态用 `long`（i64）保存——SHA-512 字长 64 位，天然契合 z42 的有符号 i64；负数
`>>` 会符号扩展，所以高位可能置 1 处用 `_lshr64` 做逻辑右移。SHA-384（Sha384.z42）
通过暴露的 `_compressBlock` 共用压缩函数，仅 IV 与输出截断不同。

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

[`src/libraries/z42.crypto/src/Sha512.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L14)

计算 SHA-512 原始摘要（64 字节）。

<a id="Std.Crypto.Sha512.HashString(string)"></a>
### static byte[] HashString(string s)

[`src/libraries/z42.crypto/src/Sha512.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L33)

对 UTF-8 编码后的字符串计算 SHA-512 摘要。

<a id="Std.Crypto.Sha512.HashHex(byte[])"></a>
### static string HashHex(byte[] data)

[`src/libraries/z42.crypto/src/Sha512.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L35)

计算 SHA-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha512.HashStringHex(string)"></a>
### static string HashStringHex(string s)

[`src/libraries/z42.crypto/src/Sha512.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L37)

对 UTF-8 字符串计算 SHA-512 摘要并输出小写十六进制。

<a id="Std.Crypto.Sha512._initialHash()"></a>
### static long[] _initialHash()

[`src/libraries/z42.crypto/src/Sha512.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L45)

构造 SHA-512 的初始 IV（8 个 i64）。

<a id="Std.Crypto.Sha512._pad(byte[])"></a>
### static byte[] _pad(byte[] data)

[`src/libraries/z42.crypto/src/Sha512.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L105)

MD 填充（供 Sha384 复用）：0x80 + 补零到 112 mod 128 + 16 字节大端位长。

长度字段高 8 字节恒为 0——z42 的字节长度 × 8 总能用 i64 表示。

<a id="Std.Crypto.Sha512._compressBlock(long[],byte[],int)"></a>
### static void _compressBlock(long[] h, byte[] block, int offset)

[`src/libraries/z42.crypto/src/Sha512.z42:126`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L126)

把 `block[offset..]` 起的一个 128 字节块压缩进 `h[0..7]`（就地累加）。

`_` 前缀的内部接口，供 Sha384 携带自己的 IV 复用。

<a id="Std.Crypto.Sha512._lshr64(long,int)"></a>
### static long _lshr64(long x, int n)

[`src/libraries/z42.crypto/src/Sha512.z42:189`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L189)

i64 逻辑右移 `n` 位；负数时用掩码 XOR 掉算术 `>>` 引入的符号扩展位。

<a id="Std.Crypto.Sha512._rotr64(long,int)"></a>
### static long _rotr64(long x, int n)

[`src/libraries/z42.crypto/src/Sha512.z42:196`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L196)

i64 循环右移 `n` 位（n ∈ [1, 63]）。

<a id="Std.Crypto.Sha512._readBE64(byte[],int)"></a>
### static long _readBE64(byte[] data, int offset)

[`src/libraries/z42.crypto/src/Sha512.z42:203`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L203)

按大端读 `data[offset..offset+7]` 的 8 字节组成 i64。

<a id="Std.Crypto.Sha512._writeBE64(byte[],int,long)"></a>
### static void _writeBE64(byte[] data, int offset, long v)

[`src/libraries/z42.crypto/src/Sha512.z42:217`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/Sha512.z42#L217)

把 `v` 按大端写进 `data[offset..offset+7]` 的 8 字节。

## X25519

[`src/libraries/z42.crypto/src/X25519.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L11)

X25519 椭圆曲线 Diffie-Hellman（RFC 7748）——Curve25519 上的密钥协商。

在 Curve25519（Montgomery 曲线 y² = x³ + 486662x² + x 于 GF(p)，p = 2^255 - 19）上做
标量乘，用于 TLS 1.3、WireGuard、Signal、age、SSH、Tor 的密钥协商。算法：RFC 7748 §5
的 Montgomery ladder，常数时间条件交换防标量计时泄漏；域算术基于
`Std.Numerics.BigInt`（ModInverse / Add / Multiply / Mod）。

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

[`src/libraries/z42.crypto/src/X25519.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L22)

按 RFC 7748 §5 clamp 32 字节标量，返回新的 clamp 副本（不改入参）。

清 byte0 低 3 位、清 byte31 最高位、置 byte31 次高位。

<a id="Std.Crypto.X25519.ScalarMult(byte[],byte[])"></a>
### static byte[] ScalarMult(byte[] scalar, byte[] point)

[`src/libraries/z42.crypto/src/X25519.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L39)

X25519 标量乘：`scalar × point`（各 32 字节小端编码）。

`scalar` 内部自动 clamp（RFC 7748 §5），调用方不必预 clamp。

<a id="Std.Crypto.X25519.ScalarMultBase(byte[])"></a>
### static byte[] ScalarMultBase(byte[] scalar)

[`src/libraries/z42.crypto/src/X25519.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.crypto/src/X25519.z42#L62)

便捷形式：`scalar × 基点`（u=9），即由私钥派生公钥。

