use crate::yaml::hir;
use compact_str::CompactString;
use hashbrown::HashMap;
use log::*;

type IdentId = usize;

/// Code generation for the [Ctx](crate::context::Ctx).
pub mod codegen;

/// Context for the project.
pub struct Ctx {
    /// The name of the project. Cannot be empty.
    name: CompactString,

    /// User comment about this project. Empty string means no comment.
    explain: CompactString,

    /// Data sources.
    srcs: Vec<DataSource>,

    /// Data sinks.
    sinks: Vec<Sink>,

    /// Parameter values for sinks. Key is a tuple of sink identifier and parameter name.
    sink_params: HashMap<(IdentId, CompactString), syn::Expr>,

    /// Filter values for sources. Key is a tuple of source identifier and parameter name.
    src_filters: HashMap<(IdentId, CompactString), syn::Expr>,

    /// Pipes that connect sources to sinks.
    /// Each pipe here is a tuple of source and sink indexes,
    /// in [Self::sources] and [Self::sinks].
    pipes: Vec<(IdentId, IdentId)>,
}

#[derive(Debug, thiserror::Error)]
#[error("Project name cannot be empty")]
pub struct EmptyNameError;

#[derive(Debug, thiserror::Error)]
pub enum AddParamErr {
    #[error("Parameter's parent with the name {0} not found")]
    DestNotFound(CompactString),

    #[error("Parameter {0} already set to `{1:?}`")]
    AlreadySet(CompactString, syn::Expr),
}

