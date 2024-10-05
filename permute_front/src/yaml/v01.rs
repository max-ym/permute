use compact_str::CompactString;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub version: CompactString,
    #[serde(rename = "type")]
    pub ty: FileKind,

    #[serde(default)]
    #[serde(rename = "use")]
    pub uses: Vec<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Main,
    Source,
    Sink,
}

/// The main file of the project.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Main {
    /// Project name.
    #[serde(rename = "permute")]
    pub header: Header,
    pub name: CompactString,
    pub explain: Option<CompactString>,

    #[serde(rename = "pipe")]
    pub pipes: Vec<CompactString>,

    #[serde(rename = "let")]
    pub bindings: MainBindings,
}

impl Main {
    /// Load the main file from the given path.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, super::Error> {
        let s = std::fs::read_to_string(path)?;
        let main: Main = serde_yml::from_str(&s)?;
        Ok(main)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MainBindings {
    pub bindings: HashMap<CompactString, MainBinding>,
}

#[derive(Debug)]
pub struct MainBinding {
    /// Name of the type that this binding is for.
    pub ty: RustTy,
    pub cfg: BindingCfg,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BindingCfg {
    Inline(CompactString),
    Map(HashMap<CompactString, MainBindingField>),
}

impl<'de> Deserialize<'de> for MainBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either<T, U> {
            First(T),
            Second(U),
        }

        let actual: HashMap<CompactString, Either<CompactString, HashMap<CompactString, MainBindingField>>> =
            Deserialize::deserialize(deserializer)?;

        if actual.len() != 1 {
            return Err(serde::de::Error::custom("expected exactly one key"));
        }

        let (ty, cfg) = actual.into_iter().next().unwrap();

        let cfg = match cfg {
            Either::First(s) => {
                // Inline Rust code.
                BindingCfg::Inline(s)
            }
            Either::Second(map) => BindingCfg::Map(map),
        };

        Ok(MainBinding {
            ty: RustTy(ty),
            cfg,
        })
    }
}

impl Serialize for MainBinding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = HashMap::with_capacity(1);
        map.insert(self.ty.0.clone(), self.cfg.clone());
        map.serialize(serializer)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MainBindingField {
    /// A field that is a simple value.
    Value(CompactString),

    /// A field that is a list of values.
    List(Vec<CompactString>),

    /// A field that is a map of other fields.
    Map(HashMap<CompactString, MainBindingField>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(rename = "permute")]
    pub header: Header,
    pub explain: Option<CompactString>,
    pub filters: HashMap<CompactString, SourceFilter>,
    pub columns: HashMap<CompactString, SourceColumn>,
    pub filter_check: Option<Check>,
    pub column_check: Option<Check>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFilter {
    pub explain: Option<CompactString>,
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub default: Option<RustExpr>,
    pub check: Option<Check>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceColumn {
    pub explain: Option<CompactString>,
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub check: Option<Check>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Check {
    Inline(CheckExpr),
    List(Vec<CheckExpr>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CheckExpr {
    ExprExpl { explain: CompactString, define: RustExpr },
    Expr(RustExpr),
}

impl CheckExpr {
    pub fn explain(&self) -> Option<&str> {
        match self {
            CheckExpr::ExprExpl { explain, .. } => Some(explain),
            CheckExpr::Expr(_) => None,
        }
    }

    pub fn expr(&self) -> &RustExpr {
        match self {
            CheckExpr::ExprExpl { define, .. } => define,
            CheckExpr::Expr(expr) => expr,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RustExpr(pub CompactString);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RustTy(pub CompactString);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sink {
    #[serde(rename = "permute")]
    pub header: Header,
    pub explain: Option<CompactString>,
    pub param: HashMap<CompactString, SinkColumn>,
    pub check: Option<Check>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SinkColumn {
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub explain: Option<CompactString>,
    pub check: Option<Check>,
    pub default: Option<RustExpr>,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn main() -> Main {
        let s = include_str!("../samples/example1/main.yaml");
        serde_yml::from_str(s).unwrap()
    }

    pub fn source() -> Source {
        let s = include_str!("../samples/example1/EmploymentRecord.yaml");
        serde_yml::from_str(s).unwrap()
    }

    pub fn sink() -> Sink {
        let s = include_str!("../samples/example1/Csv.yaml");
        serde_yml::from_str(s).unwrap()
    }

    #[test]
    fn deserialize_main() {
        println!("{:#?}", main());
    }

    #[test]
    fn deserialize_empl_rec() {
        println!("{:#?}", source());
    }

    #[test]
    fn deserialize_sink() {
        println!("{:#?}", sink());
    }
}
