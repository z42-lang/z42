//! `phase_timer` 的格式化单测。计时器本体（`Drop` 里的 `eprintln!`）测不到，
//! 能测的是「一行长什么样」——而对齐正是这个工具唯一的用处：同一次回收的十几行
//! 必须能竖着比，不然读不出形状。

use super::format_phase;
use std::time::Duration;

#[test]
fn formats_ms_with_three_decimals() {
    let line = format_phase("full mark", Duration::from_micros(25_634), None);
    assert_eq!(line, "z42-gc:   full mark                     25.634 ms");
}

#[test]
fn count_is_appended_when_present() {
    let line = format_phase("full mark", Duration::from_micros(25_634), Some(953_988));
    assert_eq!(line, "z42-gc:   full mark                     25.634 ms  (953988)");
}

#[test]
fn names_pad_to_a_common_column() {
    // 有计数和没计数的两行、长名和短名的两行，`ms` 都要落在同一列。
    let short = format_phase("sweep/var", Duration::from_millis(1), None);
    let long = format_phase("minor/chunk reclaim", Duration::from_millis(1), Some(7));
    let col = |s: &str| s.find(" ms").expect("每行都有 ms");
    assert_eq!(col(&short), col(&long));
}

#[test]
fn over_long_names_are_not_truncated() {
    // 宁可破坏对齐也不要截断名字——截断过的名字在 grep 里找不回来。
    let line = format_phase("a-very-long-phase-name-that-overflows", Duration::ZERO, None);
    assert!(line.contains("a-very-long-phase-name-that-overflows"), "{line}");
}
