# Tasks: interp 数组 get/set 去掉 GcRef 克隆

**变更说明：** interp 的 `array_get`/`array_set` 此前为释放 frame 借用而 `rc.clone()`（Arc 引用计数原子）
再 borrow；改为**先读 index**（其 frame 借用即结束），再直接经 `frame.get(arr)` 借 GcRef，省掉每次访问
的 Arc 原子。array_set 的写屏障（仅堆引用值、罕见）用 match 绑定的 `&Value` 就地传，不再 clone 数组。
**原因：** 数组扫描热循环里每元素一次 Arc inc/dec 原子；`GcRef` 是 parking_lot Mutex（多线程地基），
clone 是可省的那一半。行为完全不变（纯借用模式重排）。
**文档影响：** 无（内部实现，行为不变）。

- [x] 1.1 `interp/exec_array.rs::array_get`：先读 idx，去 `rc.clone()`
- [x] 1.2 `interp/exec_array.rs::array_set`：先读 idx，match 绑定 `arr_val` 供写屏障，去 `arr_value.clone()`
- [x] 1.3 正确性：gauntlet interp==jit 全一致；cargo test 897/0
- [x] 1.4 性能：interp fill+scan int −7.6% / long −4.7% / double −3.7%
- [x] 1.5 e2e-direct 205/208（interp+jit，=baseline 同款 3 例直跑器局限，零回退）→ PR
