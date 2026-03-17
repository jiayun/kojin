use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Token};

/// Parsed attributes from `#[task(queue = "...", max_retries = N, ...)]`.
#[derive(Default)]
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
