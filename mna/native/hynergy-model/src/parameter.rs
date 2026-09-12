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
}
