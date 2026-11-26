use std::collections::BTreeSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Expr, Lit, LitStr, Result, Token, Type, bracketed, meta::ParseNestedMeta, parenthesized,
    punctuated::Punctuated, spanned::Spanned,
};

use crate::{initial_scalar_literal, set_once};

pub(super) enum FieldDefault {
    Rust,
    Expression(Expr),
}

#[derive(Default)]
pub(super) struct FieldMetadata {
    pub(super) default: Option<FieldDefault>,
    pub(super) skip: Option<Span>,
    pub(super) read_only: Option<Span>,
    pub(super) color_srgb: Option<Span>,
    pub(super) choices: Option<Vec<LitStr>>,
    step: Option<Expr>,
    unit: Option<LitStr>,
    tooltip: Option<LitStr>,
}

impl FieldMetadata {
    pub(super) fn parse(&mut self, meta: &ParseNestedMeta<'_>) -> Result<bool> {
        let span = meta.path.span();
        if meta.path.is_ident("default") {
            let default = if meta.input.peek(Token![=]) {
                FieldDefault::Expression(meta.value()?.parse()?)
            } else {
                FieldDefault::Rust
            };
            set_once(&mut self.default, default, span, "default")?;
        } else if meta.path.is_ident("skip") {
            set_once(&mut self.skip, span, span, "skip")?;
        } else if meta.path.is_ident("read_only") {
            set_once(&mut self.read_only, span, span, "read_only")?;
        } else if meta.path.is_ident("color_srgb") {
            set_once(&mut self.color_srgb, span, span, "color_srgb")?;
        } else if meta.path.is_ident("step") {
            let step: Expr = meta.value()?.parse()?;
            if !matches!(
                initial_scalar_literal(&step),
                Ok(Lit::Int(_) | Lit::Float(_))
            ) {
                return Err(meta.error("step must be a numeric literal"));
            }
            set_once(&mut self.step, step, span, "step")?;
        } else if meta.path.is_ident("unit") {
            set_once(&mut self.unit, meta.value()?.parse()?, span, "unit")?;
        } else if meta.path.is_ident("tooltip") {
            set_once(&mut self.tooltip, meta.value()?.parse()?, span, "tooltip")?;
        } else if meta.path.is_ident("choices") {
            let input;
            if meta.input.peek(Token![=]) {
                let value = meta.value()?;
                bracketed!(input in value);
            } else {
                parenthesized!(input in meta.input);
            }
            let choices = Punctuated::<LitStr, Token![,]>::parse_terminated(&input)?
                .into_iter()
                .collect::<Vec<_>>();
            let mut unique = BTreeSet::new();
            if choices.is_empty()
                || choices.iter().any(|choice| {
                    let choice = choice.value();
                    choice.trim().is_empty() || !unique.insert(choice)
                })
            {
                return Err(meta.error("choices must contain unique nonempty string literals"));
            }
            set_once(&mut self.choices, choices, span, "choices")?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn has_editor_metadata(&self) -> bool {
        self.read_only.is_some()
            || self.color_srgb.is_some()
            || self.choices.is_some()
            || self.step.is_some()
            || self.unit.is_some()
            || self.tooltip.is_some()
    }

    pub(super) fn tokens(&self, support: &TokenStream) -> TokenStream {
        let step = self
            .step
            .as_ref()
            .map(|step| quote!(.with_step(#support::ComponentFloat::new((#step) as f64)?)));
        let unit = self.unit.as_ref().map(|unit| quote!(.with_unit(#unit)));
        let tooltip = self
            .tooltip
            .as_ref()
            .map(|tooltip| quote!(.with_tooltip(#tooltip)));
        let choices = self
            .choices
            .as_ref()
            .map(|choices| quote!(.with_choices([#(#choices),*])));
        quote!(#step #unit #tooltip #choices)
    }
}

pub(super) fn rust_default(default: Option<&FieldDefault>, ty: &Type) -> TokenStream {
    match default {
        Some(FieldDefault::Expression(expr)) => {
            quote!({ let field_default: #ty = #expr; field_default })
        }
        Some(FieldDefault::Rust) | None => quote!(<#ty as ::core::default::Default>::default()),
    }
}
