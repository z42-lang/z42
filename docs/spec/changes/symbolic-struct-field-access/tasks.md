# tasks — P2：struct 字段访问符号化（方案 A）

> 裁决：**A = `(root 类型名, 变长索引路径)`**（User 2026-09-28）。先不做 memo。
> 格式：zbc **1.45 → 1.46** / zpkg **0.50 → 0.51**。
> ⚠️ `z42.package` 已从 `src/libraries/` 搬到 **`src/compiler/z42.package/`**（另一会话的 PR）。

## wire（目标形态）

```
StructFieldGetPrim : op + tag(dst) + dst | base:u16 | root_type:u32(池) | depth:u8 | idx:u16×depth | kind:u8
StructFieldSetPrim : op + tag(val) + NoReg | base:u16 | root_type:u32(池) | depth:u8 | idx:u16×depth | kind:u8 | val:u16
```

`depth >= 1`；实测最大深度 **4**（见 proposal 的分布表）⇒ u8 绰绰有余。

## 解析（VM）

```
off = 0; cur = root_type
i = 0：cur 是 class  ⇒ off += composed_object_layout().field_offsets[idx[0]]
       cur 是 struct ⇒ off += struct_layout().field_offset(idx[0])
i >= 1：恒走 struct 分支（编译器已在非内联处断链 ⇒ 每节真内联）
每跳之后 cur = TypeDesc.fields[idx[i]].type_tag
```

⭐ **root 类型名本身就是编号空间的判别器** —— 这是 A 相对 B 的决定性优势。

## 清单

- [x] **T1 平行性检查（先做，它是 A 的前提）** ✅ 2026-09-28 —— `struct_field_table` 声称「同序平行于 Fields」，
      而路径的下一跳类型名取自 `TypeDesc.fields[idx].type_tag`。**注释的声明必须验**：
      载入期断言 `struct_layout().field_count() == fields.len()`（表在场时），不等即响。
- [x] **T2 格式常量 + 真相表 + changelog + 钉值单测**
      `ZbcFormat.z42` 1.46 · `ZpkgWriter.z42` 0.51 · `zbc_reader/versions.rs` ×2 ·
      `version-bumping.md` 四行表（**#922 的门会盯**）· `zbc.md` / `zpkg.md` changelog ·
      `zbc_reader_tests.rs` 的 `zbc_version_constants_pinned` / `zpkg_version_constants_pinned`
- [x] **T3 IR 类**（`src/compiler/z42.package/src/IrInstrObject.z42`）
      `StructFieldGetPrimInstr` / `StructFieldSetPrimInstr`：`int ByteOff` → `string RootType` + `int[] Path`；
      `Dump()` / `Clone()` / `ReadAt` / `SetReadAt` 同步。⚠️ 还要过 `IrEscapeAnalysis.z42:156,161` 的模式匹配。
- [x] **T4 写端编码**（`BinaryFormat/ZbcInstr.z42`）
- [x] **T5 两端解码**（z42 `ZbcReaderInstr.z42` + Rust `metadata/zbc_reader/instr_decode.rs` +
      `metadata/bytecode` 的 `Instruction` 变体）
- [x] **T6 interp**（`interp/exec_struct.rs`）：`resolve_field_path` + **kind 对账**
      （`root_type` 的 kind 必须与运行期 `Base` 的 `Value` 变体一致 —— A 白送的那条）
- [x] **T7 JIT**（`jit/translate/structs.rs` + `jit/helpers/struct_ops.rs`）
- [x] **T8 编译器 10 个 emit 点**：`_structChainOffset` 从「累加 int」改成「累积序号序列」；
      `_copyRegion` / `_emitLeafEqChecks` 的递归同步；扁平 4 站点直接给 depth=1
- [ ] **T9 fixture 重生**：6 个 zbc-format + 4 个 zpkg-format + golden hex 单测
- [x] **T10 文档**：`struct-value-semantics.md`（四条指令表 + 符号化节）· `zbc.md` · `zpkg.md`
- [ ] **T11 GREEN**：`cargo test --lib`（不带过滤）· 5 个 feature 组合 ·
      `xtask test`（e2e/compiler/stdlib）· `cargo test --test format_fixture_versions` ·
      `xtask test bootstrap` 边界检查
- [ ] **T12 纯隔离开销实测**，对照门槛（建议线：真实编译负载 <2% / 字段饱和 micro <6%，**User 未确认**）

## 风险与已知坑

- 🔴 **`-1` 哨兵那一类由本刀顺带消掉**（烘序号 ⇒ 无偏移算术）。见 proposal 的可达性实测节。
- 🔴 **动态权重未测**（85.1% 是**静态**发射占比）⇒ T12 必须做。
- ⚠️ **stdlib 一条 `StructFieldGetPrim` 都不发** ⇒ 覆盖面只能算 e2e 语料，别把 stdlib 全绿当覆盖。
- ⚠️ IR 优化器录入必须全（`IrOptInfo` 的 `DstId`/`AddReads`/`ReplaceReads`/`SetDst` + 逃逸汇点表）——
  历史上漏 `StructFieldSetPrim` 的 `Val` 读导致 DCE 误删喂值的 `const`。
- ⚠️ 本机 `RUSTUP_TOOLCHAIN=1.98.1`；改 z42c 要 `build compiler` **且** `build sdk`。

## T1 结果（2026-09-28）

载入期加 `debug_assert!`（表在场时 `struct_field_table.len() == fields.len()`）。
政策同 `__box_prim` / `prim_value_mismatch`：只有编译器/写端能违反 ⇒ **debug 响、release 放行**。

| 验证 | 结果 |
|---|---|
| `cargo test --lib`（debug） | 34/34 loader 测试绿 |
| **正面对照**（故意造不平行） | ✅ `should_panic` —— 门会响 |
| e2e 语料（debug VM × 289 条） | **零响** |
| **反向对照**（表在场即响） | **289 / 289** ⇒ 每条程序都走到了这条路 ⇒ 零响是实的 |

⭐ **顺带挖出一个夹具真实性缺口**：#915 的 `module_with_struct_field_table` 造的是
「有字段表、但 `fields` 为空」的模块 —— **真实 zbc 里不可能出现的形状**。
也就是说 #915 那个「从 zbc 到 layout 逐格一致」的测试，当初验的是一张**没有平行字段列表**的表。
在 P0 里无所谓（表休眠），接通 P2 后那正是正确性前提。已把夹具修成真实形态。

⇒ 与 #915 当场挖出「冷区裁剪漏守 `struct_layout`」是同一教训的第二次应验：
**休眠元数据的测试，连它的夹具形状都得是真实的。**
