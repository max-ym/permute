use super::*;
use proc_macro2::{Span, TokenStream};
use quote::{quote, TokenStreamExt};

pub fn gen_main(ctx: &Ctx) -> TokenStream {
    info!("Generator main function started");

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

    info!("Generating sources");
    for src in ctx.sources() {
        tokens.append_all(gen_data_src(src));
    }
    info!("Generating sinks");
    for sink in ctx.sinks() {
        tokens.append_all(gen_data_sink(sink));
    }

    info!("Generator main function finished");
    tokens
}

/// Generate the data source struct and impls.
pub fn gen_data_src(src: &DataSource) -> TokenStream {
    let struc = gen_data_src_struc(src);
    let impls = gen_data_src_impls(src);
    let uses = use_tree_tokens(src.uses());
    let mod_name = src.name().underscored_ident();
    quote! {
        mod #mod_name {
            #uses
            #struc
            #impls
        }
        pub use #mod_name::*;
    }
}

fn use_tree_tokens(uses: &[syn::UseTree]) -> TokenStream {
    quote! {
        #(use #uses;)*
    }
}

fn gen_data_src_struc(src: &DataSource) -> TokenStream {
    let src_name = src.name().ident();
    info!("Generating data source `{src_name}` struct");

    let filters = src.filters().iter().map(|(k, v)| {
        let name = k.ident();
        let ty = v.ty();
        quote! {
            #name: #ty
        }
    });
    quote! {
        #[allow(non_camel_case_types)]
        #[derive(Debug)]
        pub struct #src_name {
            #(#filters),*
        }
    }
}

fn gen_data_src_impls(src: &DataSource) -> TokenStream {
    let src_name = src.name().ident();
    info!("Generating data source `{src_name}` impls");

    let impls = src.filters().iter().map(|(name, _)| {
        let fmt_ty = filter_ty(src, name);
        let name = name.ident();
        // Generate getter for the filter.
        quote! {
            pub fn #name(&self) -> #fmt_ty {
                #fmt_ty(&self.#name)
            }
        }
    });
    let fmts = src.filters().iter().map(|(name, v)| {
        let ty = v.ty();
        let fmt_ty = filter_ty(src, name);
        let explain = if let Some(explain) = v.explain() {
            quote! {
                Some(#explain)
            }
        } else {
            quote! {
                None
            }
        };
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
            #[allow(non_camel_case_types)]
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
    format!("{}_{filter}", src.name()).ident()
}

fn sink_param_ty(sink: &Sink, param: &str) -> syn::Ident {
    format!("{}_{param}", sink.name()).ident()
}

/// Generate the data sink struct and impls.
pub fn gen_data_sink(sink: &Sink) -> TokenStream {
    let struc = gen_data_sink_struc(sink);
    let impls = gen_data_sink_impls(sink);
    let uses = use_tree_tokens(sink.uses());
    let mod_name = sink.name().underscored_ident();
    quote! {
        mod #mod_name {
            #uses
            #struc
            #impls
        }
        pub use #mod_name::*;
    }
}

fn gen_data_sink_struc(sink: &Sink) -> TokenStream {
    let sink_name = sink.name().ident();
    info!("Generating data sink `{sink_name}` struct");

    let impls = sink.params().iter().map(|(k, v)| {
        let name = sink_param_ty(sink, k);
        let ty = v.ty();
        let default = if let Some(default) = v.default() {
            quote! {
                impl Default for #name {
                    fn default() -> Self {
                        Self(#default)
                    }
                }
            }
        } else {
            quote! {}
        };

        let checks = v.checks().iter().map(|check| {
            let expr = check.expr();
            let explain = check.explain();
            quote! {
                if !#expr {
                    return Err(permute::sys::FilterCheckErr::new(#explain));
                }
            }
        });

        quote! {
            #[allow(non_camel_case_types)]
            #[derive(Debug)]
            pub struct #name(pub #ty);

            impl std::ops::Deref for #name {
                type Target = #ty;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl #name {
                pub fn check(&self) -> Result<(), permute::sys::FilterCheckErr<#name>> {
                    #(#checks)*
                    Ok(())
                }
            }

            #default
        }
    });

    let params = sink.params().iter().map(|(k, _)| {
        let name = sink_param_ty(sink, k);
        let ident = k.ident();
        quote! {
            #ident: #name
        }
    });

    quote! {
        #[allow(non_camel_case_types)]
        #[derive(Debug)]
        pub struct #sink_name {
            #(#params),*
        }

        #(#impls)*
    }
}

fn gen_data_sink_impls(sink: &Sink) -> TokenStream {
    let sink_name = sink.name().ident();
    info!("Generating data sink `{sink_name}` impls");

    let impls = sink.params().iter().map(|(name, param)| {
        let sink_ty_name = sink_param_ty(sink, name);
        let name = name.ident();
        let ty = param.ty();
        // Generate getter for the filter.
        quote! {
            pub fn #name(&self) -> &#ty {
                &self.#sink_ty_name.0
            }
        }
    });

    quote! {
        impl #sink_name {
            #(#impls)*
        }
    }
}

trait StrExt {
    fn ident(&self) -> syn::Ident;

    fn underscored_ident(&self) -> syn::Ident;
}

impl StrExt for str {
    fn ident(&self) -> syn::Ident {
        syn::Ident::new(self, Span::call_site())
    }

    fn underscored_ident(&self) -> syn::Ident {
        syn::Ident::new(&format!("_{}", self), Span::call_site())
    }
}

pub fn trace_printall(tokens: &TokenStream) {
    if log::STATIC_MAX_LEVEL > log::Level::Trace.to_level_filter() {
        return;
    }

    use rust_format::{Formatter, PrettyPlease};

    let fmt = PrettyPlease::default()
        .format_str(tokens.to_string())
        .unwrap();
    trace!("{fmt}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printall() {
        crate::setup_logger();

        let ctx = crate::yaml::load::tests::do_load_project();
        let tokens = gen_main(&ctx);
        trace_printall(&tokens);
    }
}
