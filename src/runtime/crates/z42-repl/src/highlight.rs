//! 输入行语法着色（rustyline `Highlighter::highlight`）。
//!
//! **关键字表不在这里定义。** 唯一 SoT 是 `z42c.syntax` 的 `Lexer._initKeywords()`；
//! z42 侧在 REPL 启动时经 `z42_repl_set_keywords` 一次性灌进来（见 `set_keywords`）。
//! 没灌过 = 关键字集为空 = 关键字不着色，其余（字符串 / 注释 / 数字）照常——
//! **降级而不是猜**，免得把不是关键字的词染成关键字。
//!
//! 为什么一次性灌而不是每次按键回调 VM：`highlight` 在**每个按键**上跑，
//! 跨 C ABI 重入 VM 取关键字会让打字变卡。

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// 本线程的关键字集。REPL 是单线程交互循环，thread_local 足够，
    /// 且与 crate 里 `CBS` 的既有做法一致。
    static KEYWORDS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// 安装关键字表（`\n` 分隔，空行忽略）。重复调用覆盖。
pub fn set_keywords(joined: &str) {
    KEYWORDS.with(|k| {
        let mut k = k.borrow_mut();
        k.clear();
        for w in joined.split('\n') {
            let w = w.trim();
            if !w.is_empty() {
                k.insert(w.to_string());
            }
        }
    });
}

fn is_keyword(w: &str) -> bool {
    KEYWORDS.with(|k| k.borrow().contains(w))
}

/// 本 crate 是否已拿到关键字表。没拿到时 `highlight` 仍会给字符串 / 注释 / 数字着色。
pub fn has_keywords() -> bool {
    KEYWORDS.with(|k| !k.borrow().is_empty())
}

// ── 颜色 ────────────────────────────────────────────────────────────────────
// 用 8 色基本集（30–37 / 90–97），不用 256 色或 truecolor：REPL 要在各种终端与
// 配色方案下都不难看，基本集由终端主题决定具体色值，跟随用户配色。

const KEYWORD: &str = "\x1b[35m"; // magenta
const STRING: &str = "\x1b[32m"; // green
const COMMENT: &str = "\x1b[90m"; // bright black（与 ghost 提示同色，都是"非代码"）
const NUMBER: &str = "\x1b[36m"; // cyan
const RESET: &str = "\x1b[0m";

/// 给一行 z42 源码着色，返回带 ANSI 序列的新串。
///
/// 这是个**只为显示服务的粗词法器**，不是编译器词法器的替身：
/// 它不建 token、不报错、遇到不认识的东西原样吐出。判定顺序（先到先得）：
/// 行注释 `//` → 块注释 `/* */` → 字符串 / 字符（含 `\` 转义与 `"""` 原始串）
/// → 数字 → 标识符（查关键字表）→ 其它原样。
pub fn highlight_line(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len() + 32);
    let mut i = 0usize;

    while i < b.len() {
        let c = b[i];

        // 行注释：到行尾
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            out.push_str(COMMENT);
            out.push_str(&line[i..]);
            out.push_str(RESET);
            return out;
        }

        // 块注释：到 */ 或行尾（REPL 一次一行，未闭合就染到行尾）
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let end = find_block_comment_end(b, i + 2);
            out.push_str(COMMENT);
            out.push_str(&line[i..end]);
            out.push_str(RESET);
            i = end;
            continue;
        }

        // 原始字符串 """..."""
        if c == b'"' && i + 2 < b.len() && b[i + 1] == b'"' && b[i + 2] == b'"' {
            let end = find_raw_string_end(b, i + 3);
            out.push_str(STRING);
            out.push_str(&line[i..end]);
            out.push_str(RESET);
            i = end;
            continue;
        }

        // 普通字符串 / 字符字面量（同一套扫法，只是定界符不同）
        if c == b'"' || c == b'\'' {
            let end = find_quoted_end(b, i + 1, c);
            out.push_str(STRING);
            out.push_str(&line[i..end]);
            out.push_str(RESET);
            i = end;
            continue;
        }

        // 数字：以数字开头，允许内部 `_` / `.` / 十六进制 / 后缀字母
        if c.is_ascii_digit() {
            let end = find_number_end(b, i);
            out.push_str(NUMBER);
            out.push_str(&line[i..end]);
            out.push_str(RESET);
            i = end;
            continue;
        }

        // 标识符 / 关键字
        if c.is_ascii_alphabetic() || c == b'_' {
            let end = find_ident_end(b, i);
            let word = &line[i..end];
            if is_keyword(word) {
                out.push_str(KEYWORD);
                out.push_str(word);
                out.push_str(RESET);
            } else {
                out.push_str(word);
            }
            i = end;
            continue;
        }

        // 其它（含非 ASCII）：按 UTF-8 字符整体搬运，别把多字节字符切开
        let step = utf8_len(c);
        let end = (i + step).min(b.len());
        out.push_str(&line[i..end]);
        i = end;
    }

    out
}

