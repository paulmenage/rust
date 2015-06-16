// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Code for type-checking closure expressions.

use super::{check_fn, Expectation, FnCtxt};

use astconv;
use middle::region;
use middle::subst;
use middle::ty::{self, ToPolyTraitRef, Ty};
use std::cmp;
use syntax::abi;
use syntax::ast;
use syntax::ast_util;
use util::ppaux::Repr;

pub fn check_expr_closure<'a,'tcx>(fcx: &FnCtxt<'a,'tcx>,
                                   expr: &ast::Expr,
                                   _capture: ast::CaptureClause,
                                   decl: &'tcx ast::FnDecl,
                                   body: &'tcx ast::Block,
                                   expected: Expectation<'tcx>) {
    debug!("check_expr_closure(expr={},expected={})",
           expr.repr(),
           expected.repr());

    // It's always helpful for inference if we know the kind of
    // closure sooner rather than later, so first examine the expected
    // type, and see if can glean a closure kind from there.
    let (expected_sig,expected_kind) = match expected.to_option(fcx) {
        Some(ty) => deduce_expectations_from_expected_type(fcx, ty),
        None => (None, None)
    };
    check_closure(fcx, expr, expected_kind, decl, body, expected_sig)
}

fn check_closure<'a,'tcx>(fcx: &FnCtxt<'a,'tcx>,
                          expr: &ast::Expr,
                          opt_kind: Option<ty::ClosureKind>,
                          decl: &'tcx ast::FnDecl,
                          body: &'tcx ast::Block,
                          expected_sig: Option<ty::FnSig<'tcx>>) {
    let expr_def_id = ast_util::local_def(expr.id);

    debug!("check_closure opt_kind={:?} expected_sig={}",
           opt_kind,
           expected_sig.repr());

    let mut fn_ty = astconv::ty_of_closure(
        fcx,
        ast::Unsafety::Normal,
        decl,
        abi::RustCall,
        expected_sig);

    let closure_type = ty::mk_closure(fcx.ccx.tcx,
                                      expr_def_id,
                                      fcx.ccx.tcx.mk_substs(
                                        fcx.inh.param_env.free_substs.clone()));

    fcx.write_ty(expr.id, closure_type);

    let fn_sig =
        ty::liberate_late_bound_regions(fcx.tcx(),
                                        region::DestructionScopeData::new(body.id),
                                        &fn_ty.sig);

    check_fn(fcx.ccx,
             ast::Unsafety::Normal,
             expr.id,
             &fn_sig,
             decl,
             expr.id,
             &*body,
             fcx.inh);

    // Tuple up the arguments and insert the resulting function type into
    // the `closures` table.
    fn_ty.sig.0.inputs = vec![ty::mk_tup(fcx.tcx(), fn_ty.sig.0.inputs)];

    debug!("closure for {} --> sig={} opt_kind={:?}",
           expr_def_id.repr(),
           fn_ty.sig.repr(),
           opt_kind);

    fcx.inh.closure_tys.borrow_mut().insert(expr_def_id, fn_ty);
    match opt_kind {
        Some(kind) => { fcx.inh.closure_kinds.borrow_mut().insert(expr_def_id, kind); }
        None => { }
    }
}

