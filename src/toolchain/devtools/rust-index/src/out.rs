//! 输出：`z42 doc` 的外部索引格式（与 z42.doc 的缓存同一套条目行）。
//!
//! ```text
//! z42doc-items 1[\t<重建命令>]
//! F <相对仓库根的文件> <crate 名>
//! I <id> <kind> <name> <parent> <sig> <line> <vis> <doc>
//! ```
//! 字段以制表符分隔；字段内的 `\`、制表符、换行写成 `\\` `\t` `\n`。

pub struct Writer {
    buf: String,
    cur_file: String,
}

/// 一条条目。`id` 是 Rust 路径（`z42::gc::heap::Heap::alloc`），`parent` 是所属条目的 id。
pub struct Item<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
    pub parent: &'a str,
    pub sig: &'a str,
    pub line: usize,
    pub vis: &'a str,
    pub doc: &'a str,
}

impl Writer {
    /// `regen` 写进表头：`z42 doc` 发现索引比源码旧时，提示用户跑它。
    pub fn new(regen: &str) -> Self {
        let head = if regen.is_empty() { "z42doc-items 1\n".to_string() } else { format!("z42doc-items 1\t{}\n", esc(regen)) };
        Writer { buf: head, cur_file: String::new() }
    }

    /// 切到另一个文件：之后的条目都记在它名下。同一文件连续写入只出一条 `F`。
    pub fn file(&mut self, rel: &str, krate: &str) {
        if self.cur_file == rel { return; }
        self.cur_file = rel.to_string();
        self.buf.push_str(&format!("F\t{}\t{}\n", esc(rel), esc(krate)));
    }

    pub fn item(&mut self, it: &Item) {
        self.buf.push_str(&format!(
            "I\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            esc(it.id), it.kind, esc(it.name), esc(it.parent), esc(it.sig), it.line, it.vis, esc(it.doc)
        ));
    }

    pub fn finish(self) -> String { self.buf }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}
