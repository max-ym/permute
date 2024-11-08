use compact_str::CompactString;
use itertools::Itertools;
use log::*;
use rustc_hir::definitions::DefPath;
use rustc_hir::Item;
use rustc_hir::ItemId;
use rustc_middle::hir::map::Map;
use rustc_middle::ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::{DefId, LocalDefId};
use rustc_span::symbol::Ident;
use smallvec::SmallVec;

/// The identifier of the "std" namespace equivalent for the project.
pub const STD_NAME: &str = "permute";

/// Ease paths creation.
macro_rules! path {
    ($(::$el:ident)*) => {
        &[STD_NAME, $(stringify!($el),)*]
    };
}

/// The segments for the path for Sink trait to be found in the "std" equivalent namespace.
pub const SINK_TRAIT_PATH: &[&str] = path!(::Sink);

/// The segments for the path for Source trait to be found in the "std" equivalent namespace.
pub const SOURCE_TRAIT_PATH: &[&str] = path!(::Source);

/// Ensure that there are no loops in the code.
/// This, however, does not check for recursion.
// We may later permit loops that are evaluatable in constant contexts.
// E.g. what Rust permits in `const fn`.
pub fn no_forbidden_loops(hir: Map) -> bool {
    fn check_item(hir: Map, item: &rustc_hir::Item) -> bool {
        let name = item.ident.name.as_str();
        trace!("Checking item: `{}`", name);
        let result = match item.kind {
            rustc_hir::ItemKind::Fn(sig, _, body_id) => {
                if sig.header.constness == rustc_hir::Constness::Const {
                    // Const functions are allowed to have loops,
                    // since we know that they will terminate.
                    return true;
                }
                check_body(hir, body_id)
            }
            _ => true,
        };
        if result {
            trace!("Item `{name}` is loop-free.");
        } else {
            trace!("Item `{name}` contains a loop.");
        }
        result
    }

    fn check_body(hir: Map, body_id: rustc_hir::BodyId) -> bool {
        let value = hir.body(body_id).value;
        let span_data = value.span.data();
        trace!("Checking body at: {span_data:?}");
        let result = check_expr(hir, value);
        if result {
            trace!("Body is loop-free. {span_data:?}");
        } else {
            trace!("Body contains a loop. {span_data:?}");
        }
        result
    }

    fn check_expr(hir: Map, expr: &rustc_hir::Expr) -> bool {
        use rustc_hir::ExprKind as K;
        if !matches!(expr.kind, K::DropTemps(..)) {
            trace!("Checking expr: {:?}", expr.precedence());
        } // else don't print, as they would just make desugared part appear twice.

        match expr.kind {
            K::ConstBlock(_) => true, // Const blocks are allowed to have loops.
            K::Array(exprs) => exprs.iter().all(|expr| check_expr(hir, expr)),
            K::Call(f, args) => {
                if let K::Path(qpath) = &f.kind {
                    log_qpath(qpath);
                }
                args.iter().all(|arg| check_expr(hir, arg))
            }
            K::MethodCall(seg, recv, args, _) => {
                trace!("Method call: {}", seg.ident.as_str());
                check_expr(hir, recv) && args.iter().all(|arg| check_expr(hir, arg))
            }
            K::Tup(exprs) => exprs.iter().all(|expr| check_expr(hir, expr)),
            K::Binary(_, lhs, rhs) => check_expr(hir, lhs) && check_expr(hir, rhs),
            K::Unary(_, expr) => check_expr(hir, expr),
            K::Lit(lit) => {
                trace!("Literal: {lit:?}");
                true
            }
            K::Cast(..) => true,
            K::Type(..) => true,
            K::DropTemps(expr) => check_expr(hir, expr),
            K::Let(let_expr) => check_expr(hir, &let_expr.init),
            K::If(cond, then, other) => {
                check_expr(hir, cond)
                    && check_expr(hir, then)
                    && other.map_or(true, |e| check_expr(hir, e))
            }
            K::Loop(..) => false,
            K::Match(expr, arms, src) => {
                trace!("Match source: {src:?}");

                use rustc_hir::MatchSource as S;
                if let S::ForLoopDesugar = src {
                    return false;
                }
                if !check_expr(hir, expr) {
                    return false;
                }
                for arm in arms {
                    if !check_expr(hir, &arm.body) {
                        return false;
                    }
                }
                true
            }
            K::Closure(closure) => {
                if closure.constness == rustc_hir::Constness::Const {
                    // Const closures are allowed to have loops,
                    // since we know that they will terminate.
                    return true;
                }
                check_body(hir, closure.body)
            }
            K::Block(block, _) => check_block(hir, block),
            K::Assign(lhs, rhs, _) => check_expr(hir, lhs) && check_expr(hir, rhs),
            K::AssignOp(_, lhs, rhs) => check_expr(hir, lhs) && check_expr(hir, rhs),
            K::Field(expr, _) => check_expr(hir, expr),
            K::Index(lhs, rhs, _) => check_expr(hir, lhs) && check_expr(hir, rhs),
            K::Path(p) => {
                log_qpath(&p);
                true
            }
            K::AddrOf(_, _, expr) => check_expr(hir, expr),
            // We check for actual loops elsewhere, ignore control flow operators.
            K::Break(_, expr) => expr.map_or(true, |expr| check_expr(hir, expr)),
            K::Continue(_) => true,
            K::Ret(expr) => expr.map_or(true, |expr| check_expr(hir, expr)),
            K::Become(expr) => check_expr(hir, expr),
            K::InlineAsm(_) => true,
            K::OffsetOf(..) => true,
            K::Struct(_, expr_field, base) => {
                for expr_field in expr_field {
                    if !check_expr(hir, &expr_field.expr) {
                        return false;
                    }
                }
                base.map_or(true, |base| check_expr(hir, base))
            }
            K::Repeat(expr, _) => check_expr(hir, expr),
            K::Yield(expr, _) => check_expr(hir, expr),
            K::Err(_) => true,
        }
    }

    fn check_stmt(hir: Map, stmt: &rustc_hir::Stmt) -> bool {
        trace!("Checking stmt: {:?}", stmt.span.data());
        use rustc_hir::StmtKind as K;
        match stmt.kind {
            K::Item(_) => true,
            K::Expr(expr) => check_expr(hir, expr),
            K::Semi(expr) => check_expr(hir, expr),
            K::Let(let_stmt) => {
                let_stmt.init.map_or(true, |expr| check_expr(hir, expr))
                    && let_stmt.els.map_or(true, |block| check_block(hir, block))
            }
        }
    }

    fn check_block(hir: Map, block: &rustc_hir::Block) -> bool {
        trace!("Checking block: {:?}", block.span.data());
        for stmt in block.stmts {
            if !check_stmt(hir, stmt) {
                trace!("Failed at stmt: {:?}", stmt.span.data());
                return false;
            }
        }
        block.expr.map_or(true, |expr| check_expr(hir, expr))
    }

    fn log_qpath(qpath: &rustc_hir::QPath) {
        use rustc_hir::QPath as P;
        match qpath {
            P::Resolved(_, path) => {
                // Collect segments of the path as "module::path::to::item"
                let path = path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.as_str())
                    .collect::<Vec<_>>()
                    .join("::");
                trace!("Resolved path: {path}");
            }
            P::TypeRelative(_, seg) => {
                trace!("Type relative path: {}", seg.ident.as_str());
            }
            P::LangItem(item, _) => {
                trace!("Lang item {item:?}");
            }
        }
    }

    let items = hir.items();
    for item_id in items {
        if !check_item(hir, hir.item(item_id)) {
            return false;
        }
    }
    true
}