fn deduce_expectations_from_expected_type<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    expected_ty: Ty<'tcx>)
    -> (Option<ty::FnSig<'tcx>>,Option<ty::ClosureKind>)
{
    debug!("deduce_expectations_from_expected_type(expected_ty={})",
           expected_ty.repr());

    match expected_ty.sty {
        ty::TyTrait(ref object_type) => {
            let proj_bounds = object_type.projection_bounds_with_self_ty(fcx.tcx(),
                                                                         fcx.tcx().types.err);
            let sig = proj_bounds.iter()
                                 .filter_map(|pb| deduce_sig_from_projection(fcx, pb))
                                 .next();
            let kind = fcx.tcx().lang_items.fn_trait_kind(object_type.principal_def_id());
            (sig, kind)
        }
        ty::TyInfer(ty::TyVar(vid)) => {
            deduce_expectations_from_obligations(fcx, vid)
        }
        _ => {
            (None, None)
        }
    }
}

fn deduce_expectations_from_obligations<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    expected_vid: ty::TyVid)
    -> (Option<ty::FnSig<'tcx>>, Option<ty::ClosureKind>)
{
    let fulfillment_cx = fcx.inh.fulfillment_cx.borrow();
    // Here `expected_ty` is known to be a type inference variable.

    let expected_sig =
        fulfillment_cx
        .pending_obligations()
        .iter()
        .filter_map(|obligation| {
            debug!("deduce_expectations_from_obligations: obligation.predicate={}",
                   obligation.predicate.repr());

            match obligation.predicate {
                // Given a Projection predicate, we can potentially infer
                // the complete signature.
                ty::Predicate::Projection(ref proj_predicate) => {
                    let trait_ref = proj_predicate.to_poly_trait_ref();
                    self_type_matches_expected_vid(fcx, trait_ref, expected_vid)
                        .and_then(|_| deduce_sig_from_projection(fcx, proj_predicate))
                }
                _ => {
                    None
                }
            }
        })
        .next();

    // Even if we can't infer the full signature, we may be able to
    // infer the kind. This can occur if there is a trait-reference
    // like `F : Fn<A>`. Note that due to subtyping we could encounter
    // many viable options, so pick the most restrictive.
    let expected_kind =
        fulfillment_cx
        .pending_obligations()
        .iter()
        .filter_map(|obligation| {
            let opt_trait_ref = match obligation.predicate {
                ty::Predicate::Projection(ref data) => Some(data.to_poly_trait_ref()),
                ty::Predicate::Trait(ref data) => Some(data.to_poly_trait_ref()),
                ty::Predicate::Equate(..) => None,
                ty::Predicate::RegionOutlives(..) => None,
                ty::Predicate::TypeOutlives(..) => None,
            };
            opt_trait_ref
                .and_then(|trait_ref| self_type_matches_expected_vid(fcx, trait_ref, expected_vid))
                .and_then(|trait_ref| fcx.tcx().lang_items.fn_trait_kind(trait_ref.def_id()))
        })
        .fold(None, pick_most_restrictive_closure_kind);

    (expected_sig, expected_kind)
}

fn pick_most_restrictive_closure_kind(best: Option<ty::ClosureKind>,
                                      cur: ty::ClosureKind)
                                      -> Option<ty::ClosureKind>
{
    match best {
        None => Some(cur),
        Some(best) => Some(cmp::min(best, cur))
    }
}

/// Given a projection like "<F as Fn(X)>::Result == Y", we can deduce
/// everything we need to know about a closure.
fn deduce_sig_from_projection<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    projection: &ty::PolyProjectionPredicate<'tcx>)
    -> Option<ty::FnSig<'tcx>>
{
    let tcx = fcx.tcx();

    debug!("deduce_sig_from_projection({})",
           projection.repr());

    let trait_ref = projection.to_poly_trait_ref();

    if tcx.lang_items.fn_trait_kind(trait_ref.def_id()).is_none() {
        return None;
    }

    let arg_param_ty = *trait_ref.substs().types.get(subst::TypeSpace, 0);
    let arg_param_ty = fcx.infcx().resolve_type_vars_if_possible(&arg_param_ty);
    debug!("deduce_sig_from_projection: arg_param_ty {}", arg_param_ty.repr());

    let input_tys = match arg_param_ty.sty {
        ty::TyTuple(ref tys) => { (*tys).clone() }
        _ => { return None; }
    };
    debug!("deduce_sig_from_projection: input_tys {}", input_tys.repr());

    let ret_param_ty = projection.0.ty;
    let ret_param_ty = fcx.infcx().resolve_type_vars_if_possible(&ret_param_ty);
    debug!("deduce_sig_from_projection: ret_param_ty {}", ret_param_ty.repr());

    let fn_sig = ty::FnSig {
        inputs: input_tys,
        output: ty::FnConverging(ret_param_ty),
        variadic: false
    };
    debug!("deduce_sig_from_projection: fn_sig {}", fn_sig.repr());

    Some(fn_sig)
}

fn self_type_matches_expected_vid<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    trait_ref: ty::PolyTraitRef<'tcx>,
    expected_vid: ty::TyVid)
    -> Option<ty::PolyTraitRef<'tcx>>
{
    let self_ty = fcx.infcx().shallow_resolve(trait_ref.self_ty());
    debug!("self_type_matches_expected_vid(trait_ref={}, self_ty={})",
           trait_ref.repr(),
           self_ty.repr());
    match self_ty.sty {
        ty::TyInfer(ty::TyVar(v)) if expected_vid == v => Some(trait_ref),
        _ => None,
    }
}
