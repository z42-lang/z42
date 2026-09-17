# z42.crypto —— 加密原语

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.crypto/`；命名空间 `Std.Crypto`

摘要、MAC、密钥派生、对称加密与 AEAD、公钥签名与密钥协商、OS CSPRNG。全部是静态类，
按算法一个类，不需要实例化（RSA 的密钥对象除外）。

所有算法都是**纯 z42 脚本实现**，不链接 OpenSSL / libcrypto。唯一走 OS 的是
`SecureRandom` 的熵源。三条使用前必读：

> ⚠️ **性能量级**：纯脚本 + 解释执行。摘要 / HMAC / AEAD 对小载荷（token、信封、单条消息）
> 足够；大块数据加密、批量签名会明显慢。RSA-2048 签名约数秒级，X25519 单次标量乘约百毫秒级。
>
> ⚠️ **侧信道防护不完整**：AEAD 的 tag 比较与 X25519 的 Montgomery ladder 做了常数时间处理，
> `ConstantTime.Equals` 也公开给用户用；但 RSA **没有 blinding**，ECDSA 的验签侧标量乘不是
> 常数时间。攻击者能精确计时同一台机器上的私钥运算时，不要依赖本包。
>
> ⚠️ **`Md5` / `Sha1` 只用于协议强制的旧系统互操作**，新设计一律用 SHA-256 及以上。

参数校验失败、tag 校验失败、padding 非法都抛 `Std.ArgumentException`。

## 摘要（hash）

每个摘要类都提供同一组四个入口（`Sha3` / `Blake2b` 见各自小节的额外形态）：

```z42
public static byte[] Hash(byte[] data)          // 原始 digest
public static byte[] HashString(string s)       // UTF-8 编码后再 Hash
public static string HashHex(byte[] data)       // 小写 hex
public static string HashStringHex(string s)    // UTF-8 + Hash + 小写 hex
```

| 类 | 标准 | 输出 | 说明 |
|---|---|---|---|
| `Sha256` | FIPS 180-4 | 32 B | 默认选它 |
| `Sha512` | FIPS 180-4 | 64 B | |
| `Sha384` | FIPS 180-4 | 48 B | |
| `Sha1` | FIPS 180-4 | 20 B | ⚠️ 碰撞已破（SHAttered 2017）。仅用于 git 兼容、`Sec-WebSocket-Accept`、HMAC-SHA1 等既有协议 |
| `Md5` | RFC 1321 | 16 B | ⚠️ 碰撞已破。仅用于 HTTP Digest、ETag、CRAM-MD5、BitTorrent v1 等强制 MD5 的格式 |
| `Blake2s` | RFC 7693 §B | 32 B | 32-bit 字，适合 32 位环境 |
| `Blake3` | BLAKE3 | 32 B | `Hash256` 是 `Hash` 的同义方法 |

### Blake2b

```z42
public static class Blake2b {
    public static byte[] Hash(byte[] data)                 // 64 B
    public static byte[] HashString(string s)
    public static string HashHex(byte[] data)
    public static string HashStringHex(string s)

    public static byte[] Hash256(byte[] data)              // 32 B
    public static byte[] Hash256String(string s)
    public static string Hash256Hex(byte[] data)
    public static string Hash256StringHex(string s)

    public static byte[] HashLen(byte[] data, byte[] key, int outLen)
}
```

`HashLen` 是变长 + keyed（MAC）模式：`outLen` ∈ `[1, 64]`，`key.Length` ∈ `[0, 64]`，
不加 key 就传 `new byte[0]`。输出长度与 key 长度都参与初始化，**同一输入换个 `outLen`
得到的是完全不相关的摘要，不是截断**。

`Blake2s.HashLen(byte[] data, byte[] key, int outLen)` 同形，上限都是 32。

### Blake3 的 XOF

```z42
public static byte[] HashLen(byte[] data, int outLen)   // outLen ≥ 0
```

BLAKE3 是可扩展输出函数：`HashLen` 的前 N 字节与更长输出的前 N 字节**一致**（与 BLAKE2 不同），
所以 `HashLen(data, 8)` 就是 `Hash(data)` 的前 8 字节。

### Sha3（FIPS 202）

```z42
public static class Sha3 {
    // SHA-3，域分隔字节 0x06
    public static byte[] Hash224(byte[] data)   // 28 B
    public static byte[] Hash256(byte[] data)   // 32 B
    public static byte[] Hash384(byte[] data)   // 48 B
    public static byte[] Hash512(byte[] data)   // 64 B
    // 每个都有 Hash<N>String / Hash<N>Hex / Hash<N>StringHex 三个姊妹形态

