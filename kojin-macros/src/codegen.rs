use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

use crate::task_attr::TaskAttr;

/// Generate a struct + Task impl from an async function.
pub fn generate_task(attr: &TaskAttr, func: &ItemFn) -> syn::Result<TokenStream> {
    let fn_name = &func.sig.ident;
    let vis = &func.vis;

    // Convert snake_case fn name to PascalCase struct name
    let struct_name = format_ident!("{}", to_pascal_case(&fn_name.to_string()));

    // Validate: must be async
    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "#[task] function must be async",
        ));
    }

    // Parse parameters: first must be &TaskContext, rest become struct fields
    let mut params = func.sig.inputs.iter();

    // First param should be ctx: &TaskContext
    let first = params.next().ok_or_else(|| {
        syn::Error::new_spanned(
            &func.sig,
            "#[task] function must have at least one parameter: ctx: &TaskContext",
        )
    })?;

    // Validate first param is a reference (ctx: &TaskContext)
    match first {
        FnArg::Typed(pat_type) => {
            if let Type::Reference(_) = pat_type.ty.as_ref() {
                // ok
            } else {
                return Err(syn::Error::new_spanned(
                    pat_type,
                    "first parameter must be a reference to TaskContext (e.g., ctx: &TaskContext)",
                ));
            }
        }
        FnArg::Receiver(_) => {
            return Err(syn::Error::new_spanned(
                first,
                "#[task] function cannot have self parameter",
            ));
        }
    }

    // Collect remaining params as struct fields
    let mut field_names = Vec::new();
    let mut field_types = Vec::new();

    for param in params {
        match param {
            FnArg::Typed(pat_type) => {
                if let Pat::Ident(ident) = pat_type.pat.as_ref() {
                    field_names.push(ident.ident.clone());
                    field_types.push(pat_type.ty.as_ref().clone());
                } else {
                    return Err(syn::Error::new_spanned(
                        pat_type,
                        "expected named parameter",
                    ));
                }
            }
            FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(param, "unexpected self parameter"));
            }
        }
    }

    // Task name: use fn name as-is
    let task_name = fn_name.to_string();

    // Queue
    let queue = attr.queue.as_deref().unwrap_or("default");

    // Max retries
    let max_retries = attr.max_retries.unwrap_or(3);

    // Return type
    let output_type = match &func.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => {
            // Expect TaskResult<T> — extract T
            // We'll just use the inner type directly if it's a Result-like
            extract_result_inner(ty)
        }
    };

    // The function body
    let body = &func.block;

    let output = quote! {
        #[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
        #vis struct #struct_name {
            #( pub #field_names: #field_types, )*
        }

        #[::async_trait::async_trait]
        impl ::kojin_core::Task for #struct_name {
            const NAME: &'static str = #task_name;
            const QUEUE: &'static str = #queue;
            const MAX_RETRIES: u32 = #max_retries;

            type Output = #output_type;

            async fn run(&self, ctx: &::kojin_core::TaskContext) -> ::kojin_core::TaskResult<Self::Output> {
                // Destructure self into local variables
                let Self { #( ref #field_names, )* } = *self;
                // Shadow with owned clones for the body
                #( let #field_names = #field_names.clone(); )*

                #body
            }
        }

        impl #struct_name {
            pub fn new(#( #field_names: #field_types ),*) -> Self {
                Self { #( #field_names, )* }
            }
        }
    };

    Ok(output)
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_simple() {
        assert_eq!(to_pascal_case("my_task"), "MyTask");
    }

    #[test]
    fn pascal_case_single_word() {
        assert_eq!(to_pascal_case("task"), "Task");
    }

    #[test]
    fn pascal_case_multiple_underscores() {
        assert_eq!(to_pascal_case("my_long_task_name"), "MyLongTaskName");
    }

    #[test]
    fn pascal_case_empty() {
        assert_eq!(to_pascal_case(""), "");
    }
}

fn extract_result_inner(ty: &Type) -> TokenStream {
    // Try to extract T from TaskResult<T> or Result<T, ...>
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let name = segment.ident.to_string();
            if name == "TaskResult" || name == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return quote! { #inner };
                    }
                }
            }
        }
    }
    // Fallback: use the type as-is
    quote! { #ty }
}
