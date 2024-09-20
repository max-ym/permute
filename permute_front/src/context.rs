use hashbrown::HashMap;

/// Context for the project.
pub struct Ctx {
    /// The name of the project. Cannot be empty.
    name: String,

    /// User comment about this project. Empty string means no comment.
    explain: String,

    /// Data sources.
    srcs: Vec<DataSource>,

    /// Data sinks.
    sinks: Vec<Sink>,
}

impl Ctx {
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

    pub fn add_source(&mut self, src: DataSource) -> Result<(), AddSourceErr> {
        // Check if the source with the same name already exists.
        if self.srcs.iter().any(|s| s.name() == src.name()) {
            return Err(AddSourceErr::NameExists(src.name().to_string()));
        }

        self.srcs.push(src);
        Ok(())
    }

    pub fn add_sink(&mut self, sink: Sink) -> Result<(), AddSinkErr> {
        // Check if the sink with the same name already exists.
        if self.sinks.iter().any(|s| s.name() == sink.name()) {
            return Err(AddSinkErr::NameExists(sink.name().to_string()));
        }

        self.sinks.push(sink);
        Ok(())
    }
}

#[derive(Debug)]
pub enum AddSourceErr {
    NameExists(String),
}

#[derive(Debug)]
pub enum AddSinkErr {
    NameExists(String),
}

pub struct DataSource {
    /// Name of the source. Cannot be empty.
    name: String,

    /// User comment about this source. Empty string means no comment.
    explain: String,

    /// Applicable filters on the source query.
    filters: HashMap<String, FilterTy>,

    /// Data columns in the source.
    columns: Vec<SourceColumn>,

    /// Global checks that can operate on multiple columns.
    checks: Vec<ExplainExpr>,
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

    pub fn filters(&self) -> &HashMap<String, FilterTy> {
        &self.filters
    }

    pub fn columns(&self) -> &[SourceColumn] {
        &self.columns
    }

    pub fn checks(&self) -> &[ExplainExpr] {
        &self.checks
    }
}

pub struct FilterTy {
    /// Default value for the filter, to use when it is not explicitly set.
    default: Option<syn::Expr>,

    /// User comment about this filter. Empty string means no comment.
    explain: String,

    /// Data type of the filter field.
    ty: syn::Type,

    /// Checks that are applied to the filter.
    checks: Vec<ExplainExpr>,
}

impl FilterTy {
    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }
}

pub struct SourceColumn {
    /// Name of the column. Cannot be empty.
    name: String,

    /// Optional explanation for the column. Empty string means no explanation.
    explain: String,

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
}

/// Data sink.
pub struct Sink {
    /// Name of the sink. Cannot be empty.
    name: String,

    /// User comment about this sink. Empty string means no comment.
    explain: String,

    /// Parameters that are passed to the sink.
    params: HashMap<String, SinkParam>,

    /// Global checks that are applied to the sink,
    /// and can operate on multiple parameters.
    checks: Vec<ExplainExpr>,
}

impl Sink {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &HashMap<String, SinkParam> {
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

/// Sink parameter.
pub struct SinkParam {
    /// Default value for the parameter, to use when it is not explicitly set.
    default: Option<syn::Expr>,

    /// Data type of the parameter.
    ty: syn::Type,

    /// Checks that are applied to the parameter.
    checks: Vec<ExplainExpr>,

    /// User comment about this sink. Empty string means no comment.
    explain: String,
}

impl SinkParam {
    pub fn explain(&self) -> Option<&str> {
        if self.explain.is_empty() {
            None
        } else {
            Some(&self.explain)
        }
    }
}

/// Track data from source column to all sinks.
pub struct TrackColumn {
    pub src: String,
    pub col: String,
}

/// Track data from sink field to all source columns.
pub struct TrackField {
    pub sink: String,
    pub field: String,
}

/// Track data used in this function to all other functions, and source, and sink fields.
pub struct TrackFn {
    pub fn_path: Vec<String>,
}