    // 前 FIPS 的 Keccak，域分隔字节 0x01
    public static byte[] KeccakLegacy256(byte[] data)
    public static byte[] KeccakLegacy512(byte[] data)
    // 同样有 String / Hex / StringHex 形态

    // SHAKE 可扩展输出（FIPS 202 §6.2）
    public static byte[] Shake128(byte[] data, int outputLen)
    public static byte[] Shake256(byte[] data, int outputLen)
    // 同样有 String / Hex / StringHex 形态
}
```

`KeccakLegacy*` 与 `Hash*` 对同一输入产生**不同**的结果，这是设计使然：以太坊地址派生、
Solidity 的 `keccak256(bytes)` 要用 `KeccakLegacy256`，FIPS 202 场景用 `Hash256`。
`Shake*` 的 `outputLen` 必须 ≥ 0，可任意长。

## MAC

```z42
public static byte[] Compute(byte[] key, byte[] message)
public static byte[] ComputeString(string key, string message)
public static string ComputeHex(byte[] key, byte[] message)
public static string ComputeStringHex(string key, string message)
```

| 类 | 底层摘要 | tag 长度 |
|---|---|---|
| `HmacSha256` | SHA-256 | 32 B |
| `HmacSha512` | SHA-512 | 64 B |
| `HmacSha384` | SHA-384 | 48 B |
| `HmacSha1` | SHA-1 | 20 B |
| `HmacMd5` | MD5 | 16 B |

HMAC 构造本身不受底层摘要碰撞的影响，所以 `HmacSha1`（TOTP RFC 6238 默认）与 `HmacMd5`
（Digest-MD5 / CRAM-MD5）在协议要求时仍可用。

### Poly1305（RFC 8439 §2.5）

```z42
public static class Poly1305 {
    public static byte[] Mac(byte[] key, byte[] message)     // key 32 B → tag 16 B
    public static string MacHex(byte[] key, byte[] message)
}
```

> ⚠️ **一次性 MAC**：32 字节 key 对每条消息必须全新。key 复用会直接泄漏多项式系数、彻底失去
> 安全性。除非你在实现协议，否则请用 `ChaCha20Poly1305`——它逐消息派生 Poly1305 key。

key 不是 32 字节抛 `ArgumentException`。

## 密钥派生

### Pbkdf2（RFC 8018 §5.2，HMAC-SHA-256）

```z42
public static class Pbkdf2 {
    public static byte[] DeriveKey(byte[] password, byte[] salt, int iterations, int dkLen)
    public static byte[] DeriveKeyString(string password, string salt, int iterations, int dkLen)
    public static string DeriveKeyHex(byte[] password, byte[] salt, int iterations, int dkLen)
    public static string DeriveKeyStringHex(string password, string salt, int iterations, int dkLen)
}
```

`iterations < 1` 或 `dkLen < 1` 抛 `ArgumentException`。只有 HMAC-SHA-256 一种 PRF。

> ⚠️ 量级参考：`iterations = 100000` / `dkLen = 32` 的一次派生在解释执行下要**十秒量级**。
> 这不是可以放进请求路径的开销，选迭代次数时请按实测定。

### Hkdf（RFC 5869）

`HkdfSha256` / `HkdfSha512` / `HkdfSha1` 三个类，接口相同：

```z42
public static byte[] Derive(byte[] salt, byte[] ikm, byte[] info, int length)   // Extract + Expand
public static byte[] Extract(byte[] salt, byte[] ikm)                           // → PRK，HashLen 字节
public static byte[] Expand(byte[] prk, byte[] info, int length)
public static string DeriveHex(byte[] salt, byte[] ikm, byte[] info, int length)
```

| 类 | HashLen | `length` 上限（255 × HashLen） |
|---|---|---|
| `HkdfSha256` | 32 | 8160 |
| `HkdfSha512` | 64 | 16320 |
| `HkdfSha1` | 20 | 5100 |

`salt` 为 `null` 或空数组时按 RFC §2.2 替换成 HashLen 个零字节；`info` 为 `null` 按空处理。
`length` 越界抛 `ArgumentException`。

### Scrypt（RFC 7914）

```z42
public static class Scrypt {
    public static byte[] Derive(byte[] password, byte[] salt, int n, int r, int p, int dkLen)
}
```

内存硬的密码哈希。约束：`n` 必须是 ≥ 2 的 2 的幂、`r ≥ 1`、`p ≥ 1`、`r * p < 2^30`、
`dkLen ≥ 0`，违反抛 `ArgumentException`。

> ⚠️ 纯脚本执行下，生产级参数（`N ≥ 16384`）的一次调用会非常慢。当前实现适合校验小参数向量与
> 低频派生，不适合每请求一次的密码校验。

## 对称加密与 AEAD

### Aes（FIPS 197）

```z42
public static class Aes {
    public static byte[] EncryptBlock(byte[] key, byte[] plaintext)     // 单块 16 B
    public static byte[] DecryptBlock(byte[] key, byte[] ciphertext)

