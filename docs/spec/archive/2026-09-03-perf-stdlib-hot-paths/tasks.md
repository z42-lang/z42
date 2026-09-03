# Tasks: 标准库热路径（perf-stdlib-hot-paths）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：perf（stdlib + runtime builtin）
**变更说明：** ① AES `_sbox()`/`_invSbox()` 静态化（此前每轮 SubBytes / 每次密钥扩展 `new int[256]` + 256 次赋值）；
② 新增 VM builtin `__str_substring`（str_meta char→byte 表 + 一次 slice 拷贝）与 `__str_concat_parts`（`string[]` 前
count 项一次拼接），`String.Substring` / `StringBuilder.ToString` 从逐字符 `CharAt` + `FromChars` 改走 bulk；
③ `List.RemoveAt/Clear` 与 `Dictionary.Remove/Clear` 清掉被移除槽的引用（镜像 C# `Array.Clear`），消除 GC 可达性滞留；
④ crypto bench 补 AES CBC/GCM 4 KB 往返基准。
**原因：** 三面评审 L-1 / L-2 / L-5。Script-First 允许经量化的「升级阶梯」把逐字符 builtin 派发的原语提为原生 bulk；
`__str_to_chars` 先例（IndexOf 8.6×）。
**文档影响：** `src/runtime/src/corelib/README.md`（功能索引：新 builtin）；`src/libraries/z42.core/README.md`（String 行）。

## 进度概览
- [x] 1. AES 静态表；List/Dictionary 清引用；String/StringBuilder bulk；Rust builtin + 注册
- [x] 2. 对比数据：`xtask bench stdlib z42.core / z42.text / z42.crypto`（micro，base = 改前工具链）→ `bench --micro-diff`
- [x] 3. `cargo test --lib`（native_decl 对账）+ `xtask test` GREEN（含 JIT e2e / stdlib）
- [x] 4. 文档同步 + 归档

## 对比数据（2026-09-03，macOS arm64 同机；`xtask bench stdlib <lib> --mode interp`，Bencher 中位数）
base = main 406c7ad9 编译器 + 改前 stdlib + main z42vm；pr = 同编译器 + 本分支 stdlib + 本分支 z42vm（含新 builtin）。

| 基准 | base | pr | 加速 |
|---|---|---|---|
| z42.core.string_substring（20 次 Substring(i,8)）| 33.1 µs | 7.9 µs | **4.18×** |
| z42.core.string_split（16 段）| 19.0 µs | 12.6 µs | **1.51×** |
| z42.core.string_join | 2.3 µs | 1.9 µs | 1.19× |
| z42.text.stringbuilder_append（128×Append + ToString）| 137.0 µs | 79.5 µs | **1.72×** |
| z42.crypto.aes_gcm_roundtrip_4k | 78.58 ms | 40.41 ms | **1.94×** |
| z42.crypto.aes_cbc_roundtrip_4k | 175.60 ms | 136.67 ms | **1.28×** |
| 其余 21 项（concat / index_of / list / dict / format / sha256 / blake3 / …）| — | — | 0.94–1.03×（噪声内）|

备注：List/Dictionary 清引用是 GC 可达性修正，不在时间基准里体现（`list_add_index` / `dict_set_get` 1.00× / 0.96×，
无回归）。首轮测量踩到两个坑（记入 memory）：warm 树改 z42.core 导出面后 `build stdlib` 首遍用旧导出面（z42.text
E0401）、静态字段初始化器不能调 private 静态方法（E0404）——修正后重建重测得上表。
