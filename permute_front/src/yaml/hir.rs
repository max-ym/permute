use compact_str::CompactString;
use hashbrown::HashMap;
use proc_macro2::Span;
use smallvec::SmallVec;

type IdentId = u16;
type TypeId = u16;

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

    /// List of types that are imported from other modules. This is done via "use" clause.
    /// This unwraps all "use"s when they are both just paths like "std::slice::Iter" and
    /// other more complex ones.
    imported_ty: Vec<ImportedTy>,
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

    pub fn imported_tys(&self) -> impl Iterator<Item = &ImportedTy> {
        self.imported_ty.iter()
    }

    pub fn get_imported_ty(&self, id: TypeId) -> Option<&ImportedTy> {
        self.imported_ty.get(id as usize)
    }

    pub fn get_binding(&self, id: IdentId) -> Option<&MainBinding> {
        self.bindings.get(&id)
    }

    /// Get type information for the given binding from this [Main].
    pub fn get_binding_ty<'a>(&'a self, binding: &'a MainBinding) -> Option<&'a ImportedTy> {
        self.get_imported_ty(binding.ty)
    }
}

pub struct ImportedTy {
    rename: Option<CompactString>,
    segments: SmallVec<[CompactString; 8]>,
}

impl ImportedTy {
    pub fn name(&self) -> &str {
        self.rename
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| self.segments.last().unwrap().as_str())
    }

    pub fn from_segments(segments: impl Iterator<Item = String>) -> Self {
        let segments = segments.map(CompactString::from).collect();
        Self {
            rename: None,
            segments,
        }
    }

    pub fn from_segments_with_rename(
        segments: impl Iterator<Item = String>,
        rename: String,
    ) -> Self {
        let segments = segments.map(CompactString::from).collect();
        Self {
            rename: Some(CompactString::from(rename)),
            segments,
        }
    }

    pub fn syn_path(&self) -> syn::Path {
        let segments = self.segments.iter().map(|s| s.as_str());
        let mut path = syn::Path {
            leading_colon: None,
            segments: Default::default(),
        };
        for segment in segments {
            path.segments.push(syn::PathSegment {
                ident: syn::Ident::new(segment, Span::call_site()),
                arguments: syn::PathArguments::None,
            });
        }
        path
    }
}

pub struct Pipe<'main> {
    pub input: &'main str,
    pub output: &'main str,
}

/// Connection between some item's type and optional related "use" clause.
pub struct TyPath {
    /// Optional import for the type if it was found. Otherwise, this is None and
    /// the [Self::ty] is considered to be "crate::" relative.
    imported_ty: Option<ImportedTy>,

    /// Type as it was declared. May be relative to the current module.
    ty: syn::TypePath,
}

impl TyPath {
    /// Get the merged path of the type and the imported type. This should output
    /// the full type path that can be used during codegen to refer to the type correctly.
    pub fn syn_path(&self) -> syn::Path {
        let mut path = self.ty.path.clone();
        if let Some(item_use) = &self.imported_ty {
            path.segments
                .extend(item_use.segments.iter().map(|segment| syn::PathSegment {
                    ident: syn::Ident::new(segment, Span::call_site()),
                    arguments: syn::PathArguments::None,
                }));
        } else if !self.is_crate_absolute() {
            // Make the path absolute to the crate root.
            path.segments.insert(
                0,
                syn::PathSegment {
                    ident: syn::Ident::new("crate", Span::call_site()),
                    arguments: syn::PathArguments::None,
                },
            );
        }
        path
    }

    /// Whether the type is absolute to the crate root, i.e. it starts with "crate::".
    pub fn is_crate_absolute(&self) -> bool {
        self.ty.path.leading_colon.is_some()
            || self
                .ty
                .path
                .segments
                .first()
                .map_or(false, |s| s.ident == "crate")
    }
}

pub struct MainBinding {
    /// Name of the type that this binding is for.
    ty: TypeId,

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