    public static byte[] EncryptCtr(byte[] key, byte[] nonce, byte[] data)   // nonce 8 B
    public static byte[] DecryptCtr(byte[] key, byte[] nonce, byte[] data)

    public static byte[] EncryptCbcPkcs7(byte[] key, byte[] iv, byte[] data) // iv 16 B
    public static byte[] DecryptCbcPkcs7(byte[] key, byte[] iv, byte[] data)

    public static byte[] EncryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] plaintext)
    public static byte[] DecryptGcm(byte[] key, byte[] iv, byte[] aad, byte[] ctAndTag)

    public static byte[] EncryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext, int tagLen)
    public static byte[] DecryptCcm(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag, int tagLen)
}
```

`key` 长度决定变体：16 B = AES-128，24 B = AES-192，32 B = AES-256；其它长度抛
`ArgumentException`。

| 模式 | 约束与输出 |
|---|---|
| `*Block` | 明文 / 密文必须正好 16 字节。**ECB 单块原语，不要直接用来加密多块数据** |
| `*Ctr` | `nonce` 必须 8 字节；计数器 8 字节大端从 0 起。输出与输入等长。**无认证** |
| `*CbcPkcs7` | `iv` 必须 16 字节。输出总是 16 的正整数倍；输入已对齐时也会追加一整块 padding。解密校验并剥离 PKCS#7，padding 非法抛 `ArgumentException`。**无认证** |
| `*Gcm` | `iv` 任意非空长度（12 字节走快路径，其它长度按 NIST SP 800-38D §7.1 折叠）。输出 = `ciphertext ‖ 16 字节 tag`。tag 固定 16 字节，不可截断 |
| `*Ccm` | `nonce` 7–13 字节（长度决定载荷上限 `2^(8*(15-nonceLen)) - 1`）；`tagLen` ∈ `{4,6,8,10,12,14,16}`。输出 = `ciphertext ‖ tagLen 字节 tag` |

`DecryptGcm` / `DecryptCcm` 的 tag 比较是常数时间的；不匹配时抛 `ArgumentException`
（消息里写明「ciphertext or AAD tampered」）。**这个异常必须当作篡改信号处理，不要吞掉。**

### ChaCha20（RFC 8439）

```z42
public static class ChaCha20 {
    public static byte[] Encrypt(byte[] key, byte[] nonce, byte[] data)               // counter 从 1 起
    public static byte[] Decrypt(byte[] key, byte[] nonce, byte[] data)               // 与 Encrypt 同义
    public static byte[] Crypt(byte[] key, byte[] nonce, int counter, byte[] data)    // 显式起始 counter
    public static byte[] Block(byte[] key, byte[] nonce, int counter)                 // 单个 64 B 密钥流块
}
```

`key` 必须 32 字节，`nonce` 必须 12 字节，否则抛 `ArgumentException`。流密码，**无认证**——
需要完整性保护请用下面的 AEAD。

### ChaCha20Poly1305（RFC 8439 §2.8）

```z42
public static class ChaCha20Poly1305 {
    public static byte[] Encrypt(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext)
    public static byte[] Decrypt(byte[] key, byte[] nonce, byte[] aad, byte[] ctAndTag)
}
```

`key` 32 字节、`nonce` 12 字节。输出 = `ciphertext ‖ 16 字节 tag`。`Decrypt` 常数时间校验 tag，
失败抛 `ArgumentException`。同一 key 下 nonce 不可复用。

## 公钥

### Ed25519（RFC 8032）

```z42
public static class Ed25519 {
    public static byte[] GeneratePublicKey(byte[] secretKey)                       // 32 B → 32 B
    public static byte[] Sign(byte[] secretKey, byte[] message)                    // → 64 B（R ‖ S）
    public static bool   Verify(byte[] publicKey, byte[] message, byte[] signature)
}
```

`Verify` 对**签名内容错误**返回 `false`，只在长度不对时抛 `ArgumentException`。

### X25519（RFC 7748）

```z42
public static class X25519 {
    public static int U_BASE = 9;                                 // 基点 u 坐标

