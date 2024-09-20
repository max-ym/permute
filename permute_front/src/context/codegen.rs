use super::*;
use proc_macro2::{Span, TokenStream};
use quote::{quote, TokenStreamExt};

pub fn gen_main(ctx: &Ctx) -> TokenStream {
    let name = ctx.name();
    let explain = ctx.explain();
    let mut tokens = quote! {
        use log::*;

        const NAME: &'static str = #name;
        const EXPLAIN: Option<&'static str> = #explain;

        fn main() {
            permute::sys::init_logger();

            info!("{NAME}");
            if let Some(explain) = EXPLAIN {
                info!("{explain}");
            }

            // TODO: Implement the main logic.
        }
    };
    for src in ctx.sources() {
        tokens.append_all(gen_data_src(src));
    }
    for sink in ctx.sinks() {
        tokens.append_all(gen_data_sink(sink));
    }
    tokens
}

fn gen_data_src(src: &DataSource) -> TokenStream {
    let struc = gen_data_src_struc(src);
    let impls = gen_data_src_impls(src);
    quote! {
        #struc
        #impls
    }
}

fn gen_data_src_struc(src: &DataSource) -> TokenStream {
    let src_name = src.src_name();
    let filters = src.filters().iter().map(|(k, v)| {
        let name = k.ident();
        let ty = v.ty();
        quote! {
            #name: #ty
        }
    });
    quote! {
        #[derive(Debug)]
        pub struct #src_name {
            #(#filters),*
        }
    }
}

fn gen_data_src_impls(src: &DataSource) -> TokenStream {
    let src_name = src.src_name();
    let impls = src.filters().iter().map(|(name, _)| {
        let fmt_ty = filter_ty(src, name);
        quote! {
            pub fn #name(&self) -> #fmt_ty {
                #fmt_ty(&self.#name)
            }
        }
    });
    let fmts = src.filters().iter().map(|(name, v)| {
        let ty = v.ty();
        let fmt_ty = filter_ty(src, name);
        let explain = v.explain();
        let default = if let Some(default) = v.default() {
            quote! {
                impl Default for #fmt_ty {
                    fn default() -> Self {
                        Self(#default)
                    }
                }
            }
        } else {
            quote! {}
        };
        let check = v.checks().iter().map(|check| {
            let expr = check.expr();
            let explain = check.explain();
            quote! {
                if !#expr {
                    return Err(permute::sys::FilterCheckErr<#fmt_ty>::new(#explain));
                }
            }
        });
        quote! {
            #[derive(Debug)]
            pub struct #fmt_ty(pub #ty);

            impl #fmt_ty {
                pub fn explain() -> Option<&'static str> {
                    #explain
                }

                pub fn check(&self) -> Result<(), permute::sys::FilterCheckErr<#fmt_ty>> {
                    #(#check)*
                    Ok(())
                }
            }

            #default
        }
    });

    quote! {
        impl #src_name {
            #(#impls)*
        }
        #(#fmts)*
    }
}

fn filter_ty(src: &DataSource, filter: &str) -> syn::Ident {
    format!("{}_{filter}", src.src_name()).ident()
}

fn gen_data_sink(sink: &Sink) -> TokenStream {
    todo!()
}

trait StrExt {
    fn ident(&self) -> syn::Ident;
}

impl StrExt for str {
    fn ident(&self) -> syn::Ident {
        syn::Ident::new(self, Span::call_site())
    }
}
