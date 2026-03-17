use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn task(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // TODO: implement task macro
    item
}
