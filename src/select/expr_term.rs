use crate::select::cmp::*;
use crate::select::select_value::{SelectValue, SelectValueType};
use crate::select::{to_f64, FilterKey};

#[derive(Debug, PartialEq)]
pub(super) enum ExprTerm<'a, T>
where
    T: SelectValue,
{
    String(String),
    Long(i64),
    Double(f64),
    Bool(bool),
    Json(Option<Vec<&'a T>>, Option<FilterKey>, Vec<&'a T>),
}

impl<'a, T> ExprTerm<'a, T>
where
    T: SelectValue,
{
    fn cmp<C1: Cmp<'a, T>, C2: Cmp<'a, T>>(
        &self,
        other: &Self,
        cmp_fn: &C1,
        reverse_cmp_fn: &C2,
    ) -> ExprTerm<'a, T> {
        match &self {
            ExprTerm::String(s1) => match &other {
                ExprTerm::String(s2) => ExprTerm::Bool(cmp_fn.cmp_string(s1, s2)),
                ExprTerm::Json(_, _, _) => other.cmp(&self, reverse_cmp_fn, cmp_fn),
                _ => ExprTerm::Bool(cmp_fn.default()),
            },
            ExprTerm::Long(n1) => match &other {
                ExprTerm::Long(n2) => ExprTerm::Bool(cmp_fn.cmp_f64(to_f64(*n1), to_f64(*n2))),
                ExprTerm::Json(_, _, _) => other.cmp(&self, reverse_cmp_fn, cmp_fn),
                _ => ExprTerm::Bool(cmp_fn.default()),
            },
            ExprTerm::Double(n1) => match &other {
                ExprTerm::Long(n2) => ExprTerm::Bool(cmp_fn.cmp_f64(*n1, to_f64(*n2))),
                ExprTerm::Json(_, _, _) => other.cmp(&self, reverse_cmp_fn, cmp_fn),
                _ => ExprTerm::Bool(cmp_fn.default()),
            },
            ExprTerm::Bool(b1) => match &other {
                ExprTerm::Bool(b2) => ExprTerm::Bool(cmp_fn.cmp_bool(*b1, *b2)),
                ExprTerm::Json(_, _, _) => other.cmp(&self, reverse_cmp_fn, cmp_fn),
                _ => ExprTerm::Bool(cmp_fn.default()),
            },
            ExprTerm::Json(rel, fk1, vec1) => {
                let ret: Vec<&T> = match &other {
                    ExprTerm::String(s2) => vec1
                        .iter()
                        .filter(|v1| match v1.get_type() {
                            SelectValueType::String => cmp_fn.cmp_string(&v1.get_str(), s2),
                            SelectValueType::Dict => {
                                if let Some(FilterKey::String(k)) = fk1 {
                                    if let Some(tmp) = v1.get_key(k) {
                                        match tmp.get_type() {
                                            SelectValueType::String => {
                                                return cmp_fn.cmp_string(&tmp.get_str(), s2)
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                cmp_fn.default()
                            }
                            _ => cmp_fn.default(),
                        })
                        .cloned()
                        .collect(),
                    ExprTerm::Long(n2) => vec1
                        .iter()
                        .filter(|v1| match v1.get_type() {
                            SelectValueType::Long => {
                                cmp_fn.cmp_f64(to_f64(v1.get_long()), to_f64(*n2))
                            }
                            SelectValueType::Double => cmp_fn.cmp_f64(v1.get_double(), to_f64(*n2)),
                            SelectValueType::Dict => {
                                if let Some(FilterKey::String(k)) = fk1 {
                                    if let Some(tmp) = v1.get_key(k) {
                                        match tmp.get_type() {
                                            SelectValueType::Long => {
                                                return cmp_fn
                                                    .cmp_f64(to_f64(tmp.get_long()), to_f64(*n2))
                                            }
                                            SelectValueType::Double => {
                                                return cmp_fn
                                                    .cmp_f64(tmp.get_double(), to_f64(*n2))
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                cmp_fn.default()
                            }
                            _ => cmp_fn.default(),
                        })
                        .cloned()
                        .collect(),
                    ExprTerm::Double(n2) => vec1
                        .iter()
                        .filter(|v1| match v1.get_type() {
                            SelectValueType::Long => cmp_fn.cmp_f64(to_f64(v1.get_long()), *n2),
                            SelectValueType::Double => cmp_fn.cmp_f64(v1.get_double(), *n2),
                            SelectValueType::Dict => {
                                if let Some(FilterKey::String(k)) = fk1 {
                                    if let Some(tmp) = v1.get_key(k) {
                                        match tmp.get_type() {
                                            SelectValueType::Long => {
                                                return cmp_fn.cmp_f64(to_f64(tmp.get_long()), *n2)
                                            }
                                            SelectValueType::Double => {
                                                return cmp_fn.cmp_f64(tmp.get_double(), *n2)
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                cmp_fn.default()
                            }
                            _ => cmp_fn.default(),
                        })
                        .cloned()
                        .collect(),
                    ExprTerm::Bool(b2) => vec1
                        .iter()
                        .filter(|v1| match v1.get_type() {
                            SelectValueType::Bool => cmp_fn.cmp_bool(v1.get_bool(), *b2),
                            SelectValueType::Dict => {
                                if let Some(FilterKey::String(k)) = fk1 {
                                    if let Some(tmp) = v1.get_key(k) {
                                        match tmp.get_type() {
                                            SelectValueType::Bool => {
                                                return cmp_fn.cmp_bool(tmp.get_bool(), *b2)
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                cmp_fn.default()
                            }
                            _ => cmp_fn.default(),
                        })
                        .cloned()
                        .collect(),
                    ExprTerm::Json(parent, _, vec2) => {
                        if let Some(vec1) = rel {
                            cmp_fn.cmp_json(vec1, vec2)
                        } else if let Some(vec2) = parent {
                            cmp_fn.cmp_json(vec1, vec2)
                        } else {
                            cmp_fn.cmp_json(vec1, vec2)
                        }
                    }
                };

                if ret.is_empty() {
                    ExprTerm::Bool(cmp_fn.default())
                } else if let Some(rel) = rel {
                    if let ExprTerm::Json(_, _, _) = &other {
                        ExprTerm::Json(Some(rel.to_vec()), None, ret)
                    } else {
                        let mut tmp = Vec::new();
                        for rel_value in rel {
                            if rel_value.get_type() == SelectValueType::Dict {
                                for map_value in rel_value.values().unwrap() {
                                    for result_value in &ret {
                                        if (*map_value).eq(*result_value) {
                                            tmp.push(*rel_value);
                                        }
                                    }
                                }
                            }
                        }
                        ExprTerm::Json(Some(tmp), None, ret)
                    }
                } else {
                    ExprTerm::Json(None, None, ret)
                }
            }
        }
    }

    pub fn eq(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("eq - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpEq, &CmpEq);
        debug!("eq = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn ne(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("ne - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpNe, &CmpNe);
        debug!("ne = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn gt(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("gt - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpGt, &CmpLt);
        debug!("gt = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn ge(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("ge - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpGe, &CmpLe);
        debug!("ge = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn lt(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("lt - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpLt, &CmpGt);
        debug!("lt = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn le(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("le - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpLe, &CmpGe);
        debug!("le = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn and(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("and - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpAnd, &CmpAnd);
        debug!("and = {:?}", tmp);
        *ret = Some(tmp);
    }

    pub fn or(&self, other: &Self, ret: &mut Option<ExprTerm<'a, T>>) {
        debug!("or - {:?} : {:?}", &self, &other);
        let _ = ret.take();
        let tmp = self.cmp(other, &CmpOr, &CmpOr);
        debug!("or = {:?}", tmp);
        *ret = Some(tmp);
    }
}

impl<'a, T> Into<ExprTerm<'a, T>> for &Vec<&'a T>
where
    T: SelectValue,
{
    fn into(self) -> ExprTerm<'a, T> {
        if self.len() == 1 {
            match &self[0].get_type() {
                SelectValueType::Long => return ExprTerm::Long(self[0].get_long()),
                SelectValueType::Double => return ExprTerm::Double(self[0].get_double()),
                SelectValueType::String => return ExprTerm::String(self[0].get_str()),
                SelectValueType::Bool => return ExprTerm::Bool(self[0].get_bool()),
                _ => {}
            }
        }

        ExprTerm::Json(None, None, self.to_vec())
    }
}

// #[cfg(test)]
// mod expr_term_inner_tests {
//     use serde_json::{Number, Value};
//     use crate::select::expr_term::ExprTerm;

//     #[test]
//     fn value_vec_into() {
//         let v = Value::Bool(true);
//         let vec = &vec![&v];
//         let term: ExprTerm = vec.into();
//         assert_eq!(term, ExprTerm::Bool(true));

//         let v = Value::String("a".to_string());
//         let vec = &vec![&v];
//         let term: ExprTerm = vec.into();
//         assert_eq!(term, ExprTerm::String("a".to_string()));

//         let v = serde_json::from_str("1.0").unwrap();
//         let vec = &vec![&v];
//         let term: ExprTerm = vec.into();
//         assert_eq!(term, ExprTerm::Number(Number::from_f64(1.0).unwrap()));
//     }
// }
