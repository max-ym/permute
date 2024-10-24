use compact_str::{CompactString, ToCompactString};
use hashbrown::HashMap;
use log::*;
use smallvec::SmallVec;
use std::fmt::Debug;

type IdentId = u16;

pub type UnnamedSink = Unnamed<Sink>;
pub type UnnamedSource = Unnamed<Source>;

#[derive(Debug)]
pub struct Main {
    /// Name of the project. Cannot be empty.
    name: CompactString,

    /// Optional explanation for the project. Empty string means no explanation.
    explain: CompactString,

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
    InvalidName(CompactString),

    #[error(transparent)]
    PipeParseError(#[from] StringToPipeParseError),

    #[error("Binding `{ident}` not found for pipe `{pipe}`")]
    BindingNotFound {
        pipe: CompactString,
        ident: CompactString,
    },

    #[error("Failed to parse binding type. {0}")]
    BindingTypeParse(syn::Error),
}

impl TryFrom<super::v01::Main> for Main {
    type Error = Vec<MainError>;

    fn try_from(input: super::v01::Main) -> Result<Self, Self::Error> {
        info!("Making main file HIR");
        let mut errors = Vec::new();

        let uses = parse_uses(input.header.uses)
            .map_err(|e| {
                errors.extend(e.into_iter().map(MainError::UseParse));
            })
            .unwrap_or_default();

        if !input.name.is_valid_ident() {
            errors.push(MainError::InvalidName(input.name.clone()));
        }

        let explain = input.explain.unwrap_or_default();

        let idents = {
            let bindings = &input.bindings.bindings;
            let mut idents = Vec::with_capacity(bindings.len());
            for (ident, _) in bindings {
                idents.push(ident.to_owned());
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
                            ident: name.into(),
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

#[derive(Debug)]
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

    /// Returns the type as a string.
    pub fn ty_str(&self) -> CompactString {
        use quote::ToTokens;
        self.ty.to_token_stream().to_compact_string()
    }
}

#[derive(Debug)]
pub struct Sink {
    /// Name of the sink. This is a valid Rust identifier.
    pub(crate) name: CompactString,

    /// Explanation for the sink. May be empty.
    pub(crate) explain: CompactString,

    /// Parameters that are passed to the sink.
    pub(crate) params: Vec<SinkParam>,

    /// Checks that are performed on the sink defined in the configuration,
    /// outside parameters, hence applicable to configuration of a sink as a whole.
    pub(crate) additional_checks: Vec<Check>,

    /// List of types that are imported from other modules. This is done via "use" clause.
    pub(crate) uses: Vec<syn::UseTree>,
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

impl ToNamed for Sink {
    fn to_named(this: Unnamed<Self>, name: String) -> Result<Self, NameError<Self>> {
        if name.is_valid_ident() {
            Ok(Sink {
                name: CompactString::from(name),
                ..this.0
            })
        } else {
            Err(NameError(name, this.0))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("Failed to parse use clause. {0}")]
    Uses(syn::Error),

    #[error("Failed to parse parameter type. {0}")]
    TypeParse(syn::Error, CompactString),

    #[error("Failed to parse default value. {0}")]
    DefaultParse(syn::Error, CompactString),

    #[error("Failed to parse check expression. {0}")]
    CheckParse(syn::Error, CompactString),
}

impl TryFrom<super::v01::Sink> for Unnamed<Sink> {
    type Error = Vec<SinkError>;

    fn try_from(input: super::v01::Sink) -> Result<Self, Self::Error> {
        info!("Making sink file HIR");
        let mut errors = Vec::new();

        macro_rules! parse_check_expr {
            ($check:expr) => {{
                let check: super::v01::CheckExpr = $check;
                syn::parse_str(&check.expr().0)
                    .map_err(|e| {
                        errors.push(SinkError::CheckParse(e, check.expr().0.to_owned()));
                    })
                    .ok() // because we already pushed the error
                    .map(|define| Check {
                        explain: check.explain().unwrap_or_default().to_owned(),
                        define,
                    })
            }};
        }
        macro_rules! parse_check {
            ($check:expr) => {{
                use super::v01::Check::*;
                let check: super::v01::Check = $check;
                trace!("Parsing check: {check:#?}");
                match check {
                    Inline(check) => {
                        if let Some(c) = parse_check_expr!(check) {
                            vec![c]
                        } else {
                            Default::default()
                        }
                    }
                    List(list) => {
                        let mut acc = Vec::with_capacity(list.len());
                        for check in list {
                            if let Some(c) = parse_check_expr!(check) {
                                acc.push(c);
                            }
                        }
                        acc
                    }
                }
            }};
        }

        let uses = parse_uses(input.header.uses)
            .map_err(|e| {
                errors.extend(e.into_iter().map(SinkError::Uses));
            })
            .unwrap_or_default();

        let explain = input.explain.unwrap_or_default();

        let params = {
            let mut params = Vec::with_capacity(input.param.len());
            for (name, param) in input.param {
                let ty = syn::parse_str(&param.ty.0)
                    .map_err(|e| SinkError::TypeParse(e, param.ty.0.to_owned()));
                let ty = match ty {
                    Ok(t) => Some(t),
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                };
                let default = param.default.map(|d| {
                    syn::parse_str(&d.0).map_err(|e| SinkError::DefaultParse(e, d.0.to_owned()))
                });
                let default = match default {
                    Some(Ok(d)) => Some(d),
                    Some(Err(e)) => {
                        errors.push(e);
                        None
                    }
                    None => None,
                };

                let checks = param.check.map(|v| parse_check!(v)).unwrap_or_default();

                if let Some(ty) = ty {
                    params.push(SinkParam {
                        name,
                        explain: param.explain.unwrap_or_default(),
                        ty,
                        default,
                        checks: checks.into(),
                    });
                } else {
                    warn!("Skipping parameter `{name}` due to fatal errors in it");
                }
            }
            debug!("Parsed {} params", params.len());
            params
        };

        let additional_checks = input
            .check
            .map(|v| parse_check!(v))
            .unwrap_or_default();

        if errors.is_empty() {
            Ok(Unnamed(Sink {
                name: Default::default(),
                explain,
                params,
                additional_checks,
                uses,
            }))
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug)]
pub struct Check {
    /// Explanation for the check. May be empty.
    pub(crate) explain: String,

    /// The expression that is used to check the condition.
    pub(crate) define: syn::Expr,
}

impl Check {
    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn define(&self) -> &syn::Expr {
        &self.define
    }
}

#[derive(Debug)]
pub struct SinkParam {
    /// Type of the parameter.
    pub(crate) ty: syn::Type,

    /// Name of the parameter. This is a valid Rust identifier.
    pub(crate) name: CompactString,

    /// Explanation for the parameter. May be empty.
    pub(crate) explain: CompactString,

    /// Checks that are performed on the parameter defined in the configuration.
    /// Can be none.
    pub(crate) checks: SmallVec<[Check; 1]>,

    /// Default value for the parameter. This is optional and may be None.
    pub(crate) default: Option<syn::Expr>,
}

impl SinkParam {
    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn explain(&self) -> &str {
        &self.explain
    }

    pub fn checks(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter()
    }

    pub fn default(&self) -> Option<&syn::Expr> {
        self.default.as_ref()
    }
}

#[derive(Debug)]
pub struct Source {
    /// Name of the source. This is a valid Rust identifier.
    pub(crate) name: CompactString,

    /// Explanation for the source. May be empty.
    pub(crate) explain: CompactString,

    pub(crate) filters: Vec<SourceFilter>,

    pub(crate) columns: Vec<SourceColumn>,

    /// Checks that are performed on the source defined in the configuration,
    /// outside parameters, hence applicable to configuration of a source as a whole.
    pub(crate) filter_additional_checks: Vec<Check>,

    /// Checks that are performed on the source data columns,
    /// to verify correctness of the data in general, with relation to one or several columns.
    pub(crate) column_additional_checks: Vec<Check>,

    /// List of types that are imported from other modules. This is done via "use" clause.
    pub(crate) uses: Vec<syn::UseTree>,
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

/// Marker struct for a parsed structure for unknown file name.
#[derive(Debug)]
pub struct Unnamed<T: ToNamed>(T);

impl<T: ToNamed> Unnamed<T> {
    pub fn to_named(self, name: String) -> Result<T, NameError<T>> {
        T::to_named(self, name)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Name is not a valid identifier. `{0}`")]
pub struct NameError<T>(String, T);

pub trait ToNamed: Sized + Debug {
    fn to_named(this: Unnamed<Self>, name: String) -> Result<Self, NameError<Self>>;
}

impl ToNamed for Source {
    fn to_named(this: Unnamed<Source>, name: String) -> Result<Source, NameError<Source>> {
        if name.is_valid_ident() {
            Ok(Source {
                name: CompactString::from(name),
                ..this.0
            })
        } else {
            Err(NameError(name, this.0))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("Failed to parse type expression. {0}")]
    TypeParse(syn::Error, CompactString),

    #[error("Failed to parse default value expression. {0}")]
    DefaultParse(syn::Error, CompactString),

    #[error("Failed to parse check expression. {0}")]
    CheckParse(syn::Error, CompactString),

    #[error("Failed to parse use clause. {0}")]
    Uses(syn::Error),
}

impl TryFrom<super::v01::Source> for Unnamed<Source> {
    type Error = Vec<SourceError>;

    fn try_from(input: super::v01::Source) -> Result<Self, Self::Error> {
        info!("Making source file HIR");
        let mut errors = Vec::new();

        macro_rules! parse_check_expr {
            ($check:expr) => {{
                let check: super::v01::CheckExpr = $check;
                syn::parse_str(&check.expr().0)
                    .map_err(|e| {
                        errors.push(SourceError::CheckParse(e, check.expr().0.to_owned()));
                    })
                    .ok() // because we already pushed the error
                    .map(|define| Check {
                        explain: check.explain().unwrap_or_default().to_owned(),
                        define,
                    })
            }};
        }
        macro_rules! parse_check {
            ($check:expr) => {{
                use super::v01::Check::*;
                let check: super::v01::Check = $check;
                trace!("Parsing check: {check:#?}");
                match check {
                    Inline(check) => {
                        if let Some(c) = parse_check_expr!(check) {
                            vec![c]
                        } else {
                            Default::default()
                        }
                    }
                    List(list) => {
                        let mut acc = Vec::with_capacity(list.len());
                        for check in list {
                            if let Some(c) = parse_check_expr!(check) {
                                acc.push(c);
                            }
                        }
                        acc
                    }
                }
            }};
        }

        let filters = {
            let mut filters = Vec::with_capacity(input.filters.len());
            for (name, filter) in input.filters {
                let ty = syn::parse_str(&filter.ty.0)
                    .map_err(|e| SourceError::TypeParse(e, filter.ty.0.to_owned()));
                let ty = match ty {
                    Ok(t) => Some(t),
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                };
                let default = filter.default.map(|d| {
                    syn::parse_str(&d.0).map_err(|e| SourceError::DefaultParse(e, d.0.to_owned()))
                });
                let default = match default {
                    Some(Ok(d)) => Some(d),
                    Some(Err(e)) => {
                        errors.push(e);
                        None
                    }
                    None => None,
                };

                let checks = filter.check.map(|v| parse_check!(v)).unwrap_or_default();

                if let Some(ty) = ty {
                    filters.push(SourceFilter {
                        name,
                        explain: filter.explain.unwrap_or_default(),
                        ty,
                        default,
                        checks: checks.into(),
                    });
                } else {
                    warn!("Skipping filter `{name}` due to fatal errors in it");
                }
            }
            debug!("Parsed {} filters", filters.len());
            filters
        };

        let columns = {
            let mut columns = Vec::with_capacity(input.columns.len());
            for (name, column) in input.columns {
                let ty = syn::parse_str(&column.ty.0)
                    .map_err(|e| SourceError::TypeParse(e, column.ty.0.to_owned()));
                let ty = match ty {
                    Ok(t) => Some(t),
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                };

                let checks = column.check.map(|v| parse_check!(v)).unwrap_or_default();

                if let Some(ty) = ty {
                    columns.push(SourceColumn {
                        name,
                        explain: column.explain.unwrap_or_default(),
                        ty,
                        checks: checks.into(),
                    });
                } else {
                    warn!("Skipping column `{name}` due to fatal errors in it");
                }
            }
            debug!("Parsed {} columns", columns.len());
            columns
        };

        let filter_additional_checks = input
            .filter_check
            .map(|v| parse_check!(v))
            .unwrap_or_default();

        let column_additional_checks = input
            .column_check
            .map(|v| parse_check!(v))
            .unwrap_or_default();

        let uses = parse_uses(input.header.uses)
            .map_err(|e| {
                errors.extend(e.into_iter().map(SourceError::Uses));
            })
            .unwrap_or_default();

        if errors.is_empty() {
            Ok(Unnamed(Source {
                name: Default::default(),
                explain: input.explain.unwrap_or_default(),
                filters,
                columns,
                filter_additional_checks,
                column_additional_checks,
                uses,
            }))
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug)]
pub struct SourceFilter {
    /// Name of the filter. This is a valid Rust identifier.
    pub(crate) name: CompactString,

    /// Explanation for the filter. May be empty.
    pub(crate) explain: CompactString,

    /// Type of the filter.
    pub(crate) ty: syn::Type,

    /// Default value for the filter. This is optional and may be None.
    pub(crate) default: Option<syn::Expr>,

    /// Checks that are performed on the filter defined in the configuration.
    /// Can be none.
    pub(crate) checks: SmallVec<[Check; 1]>,
}

impl SourceFilter {
    pub fn name(&self) -> &str {
        &self.name
    }

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

#[derive(Debug)]
pub struct SourceColumn {
    /// Name of the column. This is a valid Rust identifier.
    pub(crate) name: CompactString,

    /// Explanation for the column. May be empty.
    pub(crate) explain: CompactString,

    /// Type of the column.
    pub(crate) ty: syn::Type,

    /// Checks that are performed on the column defined in the configuration.
    /// Can be none.
    pub(crate) checks: SmallVec<[Check; 1]>,
}

impl SourceColumn {
    pub fn name(&self) -> &str {
        &self.name
    }

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

fn parse_uses(input: Vec<CompactString>) -> Result<Vec<syn::UseTree>, Vec<syn::Error>> {
    let mut errors = Vec::new();
    let mut uses = Vec::with_capacity(input.len());
    for usage in input {
        match syn::parse_str(&usage) {
            Ok(u) => uses.push(u),
            Err(err) => errors.push(err),
        }
    }
    if errors.is_empty() {
        Ok(uses)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::super::v01::tests::{main, sink, source};
    use super::*;

    #[test]
    fn test_main() {
        let main = main();
        let main = Main::try_from(main).unwrap();
        println!("{main:#?}");
    }

    #[test]
    fn test_source() {
        let source = Unnamed::<Source>::try_from(source()).unwrap();
        println!("{source:#?}");
    }

    #[test]
    fn test_sink() {
        let sink = Unnamed::<Sink>::try_from(sink()).unwrap();
        println!("{sink:#?}");
    }
}
