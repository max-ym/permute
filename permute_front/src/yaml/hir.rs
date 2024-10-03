use compact_str::CompactString;
use hashbrown::HashMap;
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

pub struct Check {
    /// Explanation for the check. May be empty.
    explain: String,

    /// The expression that is used to check the condition.
    define: RustExpr,
}

/// Rust expression parsed from the configuration file. At this stage
/// it is already parsed into AST and is known to be a valid Rust expression.
pub struct RustExpr(syn::Expr);

pub struct SinkParam {
    /// Type of the parameter.
    ty: syn::Type,

    /// Name of the parameter. This is a valid Rust identifier.
    name: CompactString,

    /// Checks that are performed on the parameter defined in the configuration.
    /// Can be none.
    checks: SmallVec<[Check; 1]>,

    /// Default value for the parameter. This is optional and may be None.
    default: Option<RustExpr>,
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

pub struct SourceFilter {
    /// Explanation for the filter. May be empty.
    explain: String,

    /// Type of the filter.
    ty: syn::Type,

    /// Default value for the filter. This is optional and may be None.
    default: Option<RustExpr>,

    /// Checks that are performed on the filter defined in the configuration.
    /// Can be none.
    checks: SmallVec<[Check; 1]>,
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
