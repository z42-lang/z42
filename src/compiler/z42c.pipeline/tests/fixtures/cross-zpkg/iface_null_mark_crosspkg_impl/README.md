# iface_null_mark_crosspkg_impl（负例：实现跨包接口时去掉形参的 `?`）

`carry-iface-null-marks` 的**正面对照之二**（D7 继承一致性这端）。`main` 里的类实现
一个**跨包**接口，却把 `Take(string? maybe)` 的 `?` 去掉 ⇒ 期望编不过、报 **E0489**。

## 方向是刻意挑的

反方向（契约没标、实现**加** `?`）**修前就能报**：导入侧标记位缺省为 false，而
「契约方没标」恰好也是 false，蒙对了 —— 那种 fixture 一条也分辨不出。只有
「契约方**标了**、实现方去掉」这一侧非得把导入标记位真的还原回来才抓得到。
（同 `nullable_marks_cross_pkg_override` 的教训，那条钉的是跨包 **override**，
本条钉的是跨包**接口实现**：两条消费路径不同，`_checkNullableDirection` 共用。）
