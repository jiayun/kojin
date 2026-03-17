use proc_macro::TokenStream;
use syn::parse_macro_input;

mod codegen;
mod task_attr;

/// Derive a task struct and `Task` impl from an async function.
///
/// # Example
/// ```ignore
/// #[task(queue = "emails", max_retries = 5)]
/// async fn send_email(ctx: &TaskContext, to: String, subject: String) -> TaskResult<()> {
///     // ...
///     Ok(())
/// }
/// ```
///
/// This generates:
/// - `struct SendEmail { pub to: String, pub subject: String }`
/// - `impl Task for SendEmail { ... }`
/// - `SendEmail::new(to, subject)`
#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as task_attr::TaskAttr);
    let func = parse_macro_input!(item as syn::ItemFn);

    match codegen::generate_task(&attr, &func) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
