#![feature(rustc_private)]

use log::*;
use std::{
    fmt, io,
    path::{Path, PathBuf},
};

use compact_str::{CompactString, ToCompactString};
use smallvec::SmallVec;

extern crate rustc_driver;
extern crate rustc_error_codes;
extern crate rustc_errors;
extern crate rustc_hash;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_session;
extern crate rustc_span;

extern crate rustc_ast;
extern crate rustc_middle;

pub mod analyze;

pub type ItemId = u32;

#[derive(Debug)]
pub struct ProjectContent {
    /// Public types that are accessible in the configuration files.
    pub pub_types: Vec<ItemPath>,

    /// Public sinks that are accessible in the configuration files.
    /// ID into [pub_types].
    pub sinks: Vec<ItemId>,

    /// Public sources that are accessible in the configuration files.
    /// ID into [pub_types].
    pub sources: Vec<ItemId>,
}

#[derive(Debug)]
pub struct ItemPath {
    pub segments: Vec<CompactString>,
}

impl fmt::Display for ItemPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.segments.join("::"))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectContentError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Forbidden loops in the project")]
    ForbiddenLoops,

    #[error(
        "Recursions found in the project: {}",
        .0
        .iter()
        .map(ToString::to_string).collect::<Vec<_>>().join(", ")
    )]
    Recursions(Vec<Recursion>),
}

#[derive(Debug)]
pub struct Recursion {
    pub callee: String,
    pub caller: String,
}

impl fmt::Display for Recursion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.caller, self.callee)
    }
}

impl ProjectContent {
    /// Load all "rs" files from a project directory. This function does not validate
    /// if the directory has a valid project structure.
    /// Attach to the project content tokens that are required to be present in the main file.
    pub fn load_from_project_dir(
        project_dir: &Path,
        added_content: proc_macro2::TokenStream,
    ) -> Result<Self, ProjectContentError> {
        let rust_files = {
            let mut buf = SmallVec::new();
            collect_files(project_dir, &mut buf)?;
            buf
        };
        let main = fake_main_for(&rust_files, added_content);

        let other_files = {
            let maybe_err: Vec<io::Result<RsFile>> = rust_files
                .into_iter()
                .map(|path| {
                    let content = std::fs::read_to_string(&path)?;
                    let relpath = path.strip_prefix(project_dir).expect(
                        "path is inside the project directory and should have folder's prefix",
                    );
                    Ok(RsFile {
                        path: relpath.to_path_buf(),
                        content,
                    })
                })
                .collect();

            let mut arr: SmallVec<[_; 64]> = SmallVec::with_capacity(maybe_err.len());
            for file in maybe_err {
                match file {
                    Ok(f) => arr.push(f),
                    Err(e) => return Err(e.into()),
                }
            }

            arr.into_vec()
        };

        run_analyze(main, other_files)
    }

    pub fn sinks(&self) -> impl Iterator<Item = &ItemPath> {
        self.sinks.iter().map(|id| &self.pub_types[*id as usize])
    }

    pub fn sources(&self) -> impl Iterator<Item = &ItemPath> {
        self.sources.iter().map(|id| &self.pub_types[*id as usize])
    }
}

/// Collect all files that have 'rs' extension inside this and children directories.
fn collect_files(dir: &Path, buf: &mut SmallVec<[PathBuf; 64]>) -> Result<(), ProjectContentError> {
    debug!("Collecting 'rs' files at level: {}", dir.display());

    for entry in dir.read_dir()? {
        let entry = entry?;
        let path = entry.path();
        let meta = entry.metadata()?;
        if meta.is_dir() {
            collect_files(&path, buf)?;
        } else if meta.is_file() {
            trace!("Checking file: {}", path.display());
            if let Some(ext) = path.extension() {
                if ext == "rs" {
                    trace!("Found 'rs' file: {}", path.display());
                    buf.push(path);
                }
            }
        }
    }

    trace!(
        "Collected {} 'rs' files at level: {}",
        buf.len(),
        dir.display()
    );
    Ok(())
}

