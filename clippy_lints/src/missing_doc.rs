// Note: More specifically this lint is largely inspired (aka copied) from
// *rustc*'s
// [`missing_doc`].
//
// [`missing_doc`]: https://github.com/rust-lang/rust/blob/d6d05904697d89099b55da3331155392f1db9c00/src/librustc_lint/builtin.rs#L246
//

use crate::utils::{in_macro, span_lint};
use rustc::hir;
use rustc::lint::{LateContext, LateLintPass, LintArray, LintContext, LintPass};
use rustc::ty;
use rustc::{declare_tool_lint, lint_array};
use syntax::ast;
use syntax::attr;
use syntax::source_map::Span;

/// **What it does:** Warns if there is missing doc for any documentable item
/// (public or private).
///
/// **Why is this bad?** Doc is good. *rustc* has a `MISSING_DOCS`
/// allowed-by-default lint for
/// public members, but has no way to enforce documentation of private items.
/// This lint fixes that.
///
/// **Known problems:** None.
declare_clippy_lint! {
    pub MISSING_DOCS_IN_PRIVATE_ITEMS,
    restriction,
    "detects missing documentation for public and private members"
}

pub struct MissingDoc {
    /// Stack of whether #[doc(hidden)] is set
    /// at each level which has lint attributes.
    doc_hidden_stack: Vec<bool>,
}

impl ::std::default::Default for MissingDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl MissingDoc {
    pub fn new() -> Self {
        Self {
            doc_hidden_stack: vec![false],
        }
    }

    fn doc_hidden(&self) -> bool {
        *self.doc_hidden_stack.last().expect("empty doc_hidden_stack")
    }

    fn check_missing_docs_attrs(
        &self,
        cx: &LateContext<'_, '_>,
        attrs: &[ast::Attribute],
        sp: Span,
        desc: &'static str,
    ) {
        // If we're building a test harness, then warning about
        // documentation is probably not really relevant right now.
        if cx.sess().opts.test {
            return;
        }

        // `#[doc(hidden)]` disables missing_docs check.
        if self.doc_hidden() {
            return;
        }

        if in_macro(sp) {
            return;
        }

        let has_doc = attrs.iter().any(|a| a.is_value_str() && a.name() == "doc");
        if !has_doc {
            span_lint(
                cx,
                MISSING_DOCS_IN_PRIVATE_ITEMS,
                sp,
                &format!("missing documentation for {}", desc),
            );
        }
    }
}

impl LintPass for MissingDoc {
    fn get_lints(&self) -> LintArray {
        lint_array![MISSING_DOCS_IN_PRIVATE_ITEMS]
    }

    fn name(&self) -> &'static str {
        "MissingDoc"
    }
}

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for MissingDoc {
    fn enter_lint_attrs(&mut self, _: &LateContext<'a, 'tcx>, attrs: &'tcx [ast::Attribute]) {
        let doc_hidden = self.doc_hidden()
            || attrs.iter().any(|attr| {
                attr.check_name("doc")
                    && match attr.meta_item_list() {
                        None => false,
                        Some(l) => attr::list_contains_name(&l[..], "hidden"),
                    }
            });
        self.doc_hidden_stack.push(doc_hidden);
    }

    fn exit_lint_attrs(&mut self, _: &LateContext<'a, 'tcx>, _: &'tcx [ast::Attribute]) {
        self.doc_hidden_stack.pop().expect("empty doc_hidden_stack");
    }

    fn check_crate(&mut self, cx: &LateContext<'a, 'tcx>, krate: &'tcx hir::Crate) {
        self.check_missing_docs_attrs(cx, &krate.attrs, krate.span, "crate");
    }

    fn check_item(&mut self, cx: &LateContext<'a, 'tcx>, it: &'tcx hir::Item) {
        let desc = match it.node {
            hir::ItemKind::Const(..) => "a constant",
            hir::ItemKind::Enum(..) => "an enum",
            hir::ItemKind::Fn(..) => {
                // ignore main()
                if it.ident.name == "main" {
                    let def_id = cx.tcx.hir().local_def_id(it.id);
                    let def_key = cx.tcx.hir().def_key(def_id);
                    if def_key.parent == Some(hir::def_id::CRATE_DEF_INDEX) {
                        return;
                    }
                }
                "a function"
            },
            hir::ItemKind::Mod(..) => "a module",
            hir::ItemKind::Static(..) => "a static",
            hir::ItemKind::Struct(..) => "a struct",
            hir::ItemKind::Trait(..) => "a trait",
            hir::ItemKind::TraitAlias(..) => "a trait alias",
            hir::ItemKind::Ty(..) => "a type alias",
            hir::ItemKind::Union(..) => "a union",
            hir::ItemKind::Existential(..) => "an existential type",
            hir::ItemKind::ExternCrate(..)
            | hir::ItemKind::ForeignMod(..)
            | hir::ItemKind::GlobalAsm(..)
            | hir::ItemKind::Impl(..)
            | hir::ItemKind::Use(..) => return,
        };

        self.check_missing_docs_attrs(cx, &it.attrs, it.span, desc);
    }

    fn check_trait_item(&mut self, cx: &LateContext<'a, 'tcx>, trait_item: &'tcx hir::TraitItem) {
        let desc = match trait_item.node {
            hir::TraitItemKind::Const(..) => "an associated constant",
            hir::TraitItemKind::Method(..) => "a trait method",
            hir::TraitItemKind::Type(..) => "an associated type",
        };

        self.check_missing_docs_attrs(cx, &trait_item.attrs, trait_item.span, desc);
    }

    fn check_impl_item(&mut self, cx: &LateContext<'a, 'tcx>, impl_item: &'tcx hir::ImplItem) {
        // If the method is an impl for a trait, don't doc.
        let def_id = cx.tcx.hir().local_def_id(impl_item.id);
        match cx.tcx.associated_item(def_id).container {
            ty::TraitContainer(_) => return,
            ty::ImplContainer(cid) => {
                if cx.tcx.impl_trait_ref(cid).is_some() {
                    return;
                }
            },
        }

        let desc = match impl_item.node {
            hir::ImplItemKind::Const(..) => "an associated constant",
            hir::ImplItemKind::Method(..) => "a method",
            hir::ImplItemKind::Type(_) => "an associated type",
            hir::ImplItemKind::Existential(_) => "an existential type",
        };
        self.check_missing_docs_attrs(cx, &impl_item.attrs, impl_item.span, desc);
    }

    fn check_struct_field(&mut self, cx: &LateContext<'a, 'tcx>, sf: &'tcx hir::StructField) {
        if !sf.is_positional() {
            self.check_missing_docs_attrs(cx, &sf.attrs, sf.span, "a struct field");
        }
    }

    fn check_variant(&mut self, cx: &LateContext<'a, 'tcx>, v: &'tcx hir::Variant, _: &hir::Generics) {
        self.check_missing_docs_attrs(cx, &v.node.attrs, v.span, "a variant");
    }
}
