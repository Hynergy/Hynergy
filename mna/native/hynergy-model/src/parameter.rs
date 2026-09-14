use hynergy_ids::define_id;
use thiserror::Error;

define_id!(ParameterId);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bound {
    pub value: f64,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ParameterConstraints {
    lower: Option<Bound>,
    upper: Option<Bound>,
    non_zero: bool,
    reciprocal_range: Option<(Option<Bound>, Option<Bound>)>,
}

#[derive(Debug, Error, Clone, Copy, Eq, PartialEq)]
pub enum ParameterConstraintError {
    #[error("parameter must be finite")]
    NonFinite,

    #[error("parameter must be non-zero")]
    ZeroNotAllowed,

    #[error("parameter is outside the allowed range")]
    OutOfRange,

    #[error("parameter reciprocal is outside the allowed range")]
    ReciprocalOutOfRange,
}

impl ParameterConstraints {
    pub fn new(
        lower: Option<Bound>,
        upper: Option<Bound>,
        non_zero: bool,
        reciprocal_range: Option<(Option<Bound>, Option<Bound>)>,
    ) -> Self {
        Self {
            lower,
            upper,
            non_zero,
            reciprocal_range,
        }
    }

    pub fn validate(&self, param: f64) -> Result<(), ParameterConstraintError> {
        if !param.is_finite() {
            return Err(ParameterConstraintError::NonFinite);
        }

        if self.non_zero && param == 0.0 {
            return Err(ParameterConstraintError::ZeroNotAllowed);
        }

        if !within_bounds(param, self.lower, self.upper) {
            return Err(ParameterConstraintError::OutOfRange);
        }

        if !self.reciprocal_range.is_none_or(|(lower, upper)| {
            let reciprocal = 1.0 / param;

            reciprocal.is_finite() && within_bounds(reciprocal, lower, upper)
        }) {
            return Err(ParameterConstraintError::ReciprocalOutOfRange);
        }

        Ok(())
    }

    pub(crate) fn is_subset_of(&self, required: &Self) -> bool {
        if !lower_bound_implies(self.lower, required.lower)
            || !upper_bound_implies(self.upper, required.upper)
        {
            return false;
        }

        let excludes_zero = self.excludes_zero();

        if required.non_zero && !excludes_zero {
            return false;
        }

        reciprocal_range_implies(
            self.reciprocal_range,
            required.reciprocal_range,
            excludes_zero,
        )
    }

    #[inline]
    fn excludes_zero(&self) -> bool {
        self.non_zero
            || self.reciprocal_range.is_some()
            || self
                .lower
                .is_some_and(|bound| bound.value > 0.0 || (bound.value == 0.0 && !bound.inclusive))
            || self
                .upper
                .is_some_and(|bound| bound.value < 0.0 || (bound.value == 0.0 && !bound.inclusive))
    }
}

#[inline]
fn lower_bound_implies(provided: Option<Bound>, required: Option<Bound>) -> bool {
    let Some(required) = required else {
        return true;
    };

    let Some(provided) = provided else {
        return false;
    };

    provided.value > required.value
        || (provided.value == required.value && (!provided.inclusive || required.inclusive))
}

#[inline]
fn upper_bound_implies(provided: Option<Bound>, required: Option<Bound>) -> bool {
    let Some(required) = required else {
        return true;
    };

    let Some(provided) = provided else {
        return false;
    };

    provided.value < required.value
        || (provided.value == required.value && (!provided.inclusive || required.inclusive))
}

#[inline]
fn reciprocal_range_implies(
    provided: Option<(Option<Bound>, Option<Bound>)>,
    required: Option<(Option<Bound>, Option<Bound>)>,
    provided_excludes_zero: bool,
) -> bool {
    let Some((required_lower, required_upper)) = required else {
        return true;
    };

    if required_lower.is_none() && required_upper.is_none() {
        return provided_excludes_zero;
    }

    let Some((provided_lower, provided_upper)) = provided else {
        return false;
    };

    lower_bound_implies(provided_lower, required_lower)
        && upper_bound_implies(provided_upper, required_upper)
}

#[inline]
fn within_bounds(value: f64, lower: Option<Bound>, upper: Option<Bound>) -> bool {
    lower.is_none_or(|b| {
        if b.inclusive {
            value >= b.value
        } else {
            value > b.value
        }
    }) && upper.is_none_or(|b| {
        if b.inclusive {
            value <= b.value
        } else {
            value < b.value
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{Bound, ParameterConstraintError, ParameterConstraints};

    #[test]
    fn reciprocal_bounds_are_applied_to_reciprocal() {
        let constraints = ParameterConstraints::new(
            None,
            None,
            false,
            Some((
                Some(Bound {
                    value: 0.4,
                    inclusive: true,
                }),
                Some(Bound {
                    value: 0.6,
                    inclusive: true,
                }),
            )),
        );

        assert_eq!(constraints.validate(2.0), Ok(()));
        assert_eq!(
            constraints.validate(0.5),
            Err(ParameterConstraintError::ReciprocalOutOfRange)
        );
    }

    #[test]
    fn inclusive_bounds_accept_boundary_values() {
        let constraints = ParameterConstraints::new(
            Some(Bound {
                value: 1.0,
                inclusive: true,
            }),
            Some(Bound {
                value: 2.0,
                inclusive: true,
            }),
            false,
            None,
        );

        assert_eq!(constraints.validate(1.0), Ok(()));
        assert_eq!(constraints.validate(2.0), Ok(()));
    }

    #[test]
    fn exclusive_bounds_reject_boundary_values() {
        let constraints = ParameterConstraints::new(
            Some(Bound {
                value: 1.0,
                inclusive: false,
            }),
            Some(Bound {
                value: 2.0,
                inclusive: false,
            }),
            false,
            None,
        );

        assert_eq!(
            constraints.validate(1.0),
            Err(ParameterConstraintError::OutOfRange)
        );
        assert_eq!(
            constraints.validate(2.0),
            Err(ParameterConstraintError::OutOfRange)
        );
        assert_eq!(constraints.validate(1.5), Ok(()));
    }

    #[test]
    fn non_finite_input_is_rejected() {
        let constraints = ParameterConstraints::default();

        assert_eq!(
            constraints.validate(f64::NAN),
            Err(ParameterConstraintError::NonFinite)
        );
        assert_eq!(
            constraints.validate(f64::INFINITY),
            Err(ParameterConstraintError::NonFinite)
        );
    }

    #[test]
    fn non_zero_constraint_rejects_zero() {
        let constraints = ParameterConstraints::new(None, None, true, None);

        assert_eq!(
            constraints.validate(0.0),
            Err(ParameterConstraintError::ZeroNotAllowed)
        );
    }

    #[test]
    fn non_finite_reciprocal_is_rejected() {
        let constraints = ParameterConstraints::new(
            None,
            None,
            true,
            Some((
                None,
                Some(Bound {
                    value: f64::MAX,
                    inclusive: true,
                }),
            )),
        );

        assert_eq!(
            constraints.validate(f64::from_bits(1)),
            Err(ParameterConstraintError::ReciprocalOutOfRange)
        );
    }

    #[test]
    fn constraint_subset_accepts_equal_constraints() {
        let constraints = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            None,
            false,
            None,
        );

        assert!(constraints.is_subset_of(&constraints));
    }

    #[test]
    fn constraint_subset_accepts_stricter_bounds() {
        let provided = ParameterConstraints::new(
            Some(Bound {
                value: 1.0,
                inclusive: true,
            }),
            Some(Bound {
                value: 10.0,
                inclusive: false,
            }),
            false,
            None,
        );

        let required = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            Some(Bound {
                value: 10.0,
                inclusive: true,
            }),
            false,
            None,
        );

        assert!(provided.is_subset_of(&required));
    }

    #[test]
    fn constraint_subset_rejects_weaker_lower_bound() {
        let provided = ParameterConstraints::default();

        let required = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            None,
            false,
            None,
        );

        assert!(!provided.is_subset_of(&required));
    }

    #[test]
    fn exclusive_zero_bound_implies_non_zero() {
        let provided = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            None,
            false,
            None,
        );

        let required = ParameterConstraints::new(None, None, true, None);

        assert!(provided.is_subset_of(&required));
    }

    #[test]
    fn reciprocal_constraint_implies_non_zero() {
        let provided = ParameterConstraints::new(None, None, false, Some((None, None)));

        let required = ParameterConstraints::new(None, None, true, None);

        assert!(provided.is_subset_of(&required));
    }

    #[test]
    fn constraint_subset_rejects_missing_required_reciprocal_bound() {
        let provided = ParameterConstraints::new(
            Some(Bound {
                value: 1.0,
                inclusive: true,
            }),
            None,
            false,
            None,
        );

        let required = ParameterConstraints::new(
            None,
            None,
            false,
            Some((
                Some(Bound {
                    value: 0.1,
                    inclusive: true,
                }),
                None,
            )),
        );

        assert!(!provided.is_subset_of(&required));
    }
}
