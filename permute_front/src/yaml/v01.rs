use hashbrown::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Header {
    pub version: String,
    #[serde(rename = "type")]
    pub ty: FileKind,
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
pub struct Main {
    /// Project name.
    #[serde(rename = "permute")]
    pub header: Header,
    pub name: String,
    pub explain: Option<String>,

    #[serde(rename = "pipe")]
    pub pipes: Vec<String>,

    #[serde(rename = "let")]
    pub bindings: MainBindings,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MainBindings {
    pub bindings: HashMap<String, MainBinding>,
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
    Inline(String),
    Map(HashMap<String, MainBindingField>),
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

        let actual: HashMap<String, Either<String, HashMap<String, MainBindingField>>> =
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
    Value(String),

    /// A field that is a list of values.
    List(Vec<String>),

    /// A field that is a map of other fields.
    Map(HashMap<String, MainBindingField>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Source {
    #[serde(rename = "permute")]
    pub header: Header,
    pub filters: HashMap<String, SourceFilter>,
    pub columns: HashMap<String, SourceColumn>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceFilter {
    pub explain: Option<String>,
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub default: Option<String>,
    pub check: Option<Check>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceColumn {
    pub explain: Option<String>,
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
    ExprExpl { explain: String, define: RustExpr },
    Expr(RustExpr),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RustExpr(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RustTy(pub String);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Sink {
    #[serde(rename = "permute")]
    pub header: Header,
    pub explain: Option<String>,
    pub param: HashMap<String, SinkColumn>,
    pub check: Option<Check>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SinkColumn {
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub explain: Option<String>,
    pub check: Option<Check>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_main() {
        let s = include_str!("../samples/example1/main.yaml");
        let main: Main = serde_yml::from_str(s).unwrap();
        println!("{main:#?}");
    }

    #[test]
    fn deserialize_empl_rec() {
        let s = include_str!("../samples/example1/EmploymentRecord.yaml");
        let source: Source = serde_yml::from_str(s).unwrap();
        println!("{source:#?}");
    }

    #[test]
    fn deserialize_sink() {
        let s = include_str!("../samples/example1/Csv.yaml");
        let sink: Sink = serde_yml::from_str(s).unwrap();
        println!("{sink:#?}");
    }
}
