use std::path::PathBuf;

use compact_str::CompactString;
use log::*;
use smallvec::{smallvec, SmallVec};

use crate::context::{Ctx, ParamKey};
use crate::yaml::hir;
use crate::yaml::v01;

/// Load input files from a project directory and create the context with them.
pub struct LoadProjectDir<'a> {
    /// Path to the directory with input files.
    pub path: &'a std::path::Path,
}

/// Error during loading of the project.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("Project path does not exist. {0}")]
    PathDoesNotExist(std::path::PathBuf),

    #[error("Path is not a directory: {0}")]
    PathIsNotDir(std::path::PathBuf),

    #[error(transparent)]
    MainFileError(#[from] MainLoadError),

    #[error(transparent)]
    Yaml(#[from] crate::yaml::Error),

    #[error("Error listing other YAML files. {0}")]
    DirList(std::io::Error),

    #[error(transparent)]
    MainHir(#[from] hir::MainError),

    #[error(transparent)]
    SinkHir(#[from] hir::SinkError),

    #[error(transparent)]
    SourceHir(#[from] hir::SourceError),

    #[error(transparent)]
    EmptyName(#[from] crate::context::EmptyNameError),

    #[error(transparent)]
    AddSink(#[from] crate::context::AddSinkErr),

    #[error(transparent)]
    AddSource(#[from] crate::context::AddSourceErr),
}

/// Error during loading of the main file.
#[derive(Debug, thiserror::Error)]
pub enum MainLoadError {
    #[error("Main file cannot be found in the project directory")]
    NotFound,

    #[error("Error loading main file. {0}")]
    LoadError(#[from] crate::yaml::Error),
}

impl LoadProjectDir<'_> {
    pub const MAIN_FILE_NAME: &'static str = "main.yaml";

    pub fn run(self) -> Result<Ctx, Vec<LoadError>> {
        info!("Load project into context");
        const EXPECT_NO_ERR: &str = "should be present since there are no errors";

        let mut errors = SmallVec::<[_; 32]>::new();
        self.validate_path().map_err(vec)?;

        let main = self.load_main().map_err(|e| errors.push(e.into())).ok();
        let (sinks, srcs) = self.load_sinks_and_sources(&mut errors);

        if !errors.is_empty() {
            return Err(errors.into_vec());
        }
        let main = main.expect(EXPECT_NO_ERR);

        info!("Translate main file into HIR");
        let main = hir::Main::try_from(main)
            .map_err(|e| errors.extend(e.into_iter().map(Into::into)))
            .ok();

        macro_rules! hir_src_sink {
            ($src_or_sink:expr, $ty:ident) => {
                $src_or_sink
                    .into_iter()
                    .map(|v| {
                        let s = v.rust_path_string();
                        let (_, v) = v.unwrap();
                        hir::$ty::try_from(v).map(|v| v.to_named(s))
                    })
                    .filter_map(|sink| {
                        sink.map_err(|e| errors.extend(e.into_iter().map(Into::into)))
                            .ok()
                    })
            };
        }

        info!(
            "Translate to HIR sinks and sources, also populate error array if there are any found"
        );
        let sinks: SmallVec<[_; 32]> = hir_src_sink!(sinks, UnnamedSink).collect();
        let srcs: SmallVec<[_; 32]> = hir_src_sink!(srcs, UnnamedSource).collect();

        if !errors.is_empty() {
            return Err(errors.into_vec());
        } else {
            debug!("HIR is ready, no errors by this point");
        }
        let main = main.expect(EXPECT_NO_ERR);
        let sinks = sinks.into_iter().map(|v| v.expect(EXPECT_NO_ERR));
        let srcs = srcs.into_iter().map(|v| v.expect(EXPECT_NO_ERR));

        info!("Creating new context");
        let ctx = Ctx::new(main.name().into(), Some(main.explain().into()));
        let mut ctx = match ctx {
            Ok(ctx) => ctx,
            Err(e) => {
                error!("Error creating context. {e}");
                errors.push(e.into());
                return Err(errors.into_vec());
            }
        };

        info!("Add sinks to the context");
        for sink in sinks {
            if let Err(e) = ctx.add_sink(sink) {
                errors.push(e.into())
            }
        }
        info!("Add sources to the context");
        for src in srcs {
            if let Err(e) = ctx.add_source(src) {
                errors.push(e.into())
            }
        }

        info!("Fill in the bindings from the main file");
        for (name, cfg) in main.bindings() {
            let ty = cfg.ty();
            let mut cfg_iter = BindingCfgIter::new(cfg.cfg());
            for (key, value) in cfg_iter {
                todo!()
            }
        }

        todo!()
    }

    fn validate_path(&self) -> Result<(), LoadError> {
        debug!("Validate project path");
        if !self.path.exists() {
            return Err(LoadError::PathDoesNotExist(self.path.into()));
        }

        if !self.path.is_dir() {
            return Err(LoadError::PathIsNotDir(self.path.into()));
        }

        Ok(())
    }

    fn load_main(&self) -> Result<v01::Main, MainLoadError> {
        debug!("Load main file");
        let main_file = self.path.join("main.yaml");
        if !main_file.exists() {
            return Err(MainLoadError::NotFound);
        }

        let main = v01::Main::load_from_path(Self::MAIN_FILE_NAME.as_ref())?;
        Ok(main)
    }

    /// List all YAML files except for main in the project directory.
    fn list_other_yaml_files(&self) -> std::io::Result<Vec<std::path::PathBuf>> {
        debug!("List other YAML files");

        let mut files = SmallVec::<[_; 32]>::new();
        for entry in std::fs::read_dir(self.path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && path.extension() == Some("yaml".as_ref())
                && path.file_name() != Some(Self::MAIN_FILE_NAME.as_ref())
            {
                files.push(path);
            }
        }

        Ok(files.into_vec())
    }

    /// Load all sinks and sources from the project directory.
    ///
    /// # Failure
    /// On error, the error array is filled with errors and function returns empty arrays.
    fn load_sinks_and_sources(
        &self,
        errors: &mut SmallVec<[LoadError; 32]>,
    ) -> (
        SmallVec<[File<v01::Sink>; 32]>,
        SmallVec<[File<v01::Source>; 32]>,
    ) {
        debug!("Load sinks and sources");

        let list = self.list_other_yaml_files().map_err(LoadError::DirList);
        let list = match list {
            Ok(list) => list,
            Err(e) => {
                error!("Error listing other YAML files: {e:?}");
                errors.push(e.into());
                return Default::default();
            }
        };

        let mut sinks = SmallVec::new();
        let mut srcs = SmallVec::new();

        let loader = list.into_iter().map(|path| {
            let v = SinkOrSource::load(&path);
            path.wrap(v)
        });
        for file in loader {
            use SinkOrSource::*;
            let (path, val) = file.unwrap();
            match val {
                Ok(Sink(sink)) => sinks.push(path.wrap(sink)),
                Ok(Source(src)) => srcs.push(path.wrap(src)),
                Err(e) => errors.push(e.into()),
            }
        }

        info!("Loaded sinks: {:?}", sinks.len());
        info!("Loaded sources: {:?}", srcs.len());
        (sinks, srcs)
    }
}

enum SinkOrSource {
    Sink(v01::Sink),
    Source(v01::Source),
}

impl SinkOrSource {
    fn load(file: &std::path::Path) -> Result<Self, crate::yaml::Error> {
        use serde_yml::from_str;
        let s = std::fs::read_to_string(file)?;
        let header = from_str::<v01::Header>(&s)?;

        use v01::FileKind::*;
        match header.ty {
            Main => unreachable!("main should be loaded by separate function"),
            Sink => {
                let sink = from_str::<v01::Sink>(&s)?;
                Ok(Self::Sink(sink))
            }
            Source => {
                let source = from_str::<v01::Source>(&s)?;
                Ok(Self::Source(source))
            }
        }
    }
}

fn vec<T>(t: T) -> Vec<T> {
    vec![t]
}

struct File<T> {
    path: PathBuf,
    t: T,
}

impl<T> File<T> {
    /// Translate relative project path into structure, like from "module1/FileName.yaml"
    /// into "module1::FileName".
    fn rust_path_string(&self) -> String {
        use std::path::Component::*;
        use std::path::Path;
        let components = self.path.components();
        let mut s = CompactString::default();
        for component in components {
            let is_start = s.is_empty();
            match component {
                Prefix(_) | RootDir => continue,
                CurDir => continue,
                Normal(name) => {
                    if !is_start {
                        s.push_str("::");
                    }

                    let stem = Path::new(name).file_stem();
                    if let Some(stem) = stem {
                        s.push_str(&stem.to_string_lossy());
                    } else {
                        unreachable!("normal component should have a stem");
                    }
                }
                ParentDir => {
                    unreachable!("parent dir should not be present in the project path");
                }
            }
        }
        s.into()
    }

    fn unwrap(self) -> (PathBuf, T) {
        (self.path, self.t)
    }
}

trait PathBufExt {
    fn wrap<T>(self, t: T) -> File<T>;
}

impl PathBufExt for PathBuf {
    fn wrap<T>(self, t: T) -> File<T> {
        File { path: self, t }
    }
}

type BindingCfgInnerIter<'a> = hashbrown::hash_map::Iter<'a, CompactString, v01::MainBindingField>;

/// Traverse [v01::BindingCfg] tree, providing iterator to [ParamKey] and associated
/// [CompactString] value.
struct BindingCfgIter<'a> {
    /// Current prefix of the key.
    prefix: ParamKey,

    /// Stack of the positions in the map.
    iter_stack: SmallVec<[BindingCfgInnerIter<'a>; 8]>,

    /// Iterator over the list of values. If the current value is a list, this iterator
    /// is used to traverse it.
    list_iter: Option<std::slice::Iter<'a, CompactString>>,

    /// Set as Some when this iterator was created on a single Inline value in the config.
    /// It is turned to None when this element is returned.
    inline: Option<&'a CompactString>,
}

impl<'a> BindingCfgIter<'a> {
    pub fn new(cfg: &'a v01::BindingCfg) -> Self {
        use v01::BindingCfg::*;
        match cfg {
            Inline(v) => Self {
                prefix: Default::default(),
                iter_stack: Default::default(),
                list_iter: None,
                inline: Some(v),
            },
            Map(map) => Self {
                prefix: Default::default(),
                iter_stack: smallvec![map.iter()],
                list_iter: None,
                inline: None,
            },
        }
    }

    /// Pop the stack and return the prefix part that was used for the last key.
    /// If the stack is empty, return None.
    /// If the first value in the traversed map is
    /// (Inline)[v01::BindingCfg::Inline], return the empty prefix, to indicate
    /// anonymous key.
    fn pop_stack(&mut self) -> Option<CompactString> {
        if self.iter_stack.pop().is_some() {
            if let Some(prefix) = self.prefix.pop() {
                Some(prefix)
            } else {
                assert!(
                    self.iter_stack.is_empty(),
                    "we assume this is a map with single Inline, so stack should be empty after the pop"
                );
                Some(Default::default())
            }
        } else {
            None
        }
    }

    fn push_stack(&mut self, iter: BindingCfgInnerIter<'a>, prefix: CompactString) {
        self.iter_stack.push(iter);
        self.prefix.push(prefix);
    }
}

impl<'a> Iterator for BindingCfgIter<'a> {
    type Item = (ParamKey, &'a CompactString);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inline) = self.inline.take() {
            trace!("Returning single inline value `{inline}`, terminating the iterator");
            return Some((self.prefix.clone(), inline));
        } else if let Some(list_iter) = self.list_iter.as_mut() {
            if let Some(value) = list_iter.next() {
                trace!("Returning list value `{value}`");
                return Some((self.prefix.clone(), value));
            } else {
                trace!(
                    "List is exhausted, terminating the list iterator and continuing with the map"
                );
                self.list_iter = None;
            }
        } else {
            trace!("Map iteration");
        }

        loop {
            if let Some(head_iter) = self.iter_stack.last_mut() {
                if let Some((key, value)) = head_iter.next() {
                    use v01::MainBindingField::*;
                    match value {
                        Value(value) => {
                            trace!("Found value `{value}`");
                            let mut prefix = self.prefix.clone();
                            prefix.push(key.clone());
                            return Some((prefix, value));
                        }
                        List(list) => {
                            trace!("Found list");
                            let mut prefix = self.prefix.clone();
                            prefix.push(key.clone());
                            let mut list_iter = list.iter();
                            let next_value = list_iter.next();
                            self.list_iter = Some(list_iter);
                            return next_value.map(|v| {
                                trace!("Returning `{v}` from new list");
                                (prefix, v)
                            });
                        }
                        Map(map) => {
                            trace!("Found map, pushing it to the stack");
                            self.push_stack(map.iter(), key.clone());
                            continue;
                        }
                    }
                } else {
                    trace!("Map is exhausted, popping the stack");
                    self.pop_stack();
                }
            } else {
                trace!("Stack is empty, iterator is exhausted");
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_rust_path_string() {
        let path = File {
            path: PathBuf::from("module1/FileName.yaml"),
            t: (),
        };
        assert_eq!(path.rust_path_string(), "module1::FileName");
    }
}
