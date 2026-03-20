use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Token};

/// Parsed attributes from `#[task(queue = "...", max_retries = N, ...)]`.
#[derive(Debug, Default)]
pub struct TaskAttr {
    pub queue: Option<String>,
    pub max_retries: Option<u32>,
}

impl Parse for TaskAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attr = TaskAttr::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "queue" => {
                    let value: LitStr = input.parse()?;
                    attr.queue = Some(value.value());
                }
                "max_retries" => {
                    let value: Expr = input.parse()?;
                    if let Expr::Lit(lit) = &value {
                        if let syn::Lit::Int(int) = &lit.lit {
                            attr.max_retries = Some(int.base10_parse()?);
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown attribute: {other}"),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(attr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn parse_empty() {
        let attr: TaskAttr = syn::parse2(quote! {}).unwrap();
        assert_eq!(attr.queue, None);
        assert_eq!(attr.max_retries, None);
    }

    #[test]
    fn parse_queue_only() {
        let attr: TaskAttr = syn::parse2(quote! { queue = "emails" }).unwrap();
        assert_eq!(attr.queue, Some("emails".into()));
        assert_eq!(attr.max_retries, None);
    }

    #[test]
    fn parse_max_retries_only() {
        let attr: TaskAttr = syn::parse2(quote! { max_retries = 5 }).unwrap();
        assert_eq!(attr.queue, None);
        assert_eq!(attr.max_retries, Some(5));
    }

    #[test]
    fn parse_both() {
        let attr: TaskAttr = syn::parse2(quote! { queue = "q", max_retries = 10 }).unwrap();
        assert_eq!(attr.queue, Some("q".into()));
        assert_eq!(attr.max_retries, Some(10));
    }

    #[test]
    fn parse_unknown_key_errors() {
        let result = syn::parse2::<TaskAttr>(quote! { unknown = "x" });
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown attribute"), "got: {err}");
    }
}