    public static byte[] Clamp(byte[] scalar)                     // 32 B → 32 B，返回新数组
    public static byte[] ScalarMult(byte[] scalar, byte[] point)  // 32 B × 32 B → 32 B
    public static byte[] ScalarMultBase(byte[] scalar)            // scalar × 基点，即公钥
}
```

`ScalarMult` **内部会自动 clamp**，调用方不必预先处理；`Clamp` 公开出来只是给需要显式形态的
协议实现用，它不修改入参。所有参数必须是 32 字节。

> ⚠️ `U_BASE` 是可写的 `public static` 字段，不是常量——不要给它赋值，会改变本进程内
> `ScalarMultBase` 的行为。

### EcdsaP256（FIPS 186-4 + RFC 6979）与 EcdsaSecp256k1

两个类接口完全相同，只是曲线不同：

```z42
public static byte[] GeneratePublicKey(byte[] privateScalar)   // 32 B → 64 B（X ‖ Y，未压缩）
public static byte[] Sign(byte[] privateScalar, byte[] message)// → 64 B（r ‖ s，各 32 B 大端）
public static bool   Verify(byte[] publicKey, byte[] message, byte[] signature)
```

- 签名 nonce 走 RFC 6979 确定性派生（HMAC-SHA-256），所以**同一私钥 + 同一消息的签名逐位相同**，
  不依赖随机数质量。
- `Verify` 对无效签名、非曲线上的点、越界的 `r` / `s` 返回 `false`，只在公钥长度不是 64 字节时
  抛 `ArgumentException`。
- `EcdsaP256` 用于 JWT ES256 / x509 / TLS；`EcdsaSecp256k1` 用于比特币与以太坊。两条曲线的签名
  互不通过对方的验签。

### Rsa（RFC 8017 / PKCS#1 v2.2）

```z42
public class RsaPublicKey {
    public BigInt n;
    public BigInt e;
    public RsaPublicKey(BigInt n, BigInt e)
}

public class RsaPrivateKey {
    public BigInt n;
    public BigInt e;
    public BigInt d;
    public RsaPrivateKey(BigInt n, BigInt e, BigInt d)
    public RsaPublicKey GetPublicKey()
}

public static class Rsa {
    public static byte[] SignPkcs1v15Sha256(RsaPrivateKey privateKey, byte[] message)
    public static bool   VerifyPkcs1v15Sha256(RsaPublicKey publicKey, byte[] message, byte[] signature)

    public static byte[] SignPssSha256(RsaPrivateKey privateKey, byte[] message, byte[] salt)
    public static bool   VerifyPssSha256(RsaPublicKey publicKey, byte[] message, byte[] signature, int saltLen)

    public static byte[] EncryptOaepSha256(RsaPublicKey publicKey, byte[] message, byte[] label, byte[] seed)
    public static byte[] DecryptOaepSha256(RsaPrivateKey privateKey, byte[] ciphertext, byte[] label)

