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

    fn bound(value: f64, inclusive: bool) -> Bound {
        Bound { value, inclusive }
    }

    #[test]
    fn validate_enforces_finiteness_bounds_and_non_zero() {
        let inclusive_range =
            ParameterConstraints::new(Some(bound(1.0, true)), Some(bound(2.0, true)), false, None);

        let exclusive_range = ParameterConstraints::new(
            Some(bound(1.0, false)),
            Some(bound(2.0, false)),
            false,
            None,
        );

        let non_zero = ParameterConstraints::new(None, None, true, None);

        let cases = [
            (
                ParameterConstraints::default(),
                f64::NAN,
                Err(ParameterConstraintError::NonFinite),
            ),
            (
                ParameterConstraints::default(),
                f64::INFINITY,
                Err(ParameterConstraintError::NonFinite),
            ),
            (inclusive_range, 1.0, Ok(())),
            (inclusive_range, 2.0, Ok(())),
            (
                exclusive_range,
                1.0,
                Err(ParameterConstraintError::OutOfRange),
            ),
            (exclusive_range, 1.5, Ok(())),
            (
                exclusive_range,
                2.0,
                Err(ParameterConstraintError::OutOfRange),
            ),
            (non_zero, 0.0, Err(ParameterConstraintError::ZeroNotAllowed)),
            (non_zero, -1.0, Ok(())),
            (non_zero, 1.0, Ok(())),
        ];

        for (constraints, value, expected) in cases {
            assert_eq!(constraints.validate(value), expected);
        }
    }

    #[test]
    fn validate_applies_reciprocal_constraints() {
        let bounded = ParameterConstraints::new(
            None,
            None,
            false,
            Some((Some(bound(0.4, true)), Some(bound(0.6, true)))),
        );

        assert_eq!(bounded.validate(2.0), Ok(()));
        assert_eq!(
            bounded.validate(0.5),
            Err(ParameterConstraintError::ReciprocalOutOfRange)
        );

        let finite_reciprocal =
            ParameterConstraints::new(None, None, true, Some((None, Some(bound(f64::MAX, true)))));

        assert_eq!(
            finite_reciprocal.validate(f64::from_bits(1)),
            Err(ParameterConstraintError::ReciprocalOutOfRange)
        );
    }

    #[test]
    fn constraint_subset_matches_supported_implications() {
        let unrestricted = ParameterConstraints::default();

        let positive = ParameterConstraints::new(Some(bound(0.0, false)), None, false, None);

        let non_negative = ParameterConstraints::new(Some(bound(0.0, true)), None, false, None);

        let non_zero = ParameterConstraints::new(None, None, true, None);

        let reciprocal_non_zero = ParameterConstraints::new(None, None, false, Some((None, None)));

        let reciprocal_required =
            ParameterConstraints::new(None, None, false, Some((Some(bound(0.1, true)), None)));

        let reciprocal_equal =
            ParameterConstraints::new(None, None, false, Some((Some(bound(0.1, true)), None)));

        let reciprocal_stricter =
            ParameterConstraints::new(None, None, false, Some((Some(bound(0.2, true)), None)));

        let cases = [
            ("equal constraints", positive, positive, true),
            (
                "stricter lower bound",
                ParameterConstraints::new(Some(bound(1.0, true)), None, false, None),
                positive,
                true,
            ),
            ("missing lower bound", unrestricted, positive, false),
            (
                "inclusive does not imply exclusive",
                non_negative,
                positive,
                false,
            ),
            ("exclusive implies inclusive", positive, non_negative, true),
            (
                "stricter upper bound",
                ParameterConstraints::new(None, Some(bound(9.0, true)), false, None),
                ParameterConstraints::new(None, Some(bound(10.0, true)), false, None),
                true,
            ),
            (
                "missing upper bound",
                unrestricted,
                ParameterConstraints::new(None, Some(bound(10.0, true)), false, None),
                false,
            ),
            ("explicit non-zero", non_zero, non_zero, true),
            ("positive implies non-zero", positive, non_zero, true),
            (
                "reciprocal constraint implies non-zero",
                reciprocal_non_zero,
                non_zero,
                true,
            ),
            (
                "equal reciprocal range",
                reciprocal_equal,
                reciprocal_required,
                true,
            ),
            (
                "stricter reciprocal range",
                reciprocal_stricter,
                reciprocal_required,
                true,
            ),
            (
                "missing reciprocal constraint",
                ParameterConstraints::new(Some(bound(1.0, true)), None, false, None),
                reciprocal_required,
                false,
            ),
        ];

        for (name, provided, required, expected) in cases {
            assert_eq!(provided.is_subset_of(&required), expected, "{name}");
        }
    }
}
