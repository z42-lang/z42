//! loom 模型：PIC entry 的 (TypeId, 载荷) 发布协议。
//!
//! 对应 `docs/spec/changes/fix-field-ic-publication-race`。
//!
//! ## 为什么要 loom
//!
//! 这个竞态是**内存序**层面的：两个独立的 `Relaxed` 原子量，读者可以看到
//! 「新 TypeId + 还没写入的载荷」。在 x86 上几乎不可能复现（TSO 保住了存储顺序）；
//! 在 Apple Silicon 上实测 2400 次进程冷启动命中 67 次，**但那是概率**。
//! loom 穷举线程交错 + C11 内存模型，让两种协议的差别变成**确定性**结果。
//!
//! ## 两个模型
//!
//! | 模型 | 协议 | 期望 |
//! |---|---|---|
//! | `packed_publication_never_tears` | 单个 `AtomicU64`（**现行实现**） | 绿：命中的载荷永远配对 |
//! | `split_publication_tears`（阴性对照，`#[ignore]`） | 两个独立 `AtomicU32`（**修复前**） | 红：loom 找得到「新 TypeId + 旧载荷」的交错 |
//!
//! 阴性对照带 `#[ignore]`，因为它断言的是**旧协议会坏**——它必须手动跑：
//! ```text
//! RUSTFLAGS="--cfg loom" cargo test --test ic_publication_loom -- --ignored --nocapture
//! ```
//! 跑它的意义：证明这个模型**确实有判别力**，而不是一个永远绿的空门。
#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use loom::sync::Arc;

const UNRESOLVED: u32 = u32::MAX;

/// 约定：TypeId `t` 的载荷恒为 `t + 100`。于是「载荷 != tid + 100」即撕裂。
const fn payload_of(t: u32) -> u32 { t + 100 }

// ── 模型 1：单原子打包（现行实现） ────────────────────────────────────────

#[test]
fn packed_publication_never_tears() {
    loom::model(|| {
        let entry = Arc::new(AtomicU64::new(((UNRESOLVED as u64) << 32) | UNRESOLVED as u64));

        // 写者：先给 TypeId 1 安装，再驱逐成 TypeId 2（覆盖"驱逐方向"的撕裂）。
        let w = {
            let entry = Arc::clone(&entry);
            loom::thread::spawn(move || {
                entry.store(((1u64) << 32) | payload_of(1) as u64, Relaxed);
                entry.store(((2u64) << 32) | payload_of(2) as u64, Relaxed);
            })
        };

        // 读者：任何时刻读到的 (tid, 载荷) 必须配对。
        let r = {
            let entry = Arc::clone(&entry);
            loom::thread::spawn(move || {
                let v = entry.load(Relaxed);
                let (tid, payload) = ((v >> 32) as u32, v as u32);
                if tid != UNRESOLVED {
                    assert_eq!(payload, payload_of(tid),
                        "打包发布仍然撕裂了：tid={tid} 拿到载荷 {payload}");
                }
            })
        };

        w.join().unwrap();
        r.join().unwrap();
    });
}

// ── 模型 2：两个独立原子量（修复前协议，阴性对照） ──────────────────────────

#[test]
#[ignore = "阴性对照：断言旧协议会撕裂，预期失败；手动跑 -- --ignored 验证模型有判别力"]
fn split_publication_tears() {
    loom::model(|| {
        let tid_cell = Arc::new(AtomicU32::new(UNRESOLVED));
        let payload_cell = Arc::new(AtomicU32::new(UNRESOLVED));

        let w = {
            let (t, p) = (Arc::clone(&tid_cell), Arc::clone(&payload_cell));
            loom::thread::spawn(move || {
                // 修复前的 install：先写载荷、后写 TypeId（"write type_id LAST"）。
                p.store(payload_of(1), Relaxed);
                t.store(1, Relaxed);
                // 驱逐成 TypeId 2 —— 同样的两步。
                p.store(payload_of(2), Relaxed);
                t.store(2, Relaxed);
            })
        };

        let r = {
            let (t, p) = (Arc::clone(&tid_cell), Arc::clone(&payload_cell));
            loom::thread::spawn(move || {
                // 修复前的 lookup：先读 TypeId，再读载荷。
                let tid = t.load(Relaxed);
                let payload = p.load(Relaxed);
                if tid != UNRESOLVED {
                    assert_eq!(payload, payload_of(tid),
                        "读到不配对的 (tid={tid}, 载荷={payload}) —— 正是本 PR 修的 bug");
                }
            })
        };

        w.join().unwrap();
        r.join().unwrap();
    });
}