// ── 扫描辅助（全部返回"结束位置的下一个字节下标"，且保证落在字符边界上）────

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else if first >> 3 == 0b11110 {
        4
    } else {
        1 // 非法起始字节：按 1 字节推进，避免死循环
    }
}

fn find_block_comment_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while i + 1 < b.len() {
        if b[i] == b'*' && b[i + 1] == b'/' {
            return i + 2;
        }
        i += utf8_len(b[i]);
    }
    b.len()
}

fn find_raw_string_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while i + 2 < b.len() {
        if b[i] == b'"' && b[i + 1] == b'"' && b[i + 2] == b'"' {
            return i + 3;
        }
        i += utf8_len(b[i]);
    }
    b.len()
}

/// 普通引号串：`\` 转义下一个字节；未闭合则到行尾。
fn find_quoted_end(b: &[u8], from: usize, quote: u8) -> usize {
    let mut i = from;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 1 + if i + 1 < b.len() { utf8_len(b[i + 1]) } else { 0 };
            continue;
        }
        if b[i] == quote {
            return i + 1;
        }
        i += utf8_len(b[i]);
    }
    b.len()
}

fn find_number_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
            // `1..2`（区间）不该整体吞掉：遇到第二个连续 `.` 就停
            if c == b'.' && i + 1 < b.len() && b[i + 1] == b'.' {
                return i;
            }
            i += 1;
        } else {
            return i;
        }
    }
    b.len()
}

fn find_ident_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_kw<T>(f: impl FnOnce() -> T) -> T {
        set_keywords("void\nint\nif\nreturn\nvar");
        f()
    }

    /// 着色只加 ANSI 序列，不改变可见文本——把序列剥掉必须还原成原串。
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == 0x1b {
                while i < b.len() && b[i] != b'm' {
                    i += 1;
                }
                i += 1;
            } else {
                let n = utf8_len(b[i]);
                out.push_str(&s[i..(i + n).min(s.len())]);
                i += n;
            }
        }
        out
    }

    #[test]
    fn roundtrip_preserves_text() {
        with_kw(|| {
            for line in [
                "void Main() { int x = 1; }",
                r#"Console.WriteLine("hi \" there");"#,
                "// 全行注释",
                "/* 块 */ var y = 0x1F;",
                r#"var s = """raw "quoted" here""";"#,
                "中文标识符也不该被切坏 x = 1;",
                "for (i = 0; i < 10; i = i + 1)",
                "1..2",
                "'a' '\\n'",
            ] {
                assert_eq!(strip_ansi(&highlight_line(line)), line, "line: {line}");
            }
        });
    }

    #[test]
    fn keywords_colored_identifiers_not() {
        with_kw(|| {
            let h = highlight_line("void Foo");
            assert!(h.contains(&format!("{KEYWORD}void{RESET}")));
            assert!(!h.contains(&format!("{KEYWORD}Foo{RESET}")));
        });
    }

    #[test]
    fn keyword_substring_is_not_a_keyword() {
        with_kw(|| {
            // `interned` 以 `int` 开头，但不是关键字——整词匹配，不是前缀匹配
            let h = highlight_line("interned");
            assert!(!h.contains(KEYWORD), "got: {h:?}");
        });
    }

    #[test]
    fn keyword_inside_string_is_not_colored() {
        with_kw(|| {
            let h = highlight_line(r#""void""#);
            assert!(h.contains(STRING));
            assert!(!h.contains(KEYWORD));
        });
    }

    #[test]
    fn no_keywords_installed_still_colors_strings() {
        set_keywords("");
        let h = highlight_line(r#"void x = "s";"#);
        assert!(!has_keywords());
        assert!(h.contains(STRING), "字符串仍应着色");
        assert!(!h.contains(KEYWORD), "没有关键字表时不得猜");
    }

    #[test]
    fn unterminated_forms_do_not_panic() {
        with_kw(|| {
            for line in ["\"unterminated", "/* unterminated", "'", r#"""" "#, "\\"] {
                let _ = highlight_line(line);
            }
        });
    }
}
