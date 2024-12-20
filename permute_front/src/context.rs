use crate::yaml::hir;
use compact_str::{CompactString, ToCompactString};
use hashbrown::HashMap;
use log::*;
use smallvec::SmallVec;

type IdentId = usize;
type BindingId = IdentId;

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

    srcs_bindings: Vec<Binding>,
    sinks_bindings: Vec<Binding>,

    /// Parameter values for sink bindings. Key is a tuple of sink identifier and parameter name.
    /// It also stores initialization expressions for native sinks, in this case
    /// [ParamKey] part of the key is blank.
    sink_params: HashMap<(IdentId, ParamKey), syn::Expr>,

    /// Filter values for source bindings. Key is a tuple of source identifier and parameter name.
    /// It also stores initialization expressions for native sources, in this case
    /// [ParamKey] part of the key is blank.
    src_filters: HashMap<(IdentId, ParamKey), syn::Expr>,

    /// Pipes that connect sources to sinks.
    /// Each pipe here is a tuple of source and sink indexes,
    /// in [Self::srcs_bindings] and [Self::sinks_bindings].
    pipes: Vec<(BindingId, BindingId)>,
}

/// Bindings are used to configure individual instances of sources and sinks.
pub struct Binding {
    /// Name of the binding identified. Cannot be empty.
    name: CompactString,

    /// Source or sink that this binding is for.
    target: IdentId,
}

#[derive(Debug, thiserror::Error)]
#[error("Project name cannot be empty")]
pub struct EmptyNameError;

#[derive(Debug, thiserror::Error)]
pub enum AddParamErr {
    #[error("Parameter's parent with the name {0} not found")]
    DestNotFound(CompactString),

    #[error("Parameter {0} already set to `{1:?}`")]
    AlreadySet(ParamKey, syn::Expr),

    #[error("Cannot add parameter to native item. {0}")]
    Native(#[from] UnexpectedNative),
}

#[derive(Debug, thiserror::Error)]
pub enum AddInitErr {
    #[error("Item with the name {0} not found")]
    DestNotFound(CompactString),

    #[error("Item {0} already initialized with `{1:?}`")]
    AlreadySet(ParamKey, syn::Expr),

    #[error("Expected native item `{0}` to attach initializer, but it instead has YAML origin.")]
    ExpectNative(CompactString),
}

#[derive(Debug, thiserror::Error)]
pub enum AddBindingErr {
    #[error("Binding with the name {0} already exists")]
    NameExists(CompactString),

    #[error("Binding target with the name {0} not found")]
    NotFound(CompactString),

    #[error(transparent)]
    Native(#[from] UnexpectedNative),
}

#[derive(Debug, thiserror::Error)]
#[error("Expected YAML origin but found native item `{0}`.")]
pub struct UnexpectedNative(pub CompactString);

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
            srcs_bindings: Vec::new(),
            sinks_bindings: Vec::new(),
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

    fn sink_with_id(&self, sink: &str) -> Option<(&Sink, IdentId)> {
        self.sink_id(sink).map(|v| (&self.sinks[v], v))
    }

    fn source_id(&self, src: &str) -> Option<IdentId> {
        self.srcs.iter().position(|s| s.name() == src).map(|v| {
            trace!("Found source with name `{src}` at index {v}");
            v
        })
    }

    fn source_with_id(&self, src: &str) -> Option<(&DataSource, IdentId)> {
        self.source_id(src).map(|v| (&self.srcs[v], v))
    }

