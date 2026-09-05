# Spec: zpkg 内部内容标识（BLID build_id + 源变更检测哈希）

## MODIFIED Requirements

### Requirement: BLID build_id 的算法

**Before:** BLID 的 16 字节 payload = `BLAKE3-128(主 zpkg 字节，BLID payload 归零)`。
**After:**  BLID 的 16 字节 payload = `MurmurHash3_x86_128(主 zpkg 字节，BLID payload 归零)`。

段布局不变：BLID 恒为最后一段、payload 恒 16 字节；写入时机不变（先写零占位、装配后回填）；
sidecar 仍写同一 build id。**不 bump zpkg/zbc 格式版本**（布局未变，reader 解析路径不变）。

#### Scenario: 主包与 sidecar 配对
- **WHEN** release strip 模式产出 `main.zpkg` + `main.zsym`
- **THEN** 两者末尾 BLID 的 16 字节完全相同，runtime 据此配对成功

#### Scenario: 内容不同 → build_id 不同
- **WHEN** 两次构建的主 zpkg 字节有任何差异（BLID 占位区之外）
- **THEN** 回填的 build_id 不同

#### Scenario: runtime 不重算
- **WHEN** runtime 装载主包 + sidecar
- **THEN** 只比较两个 BLID 值是否相等，不对任何字节重新哈希
  （`metadata::build_id::compute` 在 runtime 中无调用点）

### Requirement: 源变更检测哈希的算法与形态

**Before:** zpkg MODS 每模块的 `hash` 字段 = `"sha256:" + lowercase_hex(SHA-256(源文本))`。
**After:**  = `"mmh3:" + lowercase_hex(MurmurHash3_x86_128(UTF-8(源文本)))`，共 5 + 32 个字符。

#### Scenario: 形态
- **WHEN** 对源文本 `"abc"` 调 `ZpkgBuilder.SourceHashHex`
- **THEN** 返回 `"mmh3:d1c6cd75a506b0a2a506b0a2a506b0a2"`

#### Scenario: 文件未变 → 跳过重编
- **WHEN** 增量构建时某 `.z42` 的当前哈希与 cache/zpkg 中记录的相等
- **THEN** 该文件不重新编译

#### Scenario: 跨算法版本的产物 → 全量重编
- **WHEN** cache 中记录的是旧形态 `"sha256:…"`，当前算出的是 `"mmh3:…"`
- **THEN** 两者不等 ⇒ 判定为「已变更」⇒ 全量重编（无需任何迁移代码）

### Requirement: 密码学入口保持可用

`ZpkgBuilder.Sha256Hex` 保留原语义（`"sha256:" + lowercase_hex(SHA-256(text))`），
`Std.Crypto` 的 `Sha256` / `Blake3` 实现不变。

#### Scenario: Sha256Hex 仍是 SHA-256
- **WHEN** 调 `ZpkgBuilder.Sha256Hex("abc")`
- **THEN** 返回 `"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"`

## Out of Scope（显式不变更）

### Requirement: 散装 zbc 内容哈希不变

indexed 包 `FILE` 段的 `zbc_hash` 仍是 **BLAKE3-128** —— Rust `loader/artifact.rs` 装载时用
`blake3::hash` **重算校验**，是真的跨语言契约。

#### Scenario: 散装 zbc 校验
- **WHEN** runtime 装载 indexed 包的散装 `.zbc`
- **THEN** 用 BLAKE3 重算内容哈希并与 `zbc_hash` 比对，不匹配则拒绝装载

## Pipeline Steps

- [ ] Lexer — 不涉及
- [ ] Parser / AST — 不涉及
- [ ] TypeChecker — 不涉及
- [x] IR Codegen — zpkg 写入端（`ZpkgWriter` BLID / `ZpkgBuilder` 源哈希）
- [ ] VM interp — 不涉及（runtime 对两个字段的处理方式均未变）
