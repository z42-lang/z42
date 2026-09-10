# Tasks: 分代模式的 CI 覆盖

> 状态：🟢 已完成（2026-09-10）

| 阶段 | 状态 |
|---|---|
| 1. 试 golden 套件 → **空转**，换编译器自建 | ✅ |
| 2. 反证出 nursery 该定 16M | ✅ |
| 3. 顺手修三处漏掉的数组写屏障 + 回归测试 | ✅ |
| 4. GREEN + 文档 + 归档 | ✅ |
| 5. 🔴 **1M 下的 known-open 缺陷** | ⬜ **未修，已留精确复现** |

## 阶段 1: 选对负载

- [x] 先做「golden 套件 + 分代 + 1M nursery」（16 s）
- [x] **反证发现它是空转的**：放回 #537 的缺陷，整套 golden 仍全绿 ——
      golden 都是短命小程序，一次 minor 都不做
- [x] 换成**编译器自建**（#537/#539 当年就是以「编 z42c.semantics 编到一半崩」暴露的）

## 阶段 2: 定 nursery

- [x] 32M（默认）→ 10 次 minor → 放回 #539 的缺陷仍绿：**抓不到**
- [x] **16M → 26 次 minor → 放回 #539 的缺陷变红**（`ArrayGet: expected array, got Null`），
      干净树上绿：**选它**
- [x] 1M → 干净树上就红（见阶段 5）

## 阶段 3: 三处漏掉的数组写屏障

- [x] `Array.SetValue` / `Array.Copy` / 经 `ref` 写数组元素 —— 三处都往数组里写堆引用
      却不发屏障，而 `ArraySet` 与 JIT 的数组存储 helper 一直都发
- [x] `barrier_copied_range`：卡键在数组头的条目上，与元素下标无关，所以只扫一遍被写区间
- [x] 三个回归测试，**都验证过「不打补丁就红」**

## 阶段 4: GREEN / 文档

- [x] `./xtask test` 全绿；新 stage 7.1 s（gate 2m50s → 2m54s）
- [x] `docs/book/src/dev/test-gate.md`（stage 清单有门对账，代码与文档必须同改）
- [x] `docs/book/src/runtime/gc-tuning-and-safepoint.md`
- [x] 归档

## 阶段 5: 🔴 known-open —— 1M nursery 下仍会丢对象

```bash
env Z42_GC_MODE=generational Z42_GC_NURSERY_BYTES=1M \
  artifacts/build/runtime/release/z42vm artifacts/.z42/programs/z42c/z42c.driver.zpkg \
  -- build src/compiler/z42c.semantics/z42c.semantics.z42.toml --release --no-incremental
# → 96 次 minor 之后：__str_hash_code: arg 0 expected string, got Null   （3/3 必现）
```

- **早于 #552 / #553**（`git checkout 3935d8f1 -- src/runtime` 对照，症状与 minor 次数逐字相同）
- 2M 及以上全绿
- 补完三处屏障后**症状不变** → **不是漏发屏障**
- 探针形状与「下一步该怎么查」见 design.md「未修完的那一个」
- ⚠️ **改 `generational.rs` 加探针时注意只替换函数体** —— 这次把模块头注释一起替换掉，
  编译失败，白跑一轮

## 交给后续 change 的发现

1. 🔴 上面那个 known-open 缺陷 —— **翻 `Z42_GC_MODE` 默认的前置**
2. `purge_blocks` 的 retain 7–10 ms —— `all_blocks` 改按 chunk 分桶
3. 「不把年轻的脏卡条目当根」—— 语义改动，需单独立项

## 验收标准

- gate 含一个分代真实负载 stage，且**反证过它抓得到跨代缺陷**
- 所有写数组元素的路径都发屏障，且纯基元拷贝不置脏
- known-open 缺陷有精确复现、有形状、有下一步
