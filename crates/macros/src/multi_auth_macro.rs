use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Data, Fields};

/// `#[derive(MultiAuth)]` — 为 UserType 枚举生成 `config_key()` 方法
///
/// 将每个变体名小写化为 TOML 配置键名：
///   Admin → "admin", Business → "business", Customer → "customer"
pub fn expand(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse2(input).expect("MultiAuth: 解析失败");

    let enum_name = &ast.ident;

    let variants = match &ast.data {
        Data::Enum(data) => &data.variants,
        _ => panic!("MultiAuth 只能应用于 enum"),
    };

    // 确保所有变体都是 Unit variant
    let arms: Vec<_> = variants
        .iter()
        .map(|v| {
            if !matches!(v.fields, Fields::Unit) {
                panic!("MultiAuth 要求所有变体为 unit variant（无字段）");
            }
            let ident = &v.ident;
            let key = v.ident.to_string().to_lowercase();
            quote! { #enum_name::#ident => #key }
        })
        .collect();

    quote! {
        impl #enum_name {
            /// 返回该用户类型在 TOML 配置中的节名（变体名小写）
            pub fn config_key(&self) -> &'static str {
                match self {
                    #(#arms),*
                }
            }
        }
    }
}