pub struct Recursion {
    pub callee: LocalDefId,
    pub caller: LocalDefId,
}

/// Ensure that there is no recursion in the user's code.
pub fn no_recursion(tcx: TyCtxt) -> Result<(), Vec<Recursion>> {
    let mut errors = Vec::new();

    info!("Checking for recursion.");
    let hir = tcx.hir();
    let defs = tcx.hir_crate_items(()).definitions();
    for def in defs {
        let path = hir.def_path(def).to_string_no_crate_verbose();
        let item = hir.expect_item(def);
        if let rustc_hir::ItemKind::Fn(sig, ..) = &item.kind {
            trace!("Checking Fn item: {path}");

            if sig.header.constness == rustc_hir::Constness::Const {
                // Const functions are allowed to be recursive.
                trace!("Const function `{path}` is allowed to be recursive.");
                continue;
            }

            let instance_kind = ty::InstanceKind::Item(def.into());
            let mir_callees = tcx.mir_inliner_callees(instance_kind);
            trace!("MIR callees count: {}", mir_callees.len());

            for (callee, gen_args) in mir_callees {
                let callee_path = tcx.def_path_str(callee);

                let instance = ty::Instance {
                    def: instance_kind,
                    args: gen_args,
                };
                // Analyze only local functions. We trust in the external ones, as
                // we were the ones who added them as dependencies.
                if let Some(local) = callee.as_local() {
                    trace!("Inspecting callee: {callee_path}");

                    let reachable = tcx.mir_callgraph_reachable((instance, local));
                    if reachable {
                        error!("Recursion detected in `{path}` calling `{callee_path}`.");
                        errors.push(Recursion {
                            callee: local,
                            caller: def,
                        });
                    }
                } else {
                    trace!("Skipping external callee: {callee_path}");
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Find all public types in the HIR. These can be
/// later used to be registered into the frontend context.
pub fn type_ids(tcx: TyCtxt) -> Vec<DefId> {
    let items = tcx.hir_crate_items(()).free_items();
    let visibilities = tcx.effective_visibilities(());
    let adt_items = items
        .map(|def_id| tcx.hir().item(def_id))
        .filter(|i| i.is_adt());

    let mut vec = SmallVec::<[_; 128]>::new();

    for item in adt_items {
        let id = item.hir_id().as_owner().unwrap().def_id;
        if visibilities.is_directly_public(id) {
            println!("{:?}", item.ident);
            vec.push(id.into());
        }
    }

    vec.into_vec()
}

pub fn types(tcx: TyCtxt) -> Vec<DefPath> {
    type_ids(tcx)
        .into_iter()
        .map(|id| tcx.def_path(id))
        .collect()
}

/// Collect all "impl"s found in the project files.
pub fn impls(tcx: TyCtxt) -> Vec<ItemId> {
    let impl_items = tcx.hir_crate_items(()).free_items();
    let impl_items = impl_items
        .map(|item_id| tcx.hir().item(item_id))
        .filter_map(|item| {
            if let rustc_hir::ItemKind::Impl(_) = &item.kind {
                trace!("Found impl `{}`", item.ident.as_str());
                Some(item.item_id())
            } else {
                trace!("Not an impl `{}`", item.ident.as_str());
                None
            }
        });

    let mut vec = SmallVec::<[_; 128]>::new();
    for item in impl_items {
        vec.push(item.into());
    }
    vec.into_vec()
}

pub struct SinksAndSources {
    pub sinks: Vec<DefId>,
    pub sources: Vec<DefId>,
}

impl SinksAndSources {
    /// Collect all sinks and sources from the project.
    pub fn collect_from(tcx: TyCtxt) -> Self {
        let mut sinks = SmallVec::<[_; 32]>::new();
        let mut sources = SmallVec::<[_; 32]>::new();

        for item_id in impls(tcx) {
            let item = tcx.hir().item(item_id);
            let name = item.ident.as_str();
            if is_impl_sink_trait(tcx, item_id) {
                trace!("Found sink impl `{name}`");
                sinks.push(item.owner_id.to_def_id());
            } else if is_impl_source_trait(tcx, item_id) {
                trace!("Found source impl `{name}`");
                sources.push(item.owner_id.to_def_id());
            } else {
                trace!("Not a sink or source impl `{name}`");
            }
        }

        SinksAndSources {
            sinks: sinks.into_vec(),
            sources: sources.into_vec(),
        }
    }

    /// Filter out all items that are not in the given slice.
    /// Can be used to filter out items that are not in the project or
    /// should be ignored for some other reason. E.g. this can be used to
    /// only retain public items.
    pub fn filter_not_in(&mut self, slice: &[DefId]) {
        self.sinks.retain(|sink| !slice.contains(sink));
        self.sources.retain(|source| !slice.contains(source));
    }
}

/// Whether given item is an implementation of the Sink trait.
fn is_impl_sink_trait(tcx: TyCtxt, item_id: ItemId) -> bool {
    if let rustc_hir::ItemKind::Impl(imp) = &tcx.hir().item(item_id).kind {
        if let Some(of_trait) = imp.of_trait {
            if let Some(def_id) = of_trait.trait_def_id() {
                return is_sink_trait_path(&tcx.def_path(def_id));
            }
        }
    }
    false
}

/// Whether given path is the Sink trait path.
fn is_sink_trait_path(path: &DefPath) -> bool {
    let segs = path
        .data
        .iter()
        .map(|data| data.data.get_opt_name().map(|v| v.to_ident_string()));
    trace!(
        "Is sink trait path? {}",
        segs.clone().map(|v| v.unwrap_or_default()).join("::")
    );
    segs.zip(SINK_TRAIT_PATH)
        .all(|(a, b)| a.as_ref().map(|v| v.as_str()) == Some(*b))
}

/// Whether given path is the Source trait path.
fn is_source_trait_path(path: &DefPath) -> bool {
    let segs = path
        .data
        .iter()
        .map(|data| data.data.get_opt_name().map(|v| v.to_ident_string()));
    trace!(
        "Is source trait path? {}",
        segs.clone().map(|v| v.unwrap_or_default()).join("::")
    );
    segs.zip(SOURCE_TRAIT_PATH)
        .all(|(a, b)| a.as_ref().map(|v| v.as_str()) == Some(*b))
}

/// Whether given item is an implementation of the Source trait.
fn is_impl_source_trait(tcx: TyCtxt, item_id: ItemId) -> bool {
    if let rustc_hir::ItemKind::Impl(imp) = &tcx.hir().item(item_id).kind {
        if let Some(of_trait) = imp.of_trait {
            if let Some(def_id) = of_trait.trait_def_id() {
                return is_source_trait_path(&tcx.def_path(def_id));
            }
        }
    }
    false
}
