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
    pub cfg: MainCfg,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MainCfg {
    pub bindings: HashMap<String, MainBinding>,
}

#[derive(Debug)]
pub struct MainBinding {
    /// Name of the type that this binding is for.
    pub ty: RustTy,
    pub cfg: HashMap<String, MainBindingField>,
}

impl<'de> Deserialize<'de> for MainBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let actual: HashMap<String, HashMap<String, MainBindingField>> =
            Deserialize::deserialize(deserializer)?;

        if actual.len() != 1 {
            return Err(serde::de::Error::custom("expected exactly one key"));
        }

        let (ty, cfg) = actual.into_iter().next().unwrap();
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
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub default: Option<String>,
    pub check: Option<Vec<CheckExpr>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceColumn {
    #[serde(rename = "type")]
    pub ty: RustTy,
    pub check: Option<Vec<CheckExpr>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CheckExpr {
    ExprExpl { explain: String, expr: RustExpr },
    Expr(RustExpr),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RustExpr(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RustTy(pub String);

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
}
