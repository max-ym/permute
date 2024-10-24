#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_error_codes;
extern crate rustc_errors;
extern crate rustc_hash;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_session;
extern crate rustc_span;

extern crate rustc_middle;

use std::{path, process, str, sync::Arc};

use rustc_errors::registry;
use rustc_hash::FxHashMap;
use rustc_session::config;

use compile::analyze::*;

fn main() {
    simple_logger::SimpleLogger::new()
        .with_colors(true)
        .with_level(log::LevelFilter::Info)
        .with_module_level("permute", log::LevelFilter::Trace)
        .without_timestamps()
        .init()
        .unwrap();

    let out = process::Command::new("rustc")
        .arg("--print=sysroot")
        .current_dir(".")
        .output()
        .unwrap();
    let sysroot = str::from_utf8(&out.stdout).unwrap().trim();
    let config = rustc_interface::Config {
        // Command line options
        opts: config::Options {
            maybe_sysroot: Some(path::PathBuf::from(sysroot)),
            ..config::Options::default()
        },
        // cfg! configuration in addition to the default ones
        crate_cfg: Vec::new(),       // FxHashSet<(String, Option<String>)>
        crate_check_cfg: Vec::new(), // CheckCfg
        input: config::Input::Str {
            name: rustc_span::FileName::Custom("main.rs".into()),
            input: r#"
static HELLO: &str = "Hello, world!";
fn main() {
    println!("{HELLO}");
}

fn i(i: impl std::iter::Iterator<Item = i32>) {
    for i in i {
        println!("{i}");
    }
}

fn a() {
    b();
}

fn b() {
    c();
}

fn c() {
    d();
}

fn d() {
    a();
}

pub mod m {
    pub struct S;
}
    
"#
            .into(),
        },
        output_dir: None,  // Option<PathBuf>
        output_file: None, // Option<PathBuf>
        file_loader: None, // Option<Box<dyn FileLoader + Send + Sync>>
        locale_resources: rustc_driver::DEFAULT_LOCALE_RESOURCES,
        lint_caps: FxHashMap::default(), // FxHashMap<lint::LintId, lint::Level>
        // This is a callback from the driver that is called when [`ParseSess`] is created.
        psess_created: None, //Option<Box<dyn FnOnce(&mut ParseSess) + Send>>
        // This is a callback from the driver that is called when we're registering lints;
        // it is called during plugin registration when we have the LintStore in a non-shared state.
        //
        // Note that if you find a Some here you probably want to call that function in the new
        // function being registered.
        register_lints: None, // Option<Box<dyn Fn(&Session, &mut LintStore) + Send + Sync>>
        // This is a callback from the driver that is called just after we have populated
        // the list of queries.
        //
        // The second parameter is local providers and the third parameter is external providers.
        override_queries: None, // Option<fn(&Session, &mut ty::query::Providers<'_>, &mut ty::query::Providers<'_>)>
        // Registry of diagnostics codes.
        registry: registry::Registry::new(rustc_errors::codes::DIAGNOSTICS),
        make_codegen_backend: None,
        expanded_args: Vec::new(),
        ice_file: None,
        hash_untracked_state: None,
        using_internal_features: Arc::default(),
    };
    rustc_interface::run_compiler(config, |compiler| {
        compiler.enter(|queries| {
            // Analyze the program and inspect the types of definitions.
            queries.global_ctxt().unwrap().enter(|tcx| {
                let hir = tcx.hir();
                let none_forbidden_loops = no_forbidden_loops(hir);
                if none_forbidden_loops {
                    println!("No forbidden loops found.");
                } else {
                    println!("Forbidden loops found.");
                }

                let maybe_recursion = no_recursion(tcx);
                if let Err(recursions) = maybe_recursion {
                    println!("Recursion found.");
                    for r in recursions {
                        let caller = tcx.hir().def_path(r.caller).to_string_no_crate_verbose();
                        let callee = tcx.hir().def_path(r.callee).to_string_no_crate_verbose();
                        println!("Caller: {caller}, Callee: {callee}");
                    }
                } else {
                    println!("No recursion found.");
                }

                // Print out item names.
                for item in types(tcx) {
                    println!("Public: {item}");
                }
            })
        });
    });
}