impl Ctx {
    pub fn new(
        name: CompactString,
        explain: Option<CompactString>,
    ) -> Result<Self, EmptyNameError> {
        if name.is_empty() {
            return Err(EmptyNameError);
        }

        Ok(Self {
            name,
            explain: explain.unwrap_or_default(),
            srcs: Vec::new(),
            sinks: Vec::new(),
            sink_params: HashMap::new(),
            src_filters: HashMap::new(),
            pipes: Vec::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }

    pub fn sources(&self) -> &[DataSource] {
        &self.srcs
    }

    pub fn sinks(&self) -> &[Sink] {
        &self.sinks
    }

    fn sink_id(&self, sink: &str) -> Option<IdentId> {
        self.sinks.iter().position(|s| s.name() == sink).map(|v| {
            trace!("Found sink with name `{sink}` at index {v}");
            v
        })
    }

    fn source_id(&self, src: &str) -> Option<IdentId> {
        self.srcs.iter().position(|s| s.name() == src).map(|v| {
            trace!("Found source with name `{src}` at index {v}");
            v
        })
    }

    pub fn add_source(&mut self, src: impl Into<DataSource>) -> Result<(), AddSourceErr> {
        let src = src.into();

        // Check if the source with the same name already exists.
        if self.source_id(src.name()).is_some() {
            return Err(AddSourceErr::NameExists(src.name().into()));
        }

        debug!("Add source `{}` to the context", src.name());
        self.srcs.push(src);
        Ok(())
    }

    pub fn add_sink(&mut self, sink: impl Into<Sink>) -> Result<(), AddSinkErr> {
        let sink = sink.into();

        // Check if the sink with the same name already exists.
        if self.sink_id(sink.name()).is_some() {
            return Err(AddSinkErr::NameExists(sink.name().into()));
        }

        debug!("Add sink `{}` to the context", sink.name());
        self.sinks.push(sink);
        Ok(())
    }

    pub fn add_pipe(&mut self, src: &str, sink: &str) -> Result<(), AddPipeErr> {
        let src_idx = self
            .source_id(src)
            .ok_or_else(|| AddPipeErr::SourceNotFound(src.into()))?;
        let sink_idx = self
            .sink_id(sink)
            .ok_or_else(|| AddPipeErr::SinkNotFound(sink.into()))?;

        debug!("Add pipe from `{src}` to `{sink}`");
        self.pipes.push((src_idx, sink_idx));
        Ok(())
    }

    pub fn pipes(&self) -> impl Iterator<Item = (&DataSource, &Sink)> {
        self.pipes
            .iter()
            .copied()
            .map(|(src, sink)| (&self.srcs[src], &self.sinks[sink]))
    }

    /// Add a parameter value to the sink.
    pub fn add_sink_param(
        &mut self,
        sink_name: &str,
        param_name: &str,
        value: syn::Expr,
    ) -> Result<(), AddParamErr> {
        let sink_idx = self
            .sink_id(sink_name)
            .ok_or_else(|| AddParamErr::DestNotFound(sink_name.into()))?;

        debug!("Add parameter `{param_name}` to sink `{sink_name}`");

        // Check if exists, and if not - add new value.
        let entry = self.sink_params.entry((sink_idx, param_name.into()));
        use hashbrown::hash_map::Entry;
        match entry {
            Entry::Occupied(e) => Err(AddParamErr::AlreadySet(param_name.into(), e.get().clone())),
            Entry::Vacant(e) => {
                e.insert(value);
                Ok(())
            }
        }
    }

    /// Add a filter value to the source.
    pub fn add_src_filter(
        &mut self,
        src_name: &str,
        filter_name: &str,
        value: syn::Expr,
    ) -> Result<(), AddParamErr> {
        let src_idx = self
            .source_id(src_name)
            .ok_or_else(|| AddParamErr::DestNotFound(src_name.into()))?;

        // Check if exists, and if not - add new value.
        let entry = self.src_filters.entry((src_idx, filter_name.into()));
        use hashbrown::hash_map::Entry;
        match entry {
            Entry::Occupied(e) => Err(AddParamErr::AlreadySet(filter_name.into(), e.get().clone())),
            Entry::Vacant(e) => {
                e.insert(value);
                Ok(())
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddSourceErr {
    #[error("Source with the name {0} already exists")]
    NameExists(CompactString),
}

#[derive(Debug, thiserror::Error)]
pub enum AddSinkErr {
    #[error("Sink with the name {0} already exists")]
    NameExists(CompactString),
}

#[derive(Debug, thiserror::Error)]
pub enum AddPipeErr {
    #[error("Source with the name {0} not found")]
    SourceNotFound(CompactString),

    #[error("Sink with the name {0} not found")]
    SinkNotFound(CompactString),
}

pub struct DataSource {
    /// Identifier name for this data source. Cannot be empty. Is valid Rust identifier.
    name: CompactString,

    /// User comment about this source. Empty string means no comment.
    explain: CompactString,

    /// Applicable filters on the source query.
    filters: HashMap<CompactString, FilterTy>,

    /// Data columns in the source.
    columns: Vec<SourceColumn>,

    /// Global checks that can operate on multiple filters.
    /// These are performed during compilation on statically assigned filters.
    filter_checks: Vec<ExplainExpr>,

    /// Global checks that can operate on multiple data columns.
    /// These are performed during runtime on the source data to ensure data integrity
    /// and conformity to the schema.
    column_checks: Vec<ExplainExpr>,
}

impl DataSource {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }

    pub fn filters(&self) -> &HashMap<CompactString, FilterTy> {
        &self.filters
    }

    pub fn columns(&self) -> &[SourceColumn] {
        &self.columns
    }

    pub fn filter_checks(&self) -> &[ExplainExpr] {
        &self.filter_checks
    }

    pub fn column_checks(&self) -> &[ExplainExpr] {
        &self.column_checks
    }
}

impl From<hir::Source> for DataSource {
    fn from(src: hir::Source) -> Self {
        Self {
            name: src.name,
            explain: src.explain,
            filters: src
                .filters
                .into_iter()
                .map(|filter| (filter.name.clone(), FilterTy::from(filter)))
                .collect(),
            columns: src.columns.into_iter().map(SourceColumn::from).collect(),
            filter_checks: src
                .filter_additional_checks
                .into_iter()
                .map(ExplainExpr::from)
                .collect(),
            column_checks: src
                .column_additional_checks
                .into_iter()
                .map(ExplainExpr::from)
                .collect(),
        }
    }
}

pub struct FilterTy {
    /// Default value for the filter, to use when it is not explicitly set.
    default: Option<syn::Expr>,

    /// User comment about this filter. Empty string means no comment.
    explain: CompactString,

    /// Data type of the filter field.
    ty: syn::Type,

    /// Checks that are applied to the filter.
    checks: Vec<ExplainExpr>,
}

impl FilterTy {
    pub fn default(&self) -> Option<&syn::Expr> {
        self.default.as_ref()
    }

    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }

    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn checks(&self) -> &[ExplainExpr] {
        &self.checks
    }
}

impl From<hir::SourceFilter> for FilterTy {
    fn from(filter: hir::SourceFilter) -> Self {
        Self {
            default: filter.default,
            explain: filter.explain,
            ty: filter.ty,
            checks: filter.checks.into_iter().map(ExplainExpr::from).collect(),
        }
    }
}

pub struct SourceColumn {
    /// Name of the column. Cannot be empty.
    name: CompactString,

    /// Optional explanation for the column. Empty string means no explanation.
    explain: CompactString,

    /// Type of the column.
    ty: syn::Type,

    /// Checks that are applied to the column.
    checks: Vec<ExplainExpr>,
}

impl SourceColumn {
    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn checks(&self) -> &[ExplainExpr] {
        &self.checks
    }
}

impl From<hir::SourceColumn> for SourceColumn {
    fn from(col: hir::SourceColumn) -> Self {
        Self {
            name: col.name,
            explain: col.explain,
            ty: col.ty,
            checks: col.checks.into_iter().map(ExplainExpr::from).collect(),
        }
    }
}

pub struct ExplainExpr {
    /// Optional explanation for the expression. Empty string means no explanation.
    explain: String,
    expr: syn::Expr,
}

impl ExplainExpr {
    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }

    pub fn expr(&self) -> &syn::Expr {
        &self.expr
    }
}

impl From<hir::Check> for ExplainExpr {
    fn from(check: hir::Check) -> Self {
        Self {
            explain: check.explain,
            expr: check.define,
        }
    }
}

/// Data sink.
pub struct Sink {
    /// Name of the sink. Cannot be empty.
    name: CompactString,

    /// User comment about this sink. Empty string means no comment.
    explain: CompactString,

    /// Parameters that are passed to the sink.
    params: HashMap<CompactString, SinkParam>,

    /// Global checks that are applied to the sink,
    /// and can operate on multiple parameters.
    checks: Vec<ExplainExpr>,
}

impl Sink {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &HashMap<CompactString, SinkParam> {
        &self.params
    }

    pub fn checks(&self) -> &[ExplainExpr] {
        &self.checks
    }

    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }
}

impl From<hir::Sink> for Sink {
    fn from(sink: hir::Sink) -> Self {
        Self {
            name: sink.name,
            explain: sink.explain,
            params: sink
                .params
                .into_iter()
                .map(|param| (param.name.clone(), SinkParam::from(param)))
                .collect(),
            checks: sink
                .additional_checks
                .into_iter()
                .map(ExplainExpr::from)
                .collect(),
        }
    }
}

/// Sink parameter.
pub struct SinkParam {
    /// Default value for the parameter, to use when it is not explicitly set.
    default: Option<syn::Expr>,

    /// Data type of the parameter.
    ty: syn::Type,

    /// Checks that are applied to the parameter.
    checks: Vec<ExplainExpr>,

    /// User comment about this sink. Empty string means no comment.
    explain: CompactString,
}

impl From<hir::SinkParam> for SinkParam {
    fn from(param: hir::SinkParam) -> Self {
        let checks = param.checks.into_iter().map(ExplainExpr::from).collect();

        Self {
            default: param.default,
            ty: param.ty,
            checks,
            explain: param.explain,
        }
    }
}

impl SinkParam {
    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }

    pub fn default(&self) -> Option<&syn::Expr> {
        self.default.as_ref()
    }

    pub fn ty(&self) -> &syn::Type {
        &self.ty
    }

    pub fn checks(&self) -> &[ExplainExpr] {
        &self.checks
    }
}

/// Track data from source column to all sinks.
pub struct TrackColumn {
    pub src: CompactString,
    pub col: CompactString,
}

/// Track data from sink field to all source columns.
pub struct TrackField {
    pub sink: CompactString,
    pub field: CompactString,
}

/// Track data used in this function to all other functions, and source, and sink fields.
pub struct TrackFn {
    pub fn_path: Vec<CompactString>,
}
