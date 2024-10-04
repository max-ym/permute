use compact_str::CompactString;
use hashbrown::HashMap;
use log::*;
use smallvec::SmallVec;

type IdentId = u16;

pub struct Main {
    /// Name of the project. Cannot be empty.
    name: String,

    /// Optional explanation for the project. Empty string means no explanation.
    explain: String,

    /// List of pipes in the project. This holds identifiers of the input and output
    /// of each pipe.
    pipes: Vec<(IdentId, IdentId)>,

    /// Bindings for the project.
    bindings: HashMap<IdentId, MainBinding>,

    /// Identifiers of bindings used in the main file. These are valid Rust identifiers.
    idents: Vec<CompactString>,

    /// Use clauses for the main file. These are used to import types from other modules.
    uses: Vec<syn::UseTree>,
}

impl Main {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn pipes(&self) -> impl Iterator<Item = Pipe> + '_ {
        self.pipes.iter().copied().map(move |(input, output)| Pipe {
            input: self.idents[input as usize].as_str(),
            output: self.idents[output as usize].as_str(),
        })
    }

    pub fn bindings(&self) -> impl Iterator<Item = (&str, &MainBinding)> {
        self.bindings.iter().map(move |(ident, binding)| {
            let ident = self.idents[*ident as usize].as_str();
            (ident, binding)
        })
    }

    pub fn get_binding(&self, id: IdentId) -> Option<&MainBinding> {
        self.bindings.get(&id)
    }

    pub fn uses(&self) -> impl Iterator<Item = &syn::UseTree> {
        self.uses.iter()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MainError {
    #[error("Failed to parse use clause. {0}")]
    UseParse(syn::Error),

    #[error("Invalid project name. `{0}`")]
    InvalidName(String),

    #[error(transparent)]
    PipeParseError(#[from] StringToPipeParseError),

    #[error("Binding `{ident}` not found for pipe `{pipe}`")]
    BindingNotFound { pipe: String, ident: String },

    #[error("Failed to parse binding type. {0}")]
    BindingTypeParse(syn::Error),
}

impl TryFrom<super::v01::Main> for Main {
    type Error = Vec<MainError>;

    fn try_from(input: super::v01::Main) -> Result<Self, Self::Error> {
        info!("Making main file HIR");
        let mut errors = Vec::new();

        let uses = if input.header.uses.is_empty() {
            Default::default()
        } else {
            let at_least = input.header.uses.len().max(16);
            let mut uses = Vec::with_capacity(at_least);
            for usage in input.header.uses {
                match syn::parse_str(&usage) {
                    Ok(u) => uses.push(u),
                    Err(err) => errors.push(MainError::UseParse(err)),
                }
            }
            debug!("Parsed {} use clauses", uses.len());
            uses.shrink_to_fit();
            uses
        };

        if !input.name.is_valid_ident() {
            errors.push(MainError::InvalidName(input.name.clone()));
        }

        let explain = input.explain.unwrap_or_default();

        let idents = {
            let bindings = &input.bindings.bindings;
            let mut idents = Vec::with_capacity(bindings.len());
            for (ident, _) in bindings {
                idents.push(CompactString::from(ident));
            }
            debug!("Parsed {} binding idents", idents.len());
            idents
        };

        let pipes = {
            let mut pipes = Vec::with_capacity(input.pipes.len());
            for string in input.pipes {
                let parsed = match Pipe::try_from(string.as_str()) {
                    Ok(p) => p,
                    Err(err) => {
                        errors.push(MainError::PipeParseError(err));
                        continue;
                    }
                };
                let find_ident = |name: &str| {
                    idents
                        .iter()
                        .position(|i| i.as_str() == name)
                        .map(|v| v as IdentId)
                        .ok_or_else(|| MainError::BindingNotFound {
                            pipe: string.clone(),
                            ident: name.to_string(),
                        })
                };

                let input = match find_ident(parsed.input()) {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                let output = match find_ident(parsed.output()) {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };

                trace!("Parsed pipe: {input} -> {output}");
                pipes.push((input, output));
            }
            debug!("Parsed {} pipes", pipes.len());
            pipes
        };

        let bindings = {
            let mut bindings = HashMap::with_capacity(input.bindings.bindings.len());
            for (ident, binding) in input.bindings.bindings {
                let ty = syn::parse_str(&binding.ty.0).map_err(MainError::BindingTypeParse);
                let ty = match ty {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                let cfg = binding.cfg;
                let id = idents
                    .iter()
                    .position(|i| i == ident)
                    .map(|v| v as IdentId)
                    .expect("bindings were generated from this very array earlier");
                bindings.insert(id, MainBinding { ty, cfg });
            }
            debug!("Parsed {} bindings", bindings.len());
            bindings
        };

        if errors.is_empty() {
            Ok(Main {
                name: input.name,
                explain,
                pipes,
                bindings,
                idents,
                uses,
            })
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Pipe<'main> {
    input: &'main str,
    output: &'main str,
}

impl<'main> Pipe<'main> {
    pub fn input(&self) -> &'main str {
        self.input
    }

    pub fn output(&self) -> &'main str {
        self.output
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to parse pipe. Expected `input -> output`")]
pub struct StringToPipeParseError(String);

impl<'a> TryFrom<&'a str> for Pipe<'a> {
    type Error = StringToPipeParseError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        info!("Parsing pipe from string `{value}`");
        let err = || StringToPipeParseError(value.into());

        let mut parts = value.split("->");
        let input = parts.next().ok_or_else(err)?;
        let output = parts.next().ok_or_else(err)?;
        if parts.next().is_some() {
            Err(err())
        } else {
            Ok(Pipe {
                input: input.trim(),
                output: output.trim(),
            })
        }
    }
}

pub struct MainBinding {
    /// The type that this binding is for.
    ty: syn::Type,

    /// Configuration for the binding. We can't validate it right now as it requires
    /// us to parse the other configuration files and get to know which fields
    /// are available and how to validate them.
    cfg: super::v01::BindingCfg,
}

impl MainBinding {
    pub fn cfg(&self) -> &super::v01::BindingCfg {
        &self.cfg
    }

    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }
}

pub struct Sink {
    /// Name of the sink. This is a valid Rust identifier.
    name: CompactString,

    /// Explanation for the sink. May be empty.
    explain: String,

    /// Parameters that are passed to the sink.
    params: Vec<SinkParam>,

    /// Checks that are performed on the sink defined in the configuration,
    /// outside parameters, hence applicable to configuration of a sink as a whole.
    additional_checks: Vec<Check>,

    /// List of types that are imported from other modules. This is done via "use" clause.
    uses: Vec<syn::UseTree>,
}

impl Sink {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn params(&self) -> impl Iterator<Item = &SinkParam> {
        self.params.iter()
    }

    pub fn additional_checks(&self) -> impl Iterator<Item = &Check> {
        self.additional_checks.iter()
    }

    pub fn uses(&self) -> impl Iterator<Item = &syn::UseTree> {
        self.uses.iter()
    }
}

pub struct Check {
    /// Explanation for the check. May be empty.
    explain: String,

    /// The expression that is used to check the condition.
    define: syn::Expr,
}

impl Check {
    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn define(&self) -> &syn::Expr {
        &self.define
    }
}

pub struct SinkParam {
    /// Type of the parameter.
    ty: syn::Type,

    /// Name of the parameter. This is a valid Rust identifier.
    name: CompactString,

    /// Checks that are performed on the parameter defined in the configuration.
    /// Can be none.
    checks: SmallVec<[Check; 1]>,

    /// Default value for the parameter. This is optional and may be None.
    default: Option<syn::Expr>,
}

impl SinkParam {
    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }

    pub fn default(&self) -> Option<&syn::Expr> {
        self.default.as_ref()
    }
}

pub struct Source {
    /// Name of the source. This is a valid Rust identifier.
    name: CompactString,

    /// Explanation for the source. May be empty.
    explain: String,

    filters: Vec<SourceFilter>,

    columns: Vec<SourceColumn>,

    /// Checks that are performed on the source defined in the configuration,
    /// outside parameters, hence applicable to configuration of a source as a whole.
    filter_additional_checks: Vec<Check>,

    /// Checks that are performed on the source data columns,
    /// to verify correctness of the data in general, with relation to one or several columns.
    column_additional_checks: Vec<Check>,

    /// List of types that are imported from other modules. This is done via "use" clause.
    uses: Vec<syn::UseTree>,
}

impl Source {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn filters(&self) -> impl Iterator<Item = &SourceFilter> {
        self.filters.iter()
    }

    pub fn columns(&self) -> impl Iterator<Item = &SourceColumn> {
        self.columns.iter()
    }

    pub fn filter_additional_checks(&self) -> impl Iterator<Item = &Check> {
        self.filter_additional_checks.iter()
    }

    pub fn column_additional_checks(&self) -> impl Iterator<Item = &Check> {
        self.column_additional_checks.iter()
    }

    pub fn uses(&self) -> impl Iterator<Item = &syn::UseTree> {
        self.uses.iter()
    }
}

pub struct SourceFilter {
    /// Explanation for the filter. May be empty.
    explain: String,

    /// Type of the filter.
    ty: syn::Type,

    /// Default value for the filter. This is optional and may be None.
    default: Option<syn::Expr>,

    /// Checks that are performed on the filter defined in the configuration.
    /// Can be none.
    checks: SmallVec<[Check; 1]>,
}

impl SourceFilter {
    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn default(&self) -> Option<&syn::Expr> {
        self.default.as_ref()
    }

    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }
}

pub struct SourceColumn {
    /// Explanation for the column. May be empty.
    explain: String,

    /// Type of the column.
    ty: syn::Type,

    /// Checks that are performed on the column defined in the configuration.
    /// Can be none.
    checks: SmallVec<[Check; 1]>,
}

impl SourceColumn {
    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }
}

trait StringExt {
    fn is_valid_ident(&self) -> bool;
}

impl StringExt for str {
    fn is_valid_ident(&self) -> bool {
        let ident: Result<syn::Ident, _> = syn::parse_str(self);
        ident.is_ok()
    }
}
