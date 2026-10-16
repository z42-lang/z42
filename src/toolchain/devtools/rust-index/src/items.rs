//! 沿模块树遍历一个 crate，把条目写进索引。
//!
//! 条目 id 是 Rust 路径：crate 名 + 模块路径 + 名字（`z42::gc::heap::Heap::alloc`）。`impl` 块里的方法
//! 挂到 `impl` 所在模块里的同名类型下——绝大多数 impl 与类型同模块；跨模块的 impl 会挂到一个不存在的
//! 父条目上，方法本身照样能按名字查到。

use std::path::Path;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{File, ImplItem, Item, TraitItem, Type, Visibility};

use crate::out::{self, Writer};
use crate::rel_path;
use crate::text::{doc_of, is_test_only, path_attr, vis_of, Source};

pub struct Walker<'a> {
    root: &'a Path,
    krate: &'a str,
    w: &'a mut Writer,
}

/// 当前所在的文件与模块。
struct Ctx<'s> {
    src: &'s Source,
    rel: String,
    module: String,   // 模块的 id（crate 根 = crate 名）
}

impl<'a> Walker<'a> {
    pub fn new(root: &'a Path, krate: &'a str, w: &'a mut Writer) -> Self {
        Walker { root, krate, w }
    }

    pub fn crate_root(&mut self, file: &Path) -> Result<(), String> {
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        self.module_file(file, &dir, self.krate.to_string(), String::new(), self.krate.to_string())
    }

    /// 一个模块文件：`dir` 是它的子模块所在目录，`outer_doc` 是 `mod x;` 声明上的 doc。
    fn module_file(&mut self, file: &Path, dir: &Path, module: String, outer_doc: String, name: String) -> Result<(), String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let ast: File = syn::parse_file(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        let src = Source::new(&text);
        let rel = rel_path(self.root, file);
        self.w.file(&rel, self.krate);
        let inner = doc_of(&ast.attrs);
        let doc = join_docs(&outer_doc, &inner);
        let parent = parent_of(&module);
        self.w.item(&out::Item { id: &module, kind: "module", name: &name, parent: &parent,
            sig: &format!("mod {name}"), line: 1, vis: "public", doc: &doc });
        let ctx = Ctx { src: &src, rel, module };
        self.items(&ast.items, &ctx, dir)
    }

    fn items(&mut self, items: &[Item], ctx: &Ctx, dir: &Path) -> Result<(), String> {
        for it in items {
            self.w.file(&ctx.rel, self.krate);
            self.item(it, ctx, dir)?;
        }
        Ok(())
    }

    fn item(&mut self, it: &Item, ctx: &Ctx, dir: &Path) -> Result<(), String> {
        let m = &ctx.module;
        match it {
            Item::Fn(f) if !is_test_only(&f.attrs) => {
                let sig = ctx.src.slice(start_of(&f.vis, f.sig.span()), f.sig.span());
                self.emit(ctx, "func", &f.sig.ident.to_string(), m, &sig, f.sig.ident.span(), vis_of(&f.vis), &f.attrs);
            }
            Item::Struct(s) => {
                let id = self.emit(ctx, "struct", &s.ident.to_string(), m, &ctx.src.slice(start_of(&s.vis, s.struct_token.span), end_of_generics(s.ident.span(), &s.generics)),
                    s.ident.span(), vis_of(&s.vis), &s.attrs);
                for f in &s.fields {
                    if let Some(ident) = &f.ident {
                        let sig = ctx.src.slice(start_of(&f.vis, ident.span()), f.ty.span());
                        self.emit(ctx, "field", &ident.to_string(), &id, &sig, ident.span(), vis_of(&f.vis), &f.attrs);
                    }
                }
            }
            Item::Union(u) => {
                self.emit(ctx, "struct", &u.ident.to_string(), m, &ctx.src.slice(start_of(&u.vis, u.union_token.span), end_of_generics(u.ident.span(), &u.generics)),
                    u.ident.span(), vis_of(&u.vis), &u.attrs);
            }
            Item::Enum(e) => {
                let vis = vis_of(&e.vis);
                let id = self.emit(ctx, "enum", &e.ident.to_string(), m, &ctx.src.slice(start_of(&e.vis, e.enum_token.span), end_of_generics(e.ident.span(), &e.generics)),
                    e.ident.span(), vis, &e.attrs);
                for v in &e.variants {
                    let end = match (&v.discriminant, &v.fields) {
                        (Some((_, e)), _) => e.span(),
                        (None, syn::Fields::Unit) => v.ident.span(),
                        (None, f) => f.span(),
                    };
                    let sig = clip(&ctx.src.slice(v.ident.span(), end));
                    self.emit(ctx, "enum-member", &v.ident.to_string(), &id, &sig, v.ident.span(), vis, &v.attrs);
                }
            }
            Item::Trait(t) => {
                let id = self.emit(ctx, "trait", &t.ident.to_string(), m, &ctx.src.slice(start_of(&t.vis, t.trait_token.span), end_of_generics(t.ident.span(), &t.generics)),
                    t.ident.span(), vis_of(&t.vis), &t.attrs);
                for ti in &t.items {
                    if let TraitItem::Fn(f) = ti {
                        let sig = ctx.src.slice(f.sig.span(), f.sig.span());
                        self.emit(ctx, "method", &f.sig.ident.to_string(), &id, &sig, f.sig.ident.span(), "public", &f.attrs);
                    }
                }
            }
            Item::Impl(im) if !is_test_only(&im.attrs) => {
                let Some(owner) = self_type_name(&im.self_ty) else { return Ok(()); };
                let parent = format!("{m}::{owner}");
                let in_trait = im.trait_.is_some();
                for ii in &im.items {
                    if let ImplItem::Fn(f) = ii {
                        if is_test_only(&f.attrs) { continue; }
                        let vis = if in_trait { "public" } else { vis_of(&f.vis) };
                        let sig = ctx.src.slice(start_of(&f.vis, f.sig.span()), f.sig.span());
                        self.emit(ctx, "method", &f.sig.ident.to_string(), &parent, &sig, f.sig.ident.span(), vis, &f.attrs);
                    }
                }
            }
            Item::Const(c) => {
                let sig = ctx.src.slice(start_of(&c.vis, c.const_token.span), c.ty.span());
                self.emit(ctx, "const", &c.ident.to_string(), m, &sig, c.ident.span(), vis_of(&c.vis), &c.attrs);
            }
            Item::Static(s) => {
                let sig = ctx.src.slice(start_of(&s.vis, s.static_token.span), s.ty.span());
                self.emit(ctx, "static", &s.ident.to_string(), m, &sig, s.ident.span(), vis_of(&s.vis), &s.attrs);
            }
            Item::Type(t) => {
                let sig = ctx.src.slice(start_of(&t.vis, t.type_token.span), t.ty.span());
                self.emit(ctx, "type", &t.ident.to_string(), m, &sig, t.ident.span(), vis_of(&t.vis), &t.attrs);
            }
            Item::Macro(mac) => {
                if let Some(ident) = &mac.ident {
                    let sig = format!("macro_rules! {ident}");
                    self.emit(ctx, "macro", &ident.to_string(), m, &sig, ident.span(), "public", &mac.attrs);
                }
            }
            Item::Mod(md) if !is_test_only(&md.attrs) => self.module(md, ctx, dir)?,
            _ => {}
        }
        Ok(())
    }

