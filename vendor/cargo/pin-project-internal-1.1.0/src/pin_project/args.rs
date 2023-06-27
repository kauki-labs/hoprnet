use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    spanned::Spanned,
    Attribute, Error, Ident, Result, Token,
};

use super::PIN;
use crate::utils::{ParseBufferExt, SliceExt};

pub(super) fn parse_args(attrs: &[Attribute]) -> Result<Args> {
    // `(__private(<args>))` -> `<args>`
    struct Input(Option<TokenStream>);

    impl Parse for Input {
        fn parse(input: ParseStream<'_>) -> Result<Self> {
            Ok(Self((|| {
                let private = input.parse::<Ident>().ok()?;
                if private == "__private" {
                    input.parenthesized().ok()?.parse::<TokenStream>().ok()
                } else {
                    None
                }
            })()))
        }
    }

    if let Some(attr) = attrs.find("pin_project") {
        bail!(attr, "duplicate #[pin_project] attribute");
    }

    let mut attrs = attrs.iter().filter(|attr| attr.path().is_ident(PIN));

    let prev = if let Some(attr) = attrs.next() {
        (attr, syn::parse2::<Input>(attr.meta.require_list()?.tokens.clone())?.0)
    } else {
        // This only fails if another macro removes `#[pin]`.
        bail!(TokenStream::new(), "#[pin_project] attribute has been removed");
    };

    if let Some(attr) = attrs.next() {
        let (prev_attr, prev_res) = &prev;
        // As the `#[pin]` attribute generated by `#[pin_project]`
        // has the same span as `#[pin_project]`, it is possible
        // that a useless error message will be generated.
        // So, use the span of `prev_attr` if it is not a valid attribute.
        let res = syn::parse2::<Input>(attr.meta.require_list()?.tokens.clone())?.0;
        let span = match (prev_res, res) {
            (Some(_), _) => attr,
            (None, _) => prev_attr,
        };
        bail!(span, "duplicate #[pin] attribute");
    }
    // This `unwrap` only fails if another macro removes `#[pin]` and inserts own `#[pin]`.
    syn::parse2(prev.1.unwrap())
}

pub(super) struct Args {
    /// `PinnedDrop` argument.
    pub(super) pinned_drop: Option<Span>,
    /// `UnsafeUnpin` or `!Unpin` argument.
    pub(super) unpin_impl: UnpinImpl,
    /// `project = <ident>` argument.
    pub(super) project: Option<Ident>,
    /// `project_ref = <ident>` argument.
    pub(super) project_ref: Option<Ident>,
    /// `project_replace [= <ident>]` argument.
    pub(super) project_replace: ProjReplace,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        mod kw {
            syn::custom_keyword!(Unpin);
        }