/// Create a fake main file content that declares all found files as modules and serves as a crate
/// root.
fn fake_main_for(files: &[PathBuf], added_content: proc_macro2::TokenStream) -> String {
    debug!("Creating fake main file content");

    let added_content = added_content.to_string();
    let mut main = String::with_capacity(files.len() * 32 + added_content.len());
    main.push_str("extern crate serde_derive;\n");
    main.push_str(added_content.as_str());
    for file in files {
        let module = file
            .file_stem()
            .expect("Path was created in function that already operated on the file name to find 'rs' extension")
            .to_string_lossy();
        trace!("Declaring module: {module}");
        main.push_str(&format!("pub mod {module};\n"));
    }
    main.shrink_to_fit();

    debug!("Created fake main file content: {} bytes", main.len());
    main
}

struct RsFile {
    /// Relative path to a rust file in the project directory.
    path: PathBuf,
    content: String,
}

struct RsFileLoader(Vec<RsFile>);

impl rustc_span::source_map::FileLoader for RsFileLoader {
    fn file_exists(&self, path: &Path) -> bool {
        trace!("Checking if file exists: {}", path.display());
        self.0.iter().any(|f| f.path == path)
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        trace!("Reading file: {}", path.display());
        self.0
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.clone())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
    }

    fn read_binary_file(&self, path: &Path) -> io::Result<std::rc::Rc<[u8]>> {
        trace!("Reading binary file: {}", path.display());
        self.0
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.as_bytes().into())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
    }
}

