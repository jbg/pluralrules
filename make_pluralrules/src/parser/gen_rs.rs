//! gen_rs is a Rust code generator for expression representations of CLDR plural rules.

use std::{
    collections::BTreeMap,
    fmt::Write,
    str::{self, FromStr},
};

use phf_codegen::Map;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

use super::plural_category::PluralCategory;


/// Generates the complete TokenStream for the generated Rust code. This wraps the head and tail of the .rs file around the generated CLDR expressions.
pub fn gen_fn(
    streams: BTreeMap<String, Vec<(String, TokenStream)>>,
    locales: BTreeMap<String, Vec<String>>,
    vr: &str,
) -> TokenStream {
    let ignore_noncritical_errors = quote! {
        #![allow(unused_variables, unused_parens)]
        #![cfg_attr(feature = "cargo-clippy", allow(clippy::float_cmp))]
        #![cfg_attr(feature = "cargo-clippy", allow(clippy::unreadable_literal))]
        #![cfg_attr(feature = "cargo-clippy", allow(clippy::nonminimal_bool))]
    };
    let use_statements = quote! {
        use matches::matches;
        use phf;
        use super::operands::PluralOperands;
        use super::{PluralCategory, PluralRuleType};
    };
    let plural_function = quote! { pub type PluralRule = fn(&PluralOperands) -> PluralCategory; };
    let num: isize = vr.parse().unwrap();
    let ver = Literal::u64_unsuffixed(num as u64);
    let version = quote! { pub static CLDR_VERSION: usize = #ver; };
    let get_locales = gen_get_locales(locales);
    let head = quote! { #ignore_noncritical_errors #use_statements #plural_function #version #get_locales };
    let mut tokens = Vec::<TokenStream>::new();
    for (pr_type, stream) in streams {
        tokens.push(create_gen_pr_type_fn(&pr_type, stream));
    }
    let filling = quote! { #(#tokens),* };
    let get_pr_function = quote! { #[cfg_attr(tarpaulin, skip)] pub fn get_pr(lang_code: &str, pr_type: PluralRuleType) -> Result<PluralRule, ()> {match pr_type { #filling }} };
    quote! { #head #get_pr_function }
}

// Function writes the get locales function
fn gen_get_locales(locales: BTreeMap<String, Vec<String>>) -> TokenStream {
    let mut tokens = Vec::<TokenStream>::new();

    for (pr_type, locales) in locales {
        let match_name = match pr_type.as_str() {
            "cardinal" => quote! { PluralRuleType::CARDINAL },
            "ordinal" => quote! { PluralRuleType::ORDINAL },
            _ => panic!("Unknown plural rule type"),
        };
        let locales_tokens = quote! { &[ #(#locales),* ] };
        tokens.push(quote! { #match_name => #locales_tokens });
    }
    quote! { #[cfg_attr(tarpaulin, skip)] pub fn get_locales(pr_type: PluralRuleType) -> &'static [&'static str] { match pr_type { #(#tokens),* } } }
}

// Function wraps all match statements for plural rules in a match for ordinal and cardinal rules
fn create_gen_pr_type_fn(pr_type: &str, streams: Vec<(String, TokenStream)>) -> TokenStream {
    let mut map = Map::new();
    for (lang, func) in streams.iter() {
        map.entry(lang.as_str(), func.to_string().as_str());
    }
    let mut map_str = String::new();
    write!(map_str, "{}", map.build()).expect("unexpected failure building phf map");
    let map = TokenStream::from_str(&map_str).expect("phf-codegen returned invalid Rust!");

    let match_name = match pr_type {
        "cardinal" => quote! { PluralRuleType::CARDINAL },
        "ordinal" => quote! { PluralRuleType::ORDINAL },
        _ => panic!("Unknown plural rule type"),
    };
    quote! {
        #match_name => {
            static LANGUAGES: phf::Map<&'static str, PluralRule> = #map;
            LANGUAGES.get(lang_code).cloned().ok_or(())
        }
    }
}

// Function wraps an expression in a match statement for plural category
fn create_return(cat: PluralCategory, exp: &TokenStream) -> TokenStream {
    match cat {
        PluralCategory::ZERO => quote! {if #exp { PluralCategory::ZERO } },
        PluralCategory::ONE => quote! {if #exp { PluralCategory::ONE } },
        PluralCategory::TWO => quote! {if #exp { PluralCategory::TWO } },
        PluralCategory::FEW => quote! {if #exp { PluralCategory::FEW } },
        PluralCategory::MANY => quote! {if #exp { PluralCategory::MANY } },
        PluralCategory::OTHER => quote! { { PluralCategory::OTHER } },
    }
}

/// Generates the closures that comprise the majority of the generated rust code.
///
/// These statements are the expression representations of the CLDR plural rules.
pub fn gen_mid(lang: &str, pluralrule_set: &[(PluralCategory, TokenStream)]) -> TokenStream {
    // make pluralrule_set iterable
    let mut iter = pluralrule_set.iter();
    let rule_name = format!("rule_{}", lang.replace("-", "_").to_lowercase());
    let rule_name = Ident::new(&rule_name, Span::call_site());

    let queued = iter.next();
    let rule_tokens = match queued {
        Some(pair) => {
            // instantiate tokenstream for folded match rules
            let mut tokens = create_return(pair.0, &pair.1);

            // add all tokens to token stream, separated by commas
            for pair in iter {
                let condition = create_return(pair.0, &pair.1);
                tokens = quote! { #tokens else #condition };
            }
            tokens = quote! { #tokens else { PluralCategory::OTHER } };
            tokens
        }
        None => quote! { { PluralCategory::OTHER }  },
    };

    // We can't use a closure here because closures can't get rvalue
    // promoted to statics. They may in the future.
    quote! {
        {
            fn #rule_name(po: &PluralOperands) -> PluralCategory {
                #rule_tokens
            };
            #rule_name
        }
    }
}
