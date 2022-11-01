use proc_macro::{Literal, TokenStream, TokenTree};
use syn::{parse_macro_input, LitStr};

extern crate proc_macro;

#[proc_macro]
pub fn encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let input_string = input.value();

    let new_string = urlencoding::encode(&input_string);

    TokenStream::from(TokenTree::Literal(Literal::string(new_string.as_ref())))
}