    /// `mod x { … }` 就地递归；`mod x;` 找文件：`#[path]` 优先，否则 `dir/x.rs` 或 `dir/x/mod.rs`。
    fn module(&mut self, md: &syn::ItemMod, ctx: &Ctx, dir: &Path) -> Result<(), String> {
        let name = md.ident.to_string();
        let id = format!("{}::{name}", ctx.module);
        let doc = doc_of(&md.attrs);
        if let Some((_, items)) = &md.content {
            self.w.item(&out::Item { id: &id, kind: "module", name: &name, parent: &ctx.module,
                sig: &format!("mod {name}"), line: md.ident.span().start().line, vis: vis_of(&md.vis), doc: &doc });
            let sub = Ctx { src: ctx.src, rel: ctx.rel.clone(), module: id };
            return self.items(items, &sub, &dir.join(&name));
        }
        let (file, child_dir) = match path_attr(&md.attrs) {
            Some(p) => {
                let f = dir.join(p);
                let d = f.parent().map(Path::to_path_buf).unwrap_or_default();
                (f, d)
            }
            None => {
                let flat = dir.join(format!("{name}.rs"));
                if flat.is_file() { (flat, dir.join(&name)) } else { (dir.join(&name).join("mod.rs"), dir.join(&name)) }
            }
        };
        if !file.is_file() { return Ok(()); }   // cfg 掉的平台模块等：文件不在就跳过
        self.module_file(&file, &child_dir, id, doc, name)
    }

    /// 写一条条目，返回它的 id。
    #[allow(clippy::too_many_arguments)]
    fn emit(&mut self, ctx: &Ctx, kind: &str, name: &str, parent: &str, sig: &str, at: Span, vis: &str, attrs: &[syn::Attribute]) -> String {
        let id = format!("{parent}::{name}");
        let doc = doc_of(attrs);
        self.w.file(&ctx.rel, self.krate);
        self.w.item(&out::Item { id: &id, kind, name, parent, sig, line: at.start().line, vis, doc: &doc });
        id
    }
}

fn start_of(vis: &Visibility, fallback: Span) -> Span {
    match vis {
        Visibility::Inherited => fallback,
        v => v.span(),
    }
}

fn end_of_generics(ident: Span, g: &syn::Generics) -> Span {
    match &g.gt_token {
        Some(gt) => gt.span,
        None => ident,
    }
}

fn self_type_name(t: &Type) -> Option<String> {
    match t {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => self_type_name(&r.elem),
        _ => None,
    }
}

fn parent_of(module: &str) -> String {
    module.rsplit_once("::").map(|(p, _)| p.to_string()).unwrap_or_default()
}

/// 枚举变体可能带一整段结构体字段：签名只留前 120 个字符。
fn clip(s: &str) -> String {
    if s.chars().count() <= 120 { return s.to_string(); }
    let t: String = s.chars().take(120).collect();
    format!("{t}…")
}

fn join_docs(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a}\n\n{b}"),
    }
}