    public static byte[] RawPublicOp(RsaPublicKey publicKey, byte[] message)
    public static byte[] RawPrivateOp(RsaPrivateKey privateKey, byte[] ciphertext)
}
```

密钥用 `Std.Numerics.BigInt` 的原始 `n` / `e` / `d` 构造，字段是公开可写的。

- **PKCS#1 v1.5**：确定性签名；验签做常数时间比较，签名无效时返回 `false` 而不抛异常。
- **PSS**：`salt` 由调用方提供——传空数组得到确定性签名，传 32 字节随机数得到概率性签名；
  验签的 `saltLen` 必须与签名时一致。
- **OAEP**：`label` 一般传空数组，`seed` 是 32 字节随机数（测试时可传固定值）。
- `RawPublicOp` / `RawPrivateOp` 是**无 padding** 的裸模幂，只用于协议互操作。

> ⚠️ `DecryptOaepSha256` 对不同的 padding 失败抛出**不同消息**的 `ArgumentException`
> （`lHash mismatch` / `malformed PS` / `missing 0x01 separator` / `Y != 0`）。把这些消息
> 原样回传给远端会构成 padding oracle。对外只暴露一个笼统的「解密失败」。

## 随机与比较

### SecureRandom

```z42
public static class SecureRandom {
    public static byte[] GetBytes(int n)             // n ≥ 0
    public static int    NextInt()                   // 整个 i32 范围
    public static long   NextLong()                  // 整个 i64 范围
    public static int    NextU32Bounded(int bound)   // [0, bound)，bound > 0
}
```

熵来自 OS：Linux `getrandom(2)`、macOS `getentropy`、Windows `BCryptGenRandom`。
**wasm32 上抛 `Std.NotSupportedException`。**

`NextU32Bounded` 用 rejection sampling，没有模偏差。`n < 0` 或 `bound <= 0` 抛
`ArgumentException`。

> session token、CSRF nonce、KDF salt、密钥、口令一律用这个，**不要用
> `Std.Random`**（见 [random](random.md)，那是确定性 PRNG）。

### ConstantTime

```z42
public static class ConstantTime {
    public static bool Equals(byte[] a, byte[] b)
}
```

比较两个字节数组而不提前返回：长度不同立即返回 `false`（只泄漏长度），长度相同时**总是**遍历
每一个字节。比较 MAC、认证令牌、密码哈希时必须用它，不要用 `==` 或带 early-out 的手写循环
——后者会通过耗时泄漏「前几个字节匹配」，足以逐字节伪造。

## 用法

```z42
using Std;
using Std.IO;
using Std.Crypto;
using Std.Encoding;

void Main() {
    // 摘要
    Console.WriteLine(Sha256.HashStringHex("abc"));
    // ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad

    // HMAC
    Console.WriteLine(HmacSha256.ComputeStringHex("key", "msg"));

    // 口令派生（10 万次迭代在解释执行下要十秒量级）
    byte[] salt = SecureRandom.GetBytes(16);
    byte[] dk = Pbkdf2.DeriveKey(Utf8.GetBytes("password"), salt, 100000, 32);

    // AEAD：一次性 nonce + 认证
    byte[] key = SecureRandom.GetBytes(32);
    byte[] nonce = SecureRandom.GetBytes(12);
    byte[] sealed_ = ChaCha20Poly1305.Encrypt(key, nonce, new byte[0], Utf8.GetBytes("hello"));
    Console.WriteLine(Utf8.GetString(
        ChaCha20Poly1305.Decrypt(key, nonce, new byte[0], sealed_)));   // hello

    // 篡改必须当错误处理
    sealed_[0] = (byte)((int)sealed_[0] ^ 1);
    try {
        ChaCha20Poly1305.Decrypt(key, nonce, new byte[0], sealed_);
    } catch (ArgumentException e) {
        Console.WriteLine("tampered");
    }

    // 签名：RFC 6979 确定性
    byte[] priv = SecureRandom.GetBytes(32);
    byte[] pub = EcdsaP256.GeneratePublicKey(priv);
    byte[] sig = EcdsaP256.Sign(priv, Utf8.GetBytes("sample"));
    Console.WriteLine(EcdsaP256.Verify(pub, Utf8.GetBytes("sample"), sig));   // true

    // 比较 secret 一律走常数时间
    Console.WriteLine(ConstantTime.Equals(dk, dk));                          // true
}
```

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
