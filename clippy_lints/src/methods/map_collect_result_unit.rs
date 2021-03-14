use crate::utils::{is_trait_method, span_lint_and_sugg};
use clippy_utils::source::snippet;
use clippy_utils::ty::is_type_diagnostic_item;
use if_chain::if_chain;
use rustc_errors::Applicability;
use rustc_hir as hir;
use rustc_lint::LateContext;
use rustc_middle::ty;
use rustc_span::symbol::sym;

use super::MAP_COLLECT_RESULT_UNIT;

pub(super) fn check(
    cx: &LateContext<'_>,
    expr: &hir::Expr<'_>,
    map_args: &[hir::Expr<'_>],
    collect_args: &[hir::Expr<'_>],
) {
    if_chain! {
        // called on Iterator
        if let [map_expr] = collect_args;
        if is_trait_method(cx, map_expr, sym::Iterator);
        // return of collect `Result<(),_>`
        let collect_ret_ty = cx.typeck_results().expr_ty(expr);
        if is_type_diagnostic_item(cx, collect_ret_ty, sym::result_type);
        if let ty::Adt(_, substs) = collect_ret_ty.kind();
        if let Some(result_t) = substs.types().next();
        if result_t.is_unit();
        // get parts for snippet
        if let [iter, map_fn] = map_args;
        then {
            span_lint_and_sugg(
                cx,
                MAP_COLLECT_RESULT_UNIT,
                expr.span,
                "`.map().collect()` can be replaced with `.try_for_each()`",
                "try this",
                format!(
                    "{}.try_for_each({})",
                    snippet(cx, iter.span, ".."),
                    snippet(cx, map_fn.span, "..")
                ),
                Applicability::MachineApplicable,
            );
        }
    }
}
