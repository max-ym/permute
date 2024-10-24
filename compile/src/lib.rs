#![feature(rustc_private)]

use log::*;
use std::{
    fmt, io,
    path::{Path, PathBuf},
};

use compact_str::CompactString;
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

pub struct ProjectContent {
    /// Public types that are accessible in the configuration files.
    pub pub_types: Vec<CompactString>,
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
    pub fn load_from_project_dir(project_dir: &Path) -> Result<Self, ProjectContentError> {
        let rust_files = {
            let mut buf = SmallVec::new();
            collect_files(project_dir, &mut buf)?;
            buf
        };
        let main = fake_main_for(&rust_files);

        let other_files = {
            let maybe_err: Vec<io::Result<RsFile>> = rust_files
            .into_iter()
            .map(|path| {
                let content = std::fs::read_to_string(&path)?;
                Ok(RsFile {
                    path,
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

        let pub_types = pub_types(main, other_files)?;
        Ok(ProjectContent { pub_types })
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
fn fake_main_for(files: &[PathBuf]) -> String {
    debug!("Creating fake main file content");

    let mut main = String::with_capacity(files.len() * 32);
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
    path: PathBuf,
    content: String,
}

struct RsFileLoader(Vec<RsFile>);

impl rustc_span::source_map::FileLoader for RsFileLoader {
    fn file_exists(&self, path: &Path) -> bool {
        self.0.iter().any(|f| f.path == path)
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        self.0
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.clone())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
    }

    fn read_binary_file(&self, path: &Path) -> io::Result<std::rc::Rc<[u8]>> {
        self.0
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.as_bytes().into())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"))
    }
}

fn pub_types(main: String, other_files: Vec<RsFile>) -> Result<Vec<CompactString>, ProjectContentError> {
    use analyze::*;
    use rustc_errors::registry;
    use rustc_hash::FxHashMap;
    use rustc_session::config;
    debug!("Validating project content");

    let out = std::process::Command::new("rustc")
        .arg("--print=sysroot")
        .current_dir(".")
        .output()
        .unwrap();
    let sysroot = std::str::from_utf8(&out.stdout).unwrap().trim();

    let config = rustc_interface::Config {
        // Command line options
        opts: config::Options {
            maybe_sysroot: Some(PathBuf::from(sysroot)),
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
                let hir = tcx.hir();

                if !no_forbidden_loops(hir) {
                    return Err(ProjectContentError::ForbiddenLoops);
                }

                match no_recursion(tcx) {
                    Ok(()) => {
                        info!("No recursion found.");
                    }
                    Err(recursions) => {
                        let r = recursions
                            .iter()
                            .map(|r| {
                                let callee = tcx.def_path_str(r.callee);
                                let caller = tcx.def_path_str(r.caller);
                                self::Recursion {
                                    callee: callee.to_string(),
                                    caller: caller.to_string(),
                                }
                            })
                            .collect();
                        return Err(ProjectContentError::Recursions(r));
                    }
                }

                Ok(types(tcx))
            })
        })
    })
}