        /// Parses `= <value>` in `<name> = <value>` and returns value and span of name-value pair.
        fn parse_value(
            input: ParseStream<'_>,
            name: &Ident,
            has_prev: bool,
        ) -> Result<(Ident, TokenStream)> {
            if input.is_empty() {
                bail!(name, "expected `{0} = <identifier>`, found `{0}`", name);
            }
            let eq_token: Token![=] = input.parse()?;
            if input.is_empty() {
                let span = quote!(#name #eq_token);
                bail!(span, "expected `{0} = <identifier>`, found `{0} =`", name);
            }
            let value: Ident = input.parse()?;
            let span = quote!(#name #value);
            if has_prev {
                bail!(span, "duplicate `{}` argument", name);
            }
            Ok((value, span))
        }

        let mut pinned_drop = None;
        let mut unsafe_unpin = None;
        let mut not_unpin = None;
        let mut project = None;
        let mut project_ref = None;
        let mut project_replace_value = None;
        let mut project_replace_span = None;

        while !input.is_empty() {
            if input.peek(Token![!]) {
                let bang: Token![!] = input.parse()?;
                if input.is_empty() {
                    bail!(bang, "expected `!Unpin`, found `!`");
                }
                let unpin: kw::Unpin = input.parse()?;
                let span = quote!(#bang #unpin);
                if not_unpin.replace(span.span()).is_some() {
                    bail!(span, "duplicate `!Unpin` argument");
                }
            } else {
                let token = input.parse::<Ident>()?;
                match &*token.to_string() {
                    "PinnedDrop" => {
                        if pinned_drop.replace(token.span()).is_some() {
                            bail!(token, "duplicate `PinnedDrop` argument");
                        }
                    }
                    "UnsafeUnpin" => {
                        if unsafe_unpin.replace(token.span()).is_some() {
                            bail!(token, "duplicate `UnsafeUnpin` argument");
                        }
                    }
                    "project" => {
                        project = Some(parse_value(input, &token, project.is_some())?.0);
                    }
                    "project_ref" => {
                        project_ref = Some(parse_value(input, &token, project_ref.is_some())?.0);
                    }
                    "project_replace" => {
                        if input.peek(Token![=]) {
                            let (value, span) =
                                parse_value(input, &token, project_replace_span.is_some())?;
                            project_replace_value = Some(value);
                            project_replace_span = Some(span.span());
                        } else if project_replace_span.is_some() {
                            bail!(token, "duplicate `project_replace` argument");
                        } else {
                            project_replace_span = Some(token.span());
                        }
                    }
                    "Replace" => {
                        bail!(
                            token,
                            "`Replace` argument was removed, use `project_replace` argument instead"
                        );
                    }
                    _ => bail!(token, "unexpected argument: {}", token),
                }
            }

            if input.is_empty() {
                break;
            }
            let _: Token![,] = input.parse()?;
        }

        if project.is_some() || project_ref.is_some() {
            if project == project_ref {
                bail!(
                    project_ref,
                    "name `{}` is already specified by `project` argument",
                    project_ref.as_ref().unwrap()
                );
            }
            if let Some(ident) = &project_replace_value {
                if project == project_replace_value {
                    bail!(ident, "name `{}` is already specified by `project` argument", ident);
                } else if project_ref == project_replace_value {
                    bail!(ident, "name `{}` is already specified by `project_ref` argument", ident);
                }
            }
        }

        if let Some(span) = pinned_drop {
            if project_replace_span.is_some() {
                return Err(Error::new(
                    span,
                    "arguments `PinnedDrop` and `project_replace` are mutually exclusive",
                ));
            }
        }
        let project_replace = match (project_replace_span, project_replace_value) {
            (None, _) => ProjReplace::None,
            (Some(span), Some(ident)) => ProjReplace::Named { ident, span },
            (Some(span), None) => ProjReplace::Unnamed { span },
        };
        let unpin_impl = match (unsafe_unpin, not_unpin) {
            (None, None) => UnpinImpl::Default,
            (Some(span), None) => UnpinImpl::Unsafe(span),
            (None, Some(span)) => UnpinImpl::Negative(span),
            (Some(span), Some(_)) => {
                return Err(Error::new(
                    span,
                    "arguments `UnsafeUnpin` and `!Unpin` are mutually exclusive",
                ));
            }
        };

        Ok(Self { pinned_drop, unpin_impl, project, project_ref, project_replace })
    }
}

/// `UnsafeUnpin` or `!Unpin` argument.
#[derive(Clone, Copy)]
pub(super) enum UnpinImpl {
    Default,
    /// `UnsafeUnpin`.
    Unsafe(Span),
    /// `!Unpin`.
    Negative(Span),
}

/// `project_replace [= <ident>]` argument.
pub(super) enum ProjReplace {
    None,
    /// `project_replace`.
    Unnamed {
        span: Span,
    },
    /// `project_replace = <ident>`.
    #[allow(dead_code)] // false positive that fixed in Rust 1.38
    Named {
        span: Span,
        ident: Ident,
    },
}

impl ProjReplace {
    /// Return the span of this argument.
    pub(super) fn span(&self) -> Option<Span> {
        match self {
            Self::None => None,
            Self::Named { span, .. } | Self::Unnamed { span, .. } => Some(*span),
        }
    }

    pub(super) fn ident(&self) -> Option<&Ident> {
        if let Self::Named { ident, .. } = self {
            Some(ident)
        } else {
            None
        }
    }
}
