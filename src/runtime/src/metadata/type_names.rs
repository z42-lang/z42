//! 类型名 / 函数名的切分 —— **实参里的点不算**。
//!
//! 编译器把实例化实参写成全名（`Demo.Pair<Demo.A.P,int>`、特化函数 `Demo.C.Echo:Demo.A.P`），
//! 同包两个 ns 的同名 struct 才能各是各的。于是「取最后一个 `.`」这类切法会落进实参里：
//! `Type.Name` 得出 `P>`、静态方法的 owner 得出 `Demo.C.Echo:Demo.A`。这里的函数只看**顶层**
//! （尖括号外、特化实参 `:` 之前）的分隔符。

/// 顶层（`<…>` 之外、第一个顶层 `:` 之前）最后一个 `.` 的位置。
/// `Demo.Pair<Demo.A.P>.Get` → `Demo.Pair<Demo.A.P>` 与 `Get` 之间那个；`Demo.C.Echo:Demo.A.P` → `C` 之后那个。
pub fn last_top_dot(name: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut last = None;
    for (i, c) in name.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ':' if depth == 0 => break,
            '.' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// 顶层最后一个 `c` 的位置（同 [`last_top_dot`] 的口径，用于嵌套类型的 `+`）。
pub fn last_top(name: &str, ch: char) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut last = None;
    for (i, c) in name.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ':' if depth == 0 => break,
            _ if c == ch && depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// 去掉命名空间、实参递归同样去掉：`Demo.Pair<Demo.A.P,int>` → `Pair<P,int>`；嵌套类型保留
/// `Outer+Inner`（装箱值的默认 `ToString` 口径）。
pub fn unqualified(full: &str) -> String {
    strip(full, false)
}

/// `Type.Name` 口径：同 [`unqualified`]，基名再去掉外围类型前缀（`Outer+Inner` → `Inner`，同 C#）。
pub fn simple(full: &str) -> String {
    strip(full, true)
}

fn strip(full: &str, drop_outer: bool) -> String {
    let (base, args) = match full.find('<') {
        Some(i) if full.ends_with('>') => (&full[..i], Some(&full[i + 1..full.len() - 1])),
        _ => (full, None),
    };
    let mut b = base.rsplit('.').next().unwrap_or(base);
    if drop_outer {
        b = b.rsplit('+').next().unwrap_or(b);
    }
    match args {
        None => b.to_string(),
        Some(a) => {
            let parts: Vec<String> = split_top_commas(a).iter().map(|x| strip(x.trim(), drop_outer)).collect();
            format!("{}<{}>", b, parts.join(","))
        }
    }
}

fn split_top_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' => depth += 1,
            '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
#[path = "type_names_tests.rs"]
mod tests;