fn run_analyze(
    main: String,
    other_files: Vec<RsFile>,
) -> Result<ProjectContent, ProjectContentError> {
    use analyze::*;
    use rustc_errors::registry;
    use rustc_hash::FxHashMap;
    use rustc_session::{
        config,
        utils::{CanonicalizedPath, NativeLib, NativeLibKind},
    };
    debug!("Validating project content");

    let out = std::process::Command::new("rustc")
        .arg("--print=sysroot")
        .current_dir(".")
        .output()
        .unwrap();
    let sysroot = std::str::from_utf8(&out.stdout).unwrap().trim();

    let search_path_dir: std::path::PathBuf = "../compile/rlibs".into();
    assert!(
        search_path_dir.exists(),
        "search path directory does not exist"
    );

    let externs = {
        const HASH_LEN: usize = 18;

        let mut map = std::collections::BTreeMap::new();

        for entry in std::fs::read_dir(&search_path_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let filename = path.file_name().unwrap();
            let ext = path.extension().unwrap_or_default();
            let filename = filename.to_str().unwrap();
            let ext = ext.to_str().unwrap_or_default();
            if ext == "rlib" || ext == "so" {
                let skip_prefix = "lib".len();

                // Strip off the extension, hash suffix and lib prefix.
                let strip = &filename[skip_prefix..filename.len() - HASH_LEN - ext.len()];
                debug!("Registering extern library: {strip} - ({filename})");

                // Upsert the entry.
                use std::collections::btree_map::Entry;
                match map.entry(strip.to_string()) {
                    Entry::Vacant(e) => {
                        let cfg = config::ExternEntry {
                            location: config::ExternLocation::ExactPaths({
                                let mut tree = std::collections::BTreeSet::new();
                                tree.insert(CanonicalizedPath::new(&path));
                                tree
                            }),
                            is_private_dep: true,
                            add_prelude: true,
                            nounused_dep: false,
                            force: false,
                        };
                        e.insert(cfg);
                    }
                    Entry::Occupied(mut e) => {
                        if let config::ExternLocation::ExactPaths(location) =
                            &mut e.get_mut().location
                        {
                            location.insert(CanonicalizedPath::new(&path));
                        } else {
                            unreachable!("location is always ExactPaths");
                        }
                    }
                }
            } else {
                trace!("Skipping file: {filename}");
            }
        }

        config::Externs::new(map)
    };

    let config = rustc_interface::Config {
        // Command line options
        opts: config::Options {
            maybe_sysroot: Some(PathBuf::from(sysroot)),
            externs,
            edition: rustc_span::edition::Edition::Edition2021,
            ..config::Options::default()
        },
        // cfg! configuration in addition to the default ones
        crate_cfg: Vec::new(),
        crate_check_cfg: Vec::new(),
        output_dir: None,
        output_file: None,
        input: config::Input::Str {
            name: rustc_span::FileName::Custom("lib.rs".into()),
            input: main,
        },
        file_loader: Some(Box::new(RsFileLoader(other_files))),
        locale_resources: rustc_driver::DEFAULT_LOCALE_RESOURCES,
        lint_caps: FxHashMap::default(),
        // This is a callback from the driver that is called when [`ParseSess`] is created.
        psess_created: None,
        // This is a callback from the driver that is called when we're registering lints;
        // it is called during plugin registration when we have the LintStore in a non-shared state.
        //
        // Note that if you find a Some here you probably want to call that function in the new
        // function being registered.
        register_lints: None,
        // This is a callback from the driver that is called just after we have populated
        // the list of queries.
        //
        // The second parameter is local providers and the third parameter is external providers.
        override_queries: None,
        // Registry of diagnostics codes.
        registry: registry::Registry::new(rustc_errors::codes::DIAGNOSTICS),
        make_codegen_backend: None,
        expanded_args: Vec::new(),
        ice_file: None,
        hash_untracked_state: None,
        using_internal_features: std::sync::Arc::default(),
    };

    rustc_interface::run_compiler(config, |compiler| {
        compiler.enter(|queries| {
            queries.global_ctxt().unwrap().enter(|tcx| {
                info!("Started compiler, entered global context");
                // let hir = tcx.hir();

                // info!("Run check for forbidden loops");
                // if !no_forbidden_loops(hir) {
                //     return Err(ProjectContentError::ForbiddenLoops);
                // }

                // info!("Run check for recursion");
                // match no_recursion(tcx) {
                //     Ok(()) => {
                //         info!("No recursion found.");
                //     }
                //     Err(recursions) => {
                //         let r = recursions
                //             .iter()
                //             .map(|r| {
                //                 let callee = tcx.def_path_str(r.callee);
                //                 let caller = tcx.def_path_str(r.caller);
                //                 self::Recursion {
                //                     callee: callee.to_string(),
                //                     caller: caller.to_string(),
                //                 }
                //             })
                //             .collect();
                //         return Err(ProjectContentError::Recursions(r));
                //     }
                // }

                let pub_types = type_ids(tcx);
                let sinks_and_sources = {
                    let mut val = SinksAndSources::collect_from(tcx);
                    val.filter_not_in(pub_types.as_slice());
                    val
                };
                info!("Sinks and sources collected");

                let sinks = sinks_and_sources
                    .sinks
                    .iter()
                    .map(|id| {
                        pub_types.iter().position(|v| v == id).expect(
                            "should be present as we've got the IDs in the same compilation process",
                        ) as ItemId
                    })
                    .collect();
                let sources = sinks_and_sources
                    .sources
                    .iter()
                    .map(|id| {
                        pub_types.iter().position(|v| v == id).expect(
                            "should be present as we've got the IDs in the same compilation process",
                        ) as ItemId
                    })
                    .collect();
                debug!("Collected type IDs of sinks and sources from the compiler context");

                let pub_type_paths = pub_types
                    .into_iter()
                    .map(|id| {
                        let path = tcx.def_path(id);
                        let segments = path.data.iter().map(|s| s.to_compact_string()).collect();
                        ItemPath { segments }
                    })
                    .collect();
                info!("Public types mapped to paths");

                info!("Project content validated");
                Ok(ProjectContent {
                    pub_types: pub_type_paths,
                    sinks,
                    sources,
                })
            })
        })
    })
}
