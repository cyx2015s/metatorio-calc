//! Locate test-only Rust items without mistaking braces in comments/strings for syntax.
use std::{
    io::{self, BufRead},
    path::{Path, PathBuf},
};
use syn::{Attribute, Item, Meta, spanned::Spanned, visit::Visit};

// Unknown feature/platform predicates stay unknown. Only exclude items that
// definitely cannot exist with `test = false`.
fn production(meta: &Meta) -> Option<bool> {
    match meta {
        Meta::Path(path) if path.is_ident("test") => Some(false),
        Meta::List(list) => {
            let args = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            let values: Vec<_> = args.iter().map(production).collect();
            if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|value| !value)
            } else if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|v| *v == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|v| *v == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "test")
            || (attr.path().is_ident("cfg")
                && attr.parse_args::<Meta>().ok().as_ref().and_then(production) == Some(false))
    })
}

fn attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

// syn does not yet parse nightly field `pub mut(self)` restrictions. Blank
// only those lexer-identified tokens; retain byte offsets and line numbers.
fn parse(source: &str) -> syn::Result<syn::File> {
    fn restrictions(tokens: proc_macro2::TokenStream, spans: &mut Vec<std::ops::Range<usize>>) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        for pair in tokens.windows(2) {
            if let [
                proc_macro2::TokenTree::Ident(word),
                proc_macro2::TokenTree::Group(group),
            ] = pair
            {
                if word == "mut"
                    && group.delimiter() == proc_macro2::Delimiter::Parenthesis
                    && matches!(group.stream().into_iter().next(), Some(proc_macro2::TokenTree::Ident(name)) if matches!(name.to_string().as_str(), "self" | "super" | "crate" | "in"))
                {
                    spans.push(word.span().byte_range().start..group.span().byte_range().end);
                }
            }
        }
        for token in tokens {
            if let proc_macro2::TokenTree::Group(group) = token {
                restrictions(group.stream(), spans);
            }
        }
    }
    let mut spans = Vec::new();
    restrictions(source.parse()?, &mut spans);
    let mut bytes = source.as_bytes().to_vec();
    for span in spans {
        for byte in &mut bytes[span] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    }
    syn::parse_file(std::str::from_utf8(&bytes).expect("token boundaries preserve UTF-8"))
}

struct Tests {
    ranges: Vec<(usize, usize)>,
    modules: Vec<PathBuf>,
    directory: PathBuf,
}
impl<'ast> Visit<'ast> for Tests {
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attrs = match item {
            syn::ImplItem::Const(i) => &i.attrs,
            syn::ImplItem::Fn(i) => &i.attrs,
            syn::ImplItem::Type(i) => &i.attrs,
            syn::ImplItem::Macro(i) => &i.attrs,
            _ => return syn::visit::visit_impl_item(self, item),
        };
        if test_only(attrs) {
            self.ranges
                .push((attrs[0].span().start().line, item.span().end().line));
        } else {
            syn::visit::visit_impl_item(self, item);
        }
    }
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        assert!(
            !test_only(std::slice::from_ref(attr)),
            "test-only attribute outside a supported item at line {}",
            attr.span().start().line
        );
        syn::visit::visit_attribute(self, attr);
    }
    fn visit_item(&mut self, item: &'ast Item) {
        if test_only(attrs(item)) {
            let start = attrs(item)
                .first()
                .map_or(item.span().start().line, |a| a.span().start().line);
            self.ranges.push((start, item.span().end().line));
            if let Item::Mod(module) = item {
                if module.content.is_none() {
                    let explicit = module.attrs.iter().find_map(|attr| {
                        if let Meta::NameValue(value) = &attr.meta {
                            if value.path.is_ident("path") {
                                if let syn::Expr::Lit(expr) = &value.value {
                                    if let syn::Lit::Str(path) = &expr.lit {
                                        return Some(path.value());
                                    }
                                }
                            }
                        }
                        None
                    });
                    let name = module.ident.to_string();
                    if let Some(path) = explicit {
                        self.modules.push(self.directory.join(path));
                    } else {
                        self.modules.push(self.directory.join(format!("{name}.rs")));
                        self.modules.push(self.directory.join(name));
                    }
                }
            }
        } else {
            syn::visit::visit_item(self, item);
        }
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        let parent = self.directory.clone();
        if module.content.is_some() {
            self.directory.push(module.ident.to_string());
        }
        syn::visit::visit_item_mod(self, module);
        self.directory = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_requires_test_instead_of_merely_mentioning_it() {
        for expression in ["test", "all(test, feature = \"x\")", "not(not(test))"] {
            assert_eq!(
                production(&syn::parse_str(expression).unwrap()),
                Some(false)
            );
        }
        for expression in ["any(test, feature = \"x\")", "feature = \"x\""] {
            assert_eq!(production(&syn::parse_str(expression).unwrap()), None);
        }
        assert_eq!(
            production(&syn::parse_str("not(test)").unwrap()),
            Some(true)
        );
    }

    #[test]
    fn spans_survive_strings_comments_and_nightly_field_visibility() {
        let file = parse(
            "pub struct S { pub mut(self) n: u8 }\n\
            const S: &str = r#\"#[cfg(test)] mod fake { }\"#;\n\
            // #[cfg(test)] mod also_fake {}\n\
            #[cfg(test)]\nmod tests { fn t() { let s = \"}\"; } }\n\
            #[cfg(all(test, feature = \"x\"))]\nmod helpers;\n\
            impl S {\n#[cfg(test)]\nfn helper(&self) {}\n}\n",
        )
        .unwrap();
        let mut tests = Tests {
            ranges: vec![],
            modules: vec![],
            directory: "src".into(),
        };
        tests.visit_file(&file);
        assert_eq!(tests.ranges, [(4, 5), (6, 7), (9, 10)]);
        assert_eq!(
            tests.modules,
            [
                PathBuf::from("src/helpers.rs"),
                PathBuf::from("src/helpers")
            ]
        );
    }
}

fn main() {
    for line in io::stdin().lock().lines() {
        let path = PathBuf::from(line.expect("input path"));
        let source = std::fs::read_to_string(&path).expect("read Rust source");
        let file = parse(&source).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        if !matches!(
            path.file_stem().and_then(|s| s.to_str()),
            Some("lib" | "main" | "mod" | "build")
        ) {
            directory.push(path.file_stem().unwrap());
        }
        let mut tests = Tests {
            ranges: vec![],
            modules: vec![],
            directory,
        };
        if test_only(&file.attrs) {
            tests.ranges.push((1, source.lines().count()));
        } else {
            tests.visit_file(&file);
        }
        println!(
            "{}",
            serde_json::json!({"path": path, "ranges": tests.ranges, "modules": tests.modules})
        );
    }
}
