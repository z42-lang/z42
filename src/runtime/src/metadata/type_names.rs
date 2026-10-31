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

// ── 构造类型的规范拼写 ─────────────────────────────────────────────────────────
//
// 一个构造类型（`List<int>`）在运行期有**一个**身份名，比较一律逐字比它：实例上记的实参
// （`ObjNew`）、类型测试的目标（`is_instance`）、实例化描述符的注册名、`Type` 的身份全由它定。
// 编译器发射时已按这套拼写写（`LayoutKey`：类全名、内建类型关键字、无空格）；运行期**自己造出**
// 名字的地方（反射：`MakeGenericType` / `Activator` / `ConstructorInfo.Invoke` 从 `Type` 取名）
// 经 [`canonical`] 回到同一拼写。`Type.FullName` 是它的展示形 [`display`]（内建类型写 `Std.Int32`）。
//
// 规则：
// - 顶层名：内建类型写全名（`Std.Int32`、`Std.Object` —— 描述符的注册键）；
// - 类型实参与数组元素：内建类型写关键字（`int`、`object`），递归；
// - 去掉空白（旧拼写 `Pair<Int32, Int64>` 的逗号后空格）。

/// 内建类型：关键字 ↔ `Std` 全名（顺序无关；`float` 是 `Std.Single`）。
const BUILTIN_TYPES: [(&str, &str); 14] = [
    ("sbyte", "Std.SByte"), ("byte", "Std.Byte"), ("short", "Std.Int16"), ("ushort", "Std.UInt16"),
    ("int", "Std.Int32"), ("uint", "Std.UInt32"), ("long", "Std.Int64"), ("ulong", "Std.UInt64"),
    ("float", "Std.Single"), ("double", "Std.Double"), ("bool", "Std.Boolean"), ("char", "Std.Char"),
    ("string", "Std.String"), ("object", "Std.Object"),
];

/// 内建类型的任一拼写（关键字 `int`、VM 标签 `i32`、全名 `Std.Int32`）→ 关键字；不是内建类型 → `None`。
pub fn builtin_keyword(name: &str) -> Option<&'static str> {
    let kw = match name {
        "i8" => "sbyte", "u8" => "byte", "i16" => "short", "u16" => "ushort",
        "i32" => "int", "u32" => "uint", "i64" => "long", "u64" => "ulong",
        "f32" => "float", "f64" => "double", "str" => "string",
        _ => name,
    };
    BUILTIN_TYPES.iter().find(|(k, f)| *k == kw || *f == kw).map(|(k, _)| *k)
}

/// 内建类型的任一拼写 → `Std` 全名；不是内建类型 → `None`。
pub fn builtin_fqn(name: &str) -> Option<&'static str> {
    let kw = builtin_keyword(name)?;
    BUILTIN_TYPES.iter().find(|(k, _)| *k == kw).map(|(_, f)| *f)
}

/// 类型名的规范拼写（运行期身份名）。见本节抬头的规则。
pub fn canonical(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    write_name(name, Spelling::Top, &mut out);
    out
}

/// 类型名作为**类型实参**时的规范拼写（内建类型写关键字）。
pub fn canonical_arg(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    write_name(name, Spelling::Arg, &mut out);
    out
}

/// `Type.FullName` 的拼写：同 [`canonical`]，但内建类型一律写全名（`Std.Collections.List<Std.Int32>`）。
pub fn display(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    write_name(name, Spelling::Display, &mut out);
    out
}

/// `def` 实例化在 `args` 上的规范名（`Std.Collections.List` + `[Std.Int32]` → `Std.Collections.List<int>`）。
/// `args` 为空 → `def` 本身的规范名。
pub fn constructed(def: &str, args: &[String]) -> String {
    if args.is_empty() {
        return canonical(def);
    }
    let mut out = String::with_capacity(def.len() + 8 * args.len());
    write_name(def, Spelling::Top, &mut out);
    out.push('<');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_name(a, Spelling::Arg, &mut out);
    }
    out.push('>');
    out
}

/// 去掉实参的定义名：`Demo.Box<int>` → `Demo.Box`。
pub fn erased(name: &str) -> &str {
    match name.find('<') {
        Some(i) => &name[..i],
        None => name,
    }
}

/// 把 `name` 里**整词**等于型参 `params[i]` 的成分换成 `args[i]`（`T[]` / `List<T>` / `Func<T,R>` 均按词替换，
/// `TArray` 这类含型参名的标识符不受影响），结果是规范拼写（顶层）。元数不符 → 原样规范化。
pub fn substitute(name: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() || params.len() != args.len() {
        return canonical(name);
    }
    let is_ident = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '+' | '$');
    let mut out = String::with_capacity(name.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        match params.iter().position(|p| p == word) {
            Some(i) => out.push_str(&args[i]),
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in name.chars() {
        if is_ident(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    canonical(&out)
}

#[derive(Clone, Copy, PartialEq)]
enum Spelling {
    /// 顶层身份名：内建类型写全名。
    Top,
    /// 类型实参 / 数组元素：内建类型写关键字。
    Arg,
    /// `Type.FullName`：内建类型处处写全名。
    Display,
}

fn write_name(name: &str, sp: Spelling, out: &mut String) {
    let name = name.trim();
    let inner_sp = if sp == Spelling::Display { Spelling::Display } else { Spelling::Arg };
    if let Some(elem) = name.strip_suffix("[]") {
        write_name(elem, inner_sp, out);
        out.push_str("[]");
        return;
    }
    if let Some(inner) = name.strip_suffix('?') {
        write_name(inner, sp, out);
        out.push('?');
        return;
    }
    if let Some(lt) = name.find('<') {
        if lt > 0 && name.ends_with('>') {
            out.push_str(name[..lt].trim());
            out.push('<');
            for (i, a) in split_top_commas(&name[lt + 1..name.len() - 1]).iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_name(a, inner_sp, out);
            }
            out.push('>');
            return;
        }
    }
    let mapped = match sp {
        Spelling::Arg => builtin_keyword(name),
        Spelling::Top | Spelling::Display => builtin_fqn(name),
    };
    out.push_str(mapped.unwrap_or(name));
}

#[cfg(test)]
#[path = "type_names_tests.rs"]
mod tests;