    pub fn add_source(&mut self, src: impl Into<DataSource>) -> Result<(), AddSourceErr> {
        let src = src.into();
        trace!("Add source `{}`", src.name());

        // Check if the source with the same name already exists.
        if self.source_id(src.name()).is_some() {
            error!("Source with the name `{}` already exists", src.name());
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

    /// Get the source binding by name. Return error if binding is native.
    pub fn yaml_source_binding(&self, name: &str) -> Result<Option<IdentId>, UnexpectedNative> {
        let binding = self
            .srcs_bindings
            .iter()
            .find(|b| b.name == name)
            .map(|v| v.target);
        if let Some(binding) = binding {
            if self.srcs[binding].is_native() {
                return Err(UnexpectedNative(name.to_compact_string()));
            }
        }
        Ok(binding)
    }

    /// Get the sink binding by name. Return error if binding is native.
    pub fn yaml_sink_binding(&self, name: &str) -> Result<Option<IdentId>, UnexpectedNative> {
        let binding = self
            .sinks_bindings
            .iter()
            .find(|b| b.name == name)
            .map(|v| v.target);
        if let Some(binding) = binding {
            if self.sinks[binding].is_native() {
                return Err(UnexpectedNative(name.to_compact_string()));
            }
        }
        Ok(binding)
    }

    pub fn add_source_binding(
        &mut self,
        name: CompactString,
        src: &str,
    ) -> Result<IdentId, AddBindingErr> {
        let src_idx = self
            .source_id(src)
            .ok_or_else(|| AddBindingErr::NotFound(src.into()))?;

        // Check if the binding with the same name already exists.
        if self.yaml_source_binding(&name)?.is_some() {
            return Err(AddBindingErr::NameExists(name));
        }

        debug!("Add source binding `{name}` for `{src}` to the context");
        let binding = Binding {
            name,
            target: src_idx,
        };
        self.srcs_bindings.push(binding);
        Ok(self.srcs_bindings.len() - 1)
    }

    pub fn add_sink_binding(
        &mut self,
        name: CompactString,
        sink: &str,
    ) -> Result<IdentId, AddBindingErr> {
        let sink_idx = self
            .sink_id(sink)
            .ok_or_else(|| AddBindingErr::NotFound(sink.into()))?;

        // Check if the binding with the same name already exists.
        if self.yaml_sink_binding(&name)?.is_some() {
            return Err(AddBindingErr::NameExists(name));
        }

        debug!("Add sink binding `{name}` for `{sink}` to the context");
        let binding = Binding {
            name,
            target: sink_idx,
        };
        self.sinks_bindings.push(binding);
        Ok(self.sinks_bindings.len() - 1)
    }

    pub fn add_binding(
        &mut self,
        name: CompactString,
        src_or_sink: &str,
    ) -> Result<IdentId, AddBindingErr> {
        trace!("Add binding `{name}` for `{src_or_sink}`");
        if self.sink_id(src_or_sink).is_some() {
            self.add_sink_binding(name, src_or_sink)
        } else {
            self.add_source_binding(name, src_or_sink)
        }
    }

    /// Add a native sink to the context. These come from Rust.
    pub fn add_native_sink(&mut self, item_path: &compile::ItemPath) -> Result<(), AddSinkErr> {
        self.add_sink(Sink {
            name: item_path.to_compact_string(),
            is_native: true,
            explain: Default::default(),
            params: HashMap::new(),
            checks: Vec::new(),
            uses: Vec::new(),
        })
    }

    /// Add a native source to the context. These come from Rust.
    pub fn add_native_source(&mut self, item_path: &compile::ItemPath) -> Result<(), AddSourceErr> {
        self.add_source(DataSource {
            name: item_path.to_compact_string(),
            is_native: true,
            explain: Default::default(),
            filters: HashMap::new(),
            columns: Vec::new(),
            filter_checks: Vec::new(),
            column_checks: Vec::new(),
            uses: Vec::new(),
        })
    }

    /// Add a parameter value to the sink.
    // See the comment on Clippy in the `add_param` method.
    #[allow(clippy::result_large_err)]
    pub fn add_sink_param(
        &mut self,
        sink_binding_name: &str,
        param_name: ParamKey,
        value: syn::Expr,
    ) -> Result<(), AddParamErr> {
        let sink_bind_idx = self
            .yaml_sink_binding(sink_binding_name)?
            .ok_or_else(|| AddParamErr::DestNotFound(sink_binding_name.into()))?;

        debug!("Add parameter `{param_name}` to sink `{sink_binding_name}`");

        // Check if exists, and if not - add new value.
        let entry = self.sink_params.entry((sink_bind_idx, param_name));
        use hashbrown::hash_map::Entry;
        match entry {
            Entry::Occupied(e) => Err(AddParamErr::AlreadySet(e.key().1.clone(), e.get().clone())),
            Entry::Vacant(e) => {
                e.insert(value);
                Ok(())
            }
        }
    }

    /// Add a filter value to the source.
    // See the comment on Clippy in the `add_param` method.
    #[allow(clippy::result_large_err)]
    pub fn add_src_filter(
        &mut self,
        src_name: &str,
        filter_name: ParamKey,
        value: syn::Expr,
    ) -> Result<(), AddParamErr> {
        let src_bind_idx = self
            .yaml_source_binding(src_name)?
            .ok_or_else(|| AddParamErr::DestNotFound(src_name.into()))?;

        debug!("Add filter `{filter_name}` to source `{src_name}`");

        // Check if exists, and if not - add new value.
        let entry = self.src_filters.entry((src_bind_idx, filter_name));
        use hashbrown::hash_map::Entry;
        match entry {
            Entry::Occupied(e) => Err(AddParamErr::AlreadySet(e.key().1.clone(), e.get().clone())),
            Entry::Vacant(e) => {
                e.insert(value);
                Ok(())
            }
        }
    }

    // Allow clippy::result_large_err as in our (current) usecase we don't
    // pop errors up the call stack and immediately handle them, so we can accept
    // a larger byte size for the error type as it would anyway be probably faster than
    // a heap allocation/deallocation.
    // This is true for current 200 bytes, but may change in the future...
    #[allow(clippy::result_large_err)]
    pub fn add_param(
        &mut self,
        sink_or_src_binding_name: &str,
        param: ParamKey,
        value: syn::Expr,
    ) -> Result<(), AddParamErr> {
        if self.sink_id(sink_or_src_binding_name).is_some() {
            self.add_sink_param(sink_or_src_binding_name, param, value)
        } else {
            self.add_src_filter(sink_or_src_binding_name, param, value)
        }
    }

    /// Add a native sink/source initializer to the context.
    pub fn add_native_init(
        &mut self,
        sink_or_src_binding_name: &str,
        expr: syn::Expr,
    ) -> Result<(), AddInitErr> {
        debug!("Add native init expression");
        macro_rules! try_add {
            ($name:ident, $map:ident) => {
                if let Some((bind, id)) = self.$name(sink_or_src_binding_name) {
                    return if bind.is_native() {
                        self.$map.insert((id, ParamKey::new()), expr);
                        Ok(())
                    } else {
                        Err(AddInitErr::ExpectNative(sink_or_src_binding_name.into()))
                    };
                }
            };
        }
        try_add!(sink_with_id, sink_params);
        try_add!(source_with_id, src_filters);
        Err(AddInitErr::DestNotFound(sink_or_src_binding_name.into()))
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

    /// Whether this source comes from YAML configuration or from
    /// a native Rust code.
    is_native: bool,

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

    /// External types that should be "used" in the generated code, for
    /// column type resolution. This is in form of "use" syntax tree
    /// as these expressions can be complex.
    uses: Vec<syn::UseTree>,
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

    pub fn is_native(&self) -> bool {
        self.is_native
    }

    pub fn uses(&self) -> &[syn::UseTree] {
        &self.uses
    }
}

impl From<hir::Source> for DataSource {
    fn from(src: hir::Source) -> Self {
        Self {
            name: src.name,
            is_native: false,
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
            uses: src.uses,
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

    /// Whether this sink comes from YAML configuration or from
    /// a native Rust code.
    is_native: bool,

    /// User comment about this sink. Empty string means no comment.
    explain: CompactString,

    /// Parameters that are passed to the sink.
    params: HashMap<CompactString, SinkParam>,

    /// Global checks that are applied to the sink,
    /// and can operate on multiple parameters.
    checks: Vec<ExplainExpr>,

    /// External types that should be "used" in the generated code, for
    /// param type resolution. This is in form of "use" syntax tree
    /// as these expressions can be complex.
    uses: Vec<syn::UseTree>,
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

    pub fn is_native(&self) -> bool {
        self.is_native
    }

    pub fn uses(&self) -> &[syn::UseTree] {
        &self.uses
    }
}

impl From<hir::Sink> for Sink {
    fn from(sink: hir::Sink) -> Self {
        Self {
            name: sink.name,
            explain: sink.explain,
            is_native: false,
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
            uses: sink.uses,
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

/// Parameter path. Is a key of:
/// - zero elements - unnamed and only parameter.
/// - one element - named parameter.
/// - several elements - named parameter in a nested structure.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamKey(SmallVec<[CompactString; 8]>);

impl ParamKey {
    pub fn new() -> Self {
        Self(SmallVec::new())
    }

    pub fn push(&mut self, part: CompactString) {
        self.0.push(part);
    }

    pub fn pop(&mut self) -> Option<CompactString> {
        self.0.pop()
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|s| s.as_str())
    }
}

impl std::fmt::Display for ParamKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, part) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{part}")?;
        }
        Ok(())
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
