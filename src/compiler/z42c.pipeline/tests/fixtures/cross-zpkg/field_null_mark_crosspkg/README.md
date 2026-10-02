# field_null_mark_crosspkg（正例：跨包字段 / 属性的 `?` 标记生效）

`carry-field-null-marks` 的跨包回归门。

- `target`（`demo.fnull`）：`public string? Marked;` / `public string? MarkedProp { get; set; }`，
  各配一个**没标**的对照（`Unmarked` / `UnmarkedProp`）。
- `main`（`demo.fnullapp`）：跨包读它们 —— 标了的走**快照 + 检查**（E0484 要求），
  没标的直接解引用。

## 为什么这是真门

`?` **不能拼进字段的 TYPE 段类型名**（那串是查找键 / struct 布局判据），标记只能走字段自己的
attr-ref 哨兵 `$Nullable`。这条通道断掉时**同包内看不出来**（符号表里本来就有标记位），
只有跨包才暴露。

配套的负例 `field_null_mark_crosspkg_unchecked/` 钉**该报必报** ——
没有它，本正例在「机制完全没接通」时**照样全绿**（不受检也能编过、输出一样）。
