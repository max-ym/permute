/// Version 0.1 of YAML format.
pub mod v01;

/// High-level representation of the project. This validates the input data and turns
/// it into a form that is easier to work with during compilation.
pub mod hir;

/// Load project from files.
pub mod load;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yml::Error),
}
