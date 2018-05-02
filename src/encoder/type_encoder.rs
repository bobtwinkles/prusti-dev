// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rustc::ty;
use encoder::Encoder;
use encoder::vir;
use encoder::vir::utils::ExprIterator;

pub struct TypeEncoder<'p, 'v: 'p, 'r: 'v, 'a: 'r, 'tcx: 'a> {
    encoder: &'p Encoder<'v, 'r, 'a, 'tcx>,
    ty: ty::Ty<'tcx>
}

impl<'p, 'v, 'r: 'v, 'a: 'r, 'tcx: 'a> TypeEncoder<'p, 'v, 'r, 'a, 'tcx> {
    pub fn new(encoder: &'p Encoder<'v, 'r, 'a, 'tcx>, ty: ty::Ty<'tcx>) -> Self {
        TypeEncoder {
            encoder,
            ty
        }
    }

    pub fn encode_value_field(self) -> vir::Field {
        debug!("Encode value field for type '{:?}'", self.ty);
        match self.ty.sty {
            ty::TypeVariants::TyBool => vir::Field::new("val_bool", vir::Type::Bool),

            ty::TypeVariants::TyInt(_) => vir::Field::new("val_int", vir::Type::Int),
            ty::TypeVariants::TyUint(_) => vir::Field::new("val_uint", vir::Type::Int),

            ty::TypeVariants::TyRawPtr(_) |
            ty::TypeVariants::TyRef(_, _) => vir::Field::new("val_ref", vir::Type::Ref),

            ty::TypeVariants::TyAdt(_, _) |
            ty::TypeVariants::TyTuple(_) => unimplemented!(),

            ref x => unimplemented!("{:?}", x),
        }
    }

    pub fn encode_value_field_name(self) -> String {
        debug!("Encode value field name for type '{:?}'", self.ty);
        match self.ty.sty {
            ty::TypeVariants::TyBool => "val_bool",

            ty::TypeVariants::TyInt(_) => "val_int",
            ty::TypeVariants::TyUint(_) => "val_uint",

            ty::TypeVariants::TyRawPtr(_) |
            ty::TypeVariants::TyRef(_, _) => "val_ref",

            ty::TypeVariants::TyAdt(_, _) |
            ty::TypeVariants::TyTuple(_) => unimplemented!(),

            ref x => unimplemented!("{:?}", x),
        }.to_string()
    }

    pub fn encode_predicate_def(self) -> vir::Predicate {
        debug!("Encode type predicate '{:?}'", self.ty);
        let predicate_name = self.encoder.encode_type_predicate_use(self.ty);
        let self_local_var = vir::LocalVar::new("self", vir::Type::Ref);

        let field_predicates = match self.ty.sty {
            ty::TypeVariants::TyBool |
            ty::TypeVariants::TyInt(_) |
            ty::TypeVariants::TyUint(_) =>
                vec![
                    vir::Expr::FieldAccessPredicate(
                        box vir::Place::from(self_local_var).access(
                            self.encoder.encode_value_field(self.ty)
                        ).into(),
                        vir::Perm::full()
                    )
                ],

            ty::TypeVariants::TyRawPtr(ty::TypeAndMut { ref ty, .. }) |
            ty::TypeVariants::TyRef(_, ty::TypeAndMut { ref ty, .. }) => {
                let predicate_name = self.encoder.encode_type_predicate_use(ty);
                let elem_field = self.encoder.encode_ref_field("val_ref");
                let elem_loc = vir::Place::from(self_local_var).access(elem_field);
                vec![
                    vir::Expr::FieldAccessPredicate(
                        box elem_loc.clone().into(),
                        vir::Perm::full()
                    ),
                    vir::Expr::PredicateAccessPredicate(
                        box vir::Expr::PredicateAccess(
                            predicate_name,
                            vec![ elem_loc.into() ]
                        ),
                        vir::Perm::full()
                    ),
                ]
            }

            ty::TypeVariants::TyTuple(elems) => {
                elems.iter().enumerate().flat_map(|(field_num, ty)| {
                    let field_name = format!("tuple_{}", field_num);
                    let elem_field = self.encoder.encode_ref_field(&field_name);
                    let predicate_name = self.encoder.encode_type_predicate_use(ty);
                    let elem_loc = vir::Place::from(self_local_var.clone()).access(elem_field);
                    vec![
                        vir::Expr::FieldAccessPredicate(
                            box elem_loc.clone().into(),
                            vir::Perm::full()
                        ),
                        vir::Expr::PredicateAccessPredicate(
                            box vir::Expr::PredicateAccess(
                                predicate_name,
                                vec![ elem_loc.into() ]
                            ),
                            vir::Perm::full()
                        ),
                    ]
                }).collect()
            },

            ty::TypeVariants::TyAdt(ref adt_def, ref subst)  => {
                let mut perms: Vec<vir::Expr> = vec![];
                let num_variants = adt_def.variants.len();
                let tcx = self.encoder.env().tcx();
                if num_variants == 1 {
                    debug!("ADT has only one variant: {:?}", adt_def);
                    for field in &adt_def.variants[0].fields {
                        debug!("Encoding field {:?}", field);
                        let field_name = format!("struct_{}", field.name);
                        let field_ty = field.ty(tcx, subst);
                        let elem_field = self.encoder.encode_ref_field(&field_name);
                        let predicate_name = self.encoder.encode_type_predicate_use(field_ty);
                        let elem_loc = vir::Place::from(self_local_var.clone()).access(elem_field);
                        perms.push(
                            vir::Expr::FieldAccessPredicate(
                                box elem_loc.clone().into(),
                                vir::Perm::full()
                            )
                        );
                        perms.push(
                            vir::Expr::PredicateAccessPredicate(
                                box vir::Expr::PredicateAccess(
                                    predicate_name,
                                    vec![ elem_loc.into() ]
                                ),
                                vir::Perm::full()
                            )
                        )
                    }
                } else if num_variants > 1 {
                    debug!("ADT has {} variants: {:?}", num_variants, adt_def);
                    let discriminant_field = self.encoder.encode_discriminant_field();
                    let discriminan_loc = vir::Place::from(self_local_var.clone()).access(discriminant_field);
                    // acc(self.discriminant)
                    perms.push(
                        vir::Expr::FieldAccessPredicate(
                            box discriminan_loc.clone().into(),
                            vir::Perm::full()
                        )
                    );
                    // 0 <= self.discriminant <= num_variants - 1
                    perms.push(
                        vir::Expr::and(
                            vir::Expr::le_cmp(
                                0.into(),
                                discriminan_loc.clone().into()
                            ),
                            vir::Expr::le_cmp(
                                discriminan_loc.into(),
                                (num_variants - 1).into()
                            )
                        )
                    );
                    for (variant_index, variant_def) in adt_def.variants.iter().enumerate() {
                        debug!("Encoding variant {:?}", variant_def);
                        assert!(variant_index as u128 == adt_def.discriminant_for_variant(tcx, variant_index).val);
                        for field in &variant_def.fields {
                            debug!("Encoding field {:?}", field);
                            let field_name = format!("enum_{}_{}", variant_index, field.name);
                            let field_ty = field.ty(tcx, subst);
                            let elem_field = self.encoder.encode_ref_field(&field_name);
                            let predicate_name = self.encoder.encode_type_predicate_use(field_ty);
                            let elem_loc = vir::Place::from(self_local_var.clone()).access(elem_field);
                            perms.push(
                                vir::Expr::FieldAccessPredicate(
                                    box elem_loc.clone().into(),
                                    vir::Perm::full()
                                )
                            );
                            perms.push(
                                vir::Expr::PredicateAccessPredicate(
                                    box vir::Expr::PredicateAccess(
                                        predicate_name,
                                        vec![ elem_loc.into() ]
                                    ),
                                    vir::Perm::full()
                                )
                            )
                        }
                    }
                }
                perms
            },

            ref x => unimplemented!("{:?}", x),
        };

        vir::Predicate::new(
            predicate_name,
            vec![
                vir::LocalVar::new("self", vir::Type::Ref)
            ],
            Some(
                field_predicates.into_iter().conjoin()
            )
        )
    }

    pub fn encode_predicate_use(self) -> String {
        debug!("Encode type predicate name '{:?}'", self.ty);

        match self.ty.sty {
            ty::TypeVariants::TyBool => "bool".to_string(),

            ty::TypeVariants::TyInt(_) => "int".to_string(),
            ty::TypeVariants::TyUint(_) => "uint".to_string(),

            ty::TypeVariants::TyRawPtr(ty::TypeAndMut { ref ty, .. }) |
            ty::TypeVariants::TyRef(_, ty::TypeAndMut { ref ty, .. }) =>
                format!("ref${}", self.encoder.encode_type_predicate_use(ty)),

            ty::TypeVariants::TyAdt(&ty::AdtDef { did, .. }, _) =>
                self.encoder.encode_type_name(did),

            ty::TypeVariants::TyTuple(elems) => {
                let elem_predicate_names: Vec<String> = elems.iter().map(
                    |ty| self.encoder.encode_type_predicate_use(ty)
                ).collect();
                format!("tuple{}${}", elems.len(), elem_predicate_names.join("$"))
            }

            ref x => unimplemented!("{:?}", x),
        }
    }

    pub fn encode_fields(self) -> Vec<vir::Field> {
        debug!("Encode fields of '{:?}'", self.ty);

        match self.ty.sty {
            ty::TypeVariants::TyBool |
            ty::TypeVariants::TyInt(_) |
            ty::TypeVariants::TyUint(_) => {
                vec![
                    self.encoder.encode_value_field(self.ty)
                ]
            }

            ty::TypeVariants::TyRawPtr(ty::TypeAndMut{ .. }) |
            ty::TypeVariants::TyRef(_, ty::TypeAndMut{ .. }) => {
                vec![
                    self.encoder.encode_value_field(self.ty)
                ]
            }

            ty::TypeVariants::TyTuple(elems) => {
                elems.iter().enumerate().map(|(field_num, _ty)| {
                    let field_name = format!("tuple_{}", field_num);
                    let encoded_field = self.encoder.encode_ref_field(&field_name);
                    encoded_field
                }).collect()
            },

            ty::TypeVariants::TyAdt(ref adt_def, ref _subst) => {
                let mut fields = vec![];
                let num_variants = adt_def.variants.len();
                if num_variants > 1 {
                    let discr_field = self.encoder.encode_discriminant_field();
                    fields.push(discr_field);
                }
                let tcx = self.encoder.env().tcx();
                for (variant_index, variant_def) in adt_def.variants.iter().enumerate() {
                    assert!(variant_index as u128 == adt_def.discriminant_for_variant(tcx, variant_index).val);
                    for field in &variant_def.fields {
                        let field_name = if num_variants == 1 {
                            format!("struct_{}", field.name)
                        } else {
                            format!("enum_{}_{}", variant_index, field.name)
                        };
                        let encoded_field = self.encoder.encode_ref_field(&field_name);
                        //let ty = field.ty(tcx, subst);
                        fields.push(encoded_field)
                    }
                }
                fields
            },

            ref x => unimplemented!("{:?}", x),
        }
    }
}
