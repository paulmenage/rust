//! This pass transforms derefs of Box into a deref of the pointer inside Box
//! Codegen does not allow box to be directly dereferenced

use crate::MirPass;
use rustc_hir::def_id::DefId;
use rustc_index::vec::Idx;
use rustc_middle::mir::patch::MirPatch;
use rustc_middle::mir::visit::MutVisitor;
use rustc_middle::mir::*;
use rustc_middle::ty::subst::Subst;
use rustc_middle::ty::TyCtxt;

struct ElaborateBoxDerefVistor<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    unique_did: DefId,
    nonnull_did: DefId,
    local_decls: &'a mut LocalDecls<'tcx>,
    patch: MirPatch<'tcx>,
}

impl<'tcx, 'a> MutVisitor<'tcx> for ElaborateBoxDerefVistor<'tcx, 'a> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

    fn visit_place(
        &mut self,
        place: &mut Place<'tcx>,
        context: visit::PlaceContext,
        location: Location,
    ) {
        let tcx = self.tcx;

        let base_ty = self.local_decls[place.local].ty;

        // Derefer ensures that derefs are always the first projection
        if place.projection.first() == Some(&PlaceElem::Deref) && base_ty.is_box() {
            let source_info = self.local_decls[place.local].source_info;

            let substs = tcx.intern_substs(&[base_ty.boxed_ty().into()]);
            let unique_ty = tcx.bound_type_of(self.unique_did).subst(tcx, substs);
            let nonnull_ty = tcx.bound_type_of(self.nonnull_did).subst(tcx, substs);
            let ptr_ty = tcx.mk_imm_ptr(base_ty.boxed_ty());

            let ptr_local = self.patch.new_temp(ptr_ty, source_info.span);
            self.local_decls.push(LocalDecl::new(ptr_ty, source_info.span));

            self.patch.add_statement(location, StatementKind::StorageLive(ptr_local));

            self.patch.add_assign(
                location,
                Place::from(ptr_local),
                Rvalue::Use(Operand::Copy(Place::from(place.local).project_deeper(
                    &[
                        PlaceElem::Field(Field::new(0), unique_ty),
                        PlaceElem::Field(Field::new(0), nonnull_ty),
                        PlaceElem::Field(Field::new(0), ptr_ty),
                    ],
                    tcx,
                ))),
            );

            place.local = ptr_local;

            self.patch.add_statement(
                Location { block: location.block, statement_index: location.statement_index + 1 },
                StatementKind::StorageDead(ptr_local),
            );
        }

        self.super_place(place, context, location);
    }
}

pub struct ElaborateBoxDerefs;

impl<'tcx> MirPass<'tcx> for ElaborateBoxDerefs {
    fn run_pass(&self, tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) {
        if let Some(def_id) = tcx.lang_items().owned_box() {
            let unique_did = tcx.adt_def(def_id).non_enum_variant().fields[0].did;

            let Some(nonnull_def) = tcx.type_of(unique_did).ty_adt_def() else {
                span_bug!(tcx.def_span(unique_did), "expected Box to contain Unique")
            };

            let nonnull_did = nonnull_def.non_enum_variant().fields[0].did;

            let patch = MirPatch::new(body);

            let (basic_blocks, local_decls) = body.basic_blocks_and_local_decls_mut();

            let mut visitor =
                ElaborateBoxDerefVistor { tcx, unique_did, nonnull_did, local_decls, patch };

            for (block, BasicBlockData { statements, terminator, .. }) in
                basic_blocks.iter_enumerated_mut()
            {
                let mut index = 0;
                for statement in statements {
                    let location = Location { block, statement_index: index };
                    visitor.visit_statement(statement, location);
                    index += 1;
                }

                if let Some(terminator) = terminator
                && !matches!(terminator.kind, TerminatorKind::Yield{..})
                {
                    let location = Location { block, statement_index: index };
                    visitor.visit_terminator(terminator, location);
                }

                let location = Location { block, statement_index: index };
                match terminator {
                    // yielding into a box is handed when lowering generators
                    Some(Terminator { kind: TerminatorKind::Yield { value, .. }, .. }) => {
                        visitor.visit_operand(value, location);
                    }
                    Some(terminator) => {
                        visitor.visit_terminator(terminator, location);
                    }
                    None => {}
                }
            }

            visitor.patch.apply(body);
        } else {
            // box is not present, this pass doesn't need to do anything
        }
    }
}
