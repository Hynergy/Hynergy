#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverIterationProfile {
    stability_changes: usize,
    matrix_source_changes: usize,
    max_solution_delta: f64,
}

impl SolverIterationProfile {
    #[inline]
    pub const fn stability_changes(self) -> usize {
        self.stability_changes
    }

    #[inline]
    pub const fn matrix_source_changes(self) -> usize {
        self.matrix_source_changes
    }

    #[inline]
    pub const fn max_solution_delta(self) -> f64 {
        self.max_solution_delta
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SolverDiscreteProfile {
    plan_present: bool,
    qualified_drivers: usize,
    matrix_barriers: usize,
    rhs_barriers: usize,
    closure_attempts: usize,
    closure_rounds: usize,
    driver_scans: usize,
    output_updates: usize,
    zero_update_attempts: usize,
    barrier_exits: usize,
    verification_solves: usize,
    budget_fallbacks: usize,
    invalid_fallbacks: usize,
}

impl SolverDiscreteProfile {
    #[inline]
    pub const fn has_plan(self) -> bool {
        self.plan_present
    }

    #[inline]
    pub const fn qualified_drivers(self) -> usize {
        self.qualified_drivers
    }

    #[inline]
    pub const fn matrix_barriers(self) -> usize {
        self.matrix_barriers
    }

    #[inline]
    pub const fn rhs_barriers(self) -> usize {
        self.rhs_barriers
    }

    #[inline]
    pub const fn closure_attempts(self) -> usize {
        self.closure_attempts
    }

    #[inline]
    pub const fn closure_rounds(self) -> usize {
        self.closure_rounds
    }

    #[inline]
    pub const fn driver_scans(self) -> usize {
        self.driver_scans
    }

    #[inline]
    pub const fn output_updates(self) -> usize {
        self.output_updates
    }

    #[inline]
    pub const fn zero_update_attempts(self) -> usize {
        self.zero_update_attempts
    }

    #[inline]
    pub const fn barrier_exits(self) -> usize {
        self.barrier_exits
    }

    #[inline]
    pub const fn verification_solves(self) -> usize {
        self.verification_solves
    }

    #[inline]
    pub const fn budget_fallbacks(self) -> usize {
        self.budget_fallbacks
    }

    #[inline]
    pub const fn invalid_fallbacks(self) -> usize {
        self.invalid_fallbacks
    }

    #[inline]
    pub const fn fallbacks(self) -> usize {
        self.budget_fallbacks + self.invalid_fallbacks
    }

    #[inline]
    pub(crate) fn reset(
        &mut self,
        qualified_drivers: usize,
        matrix_barriers: usize,
        rhs_barriers: usize,
    ) {
        *self = Self {
            plan_present: qualified_drivers != 0,
            qualified_drivers,
            matrix_barriers,
            rhs_barriers,
            ..Self::default()
        };
    }

    #[inline]
    pub(crate) fn record_closure(
        &mut self,
        rounds: usize,
        driver_scans: usize,
        output_updates: usize,
    ) {
        self.closure_attempts += 1;
        self.closure_rounds += rounds;
        self.driver_scans += driver_scans;
        self.output_updates += output_updates;

        if output_updates == 0 {
            self.zero_update_attempts += 1;
        }
    }

    #[inline]
    pub(crate) fn record_barrier_exit(&mut self) {
        self.barrier_exits += 1;
    }

    #[inline]
    pub(crate) fn record_verification_solve(&mut self) {
        self.verification_solves += 1;
    }

    #[inline]
    pub(crate) fn record_budget_fallback(&mut self) {
        self.budget_fallbacks += 1;
    }

    #[inline]
    pub(crate) fn record_invalid_fallback(&mut self) {
        self.invalid_fallbacks += 1;
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SolverIslandProfile {
    island_index: usize,
    nonlinear: bool,
    slept: bool,
    mna_solves: usize,
    matrix_factorizations: usize,
    discrete: SolverDiscreteProfile,
    iterations: Vec<SolverIterationProfile>,
}

impl SolverIslandProfile {
    #[inline]
    pub const fn island_index(&self) -> usize {
        self.island_index
    }

    #[inline]
    pub const fn is_nonlinear(&self) -> bool {
        self.nonlinear
    }

    #[inline]
    pub const fn slept(&self) -> bool {
        self.slept
    }

    #[inline]
    pub const fn mna_solves(&self) -> usize {
        self.mna_solves
    }

    #[inline]
    pub const fn matrix_factorizations(&self) -> usize {
        self.matrix_factorizations
    }

    #[inline]
    pub const fn discrete(&self) -> SolverDiscreteProfile {
        self.discrete
    }

    #[inline]
    pub fn nonlinear_iterations(&self) -> usize {
        self.iterations.len()
    }

    #[inline]
    pub fn iterations(&self) -> &[SolverIterationProfile] {
        &self.iterations
    }

    #[inline]
    pub fn stability_changes(&self) -> usize {
        self.iterations
            .iter()
            .map(|iteration| iteration.stability_changes)
            .sum()
    }

    #[inline]
    pub fn matrix_source_changes(&self) -> usize {
        self.iterations
            .iter()
            .map(|iteration| iteration.matrix_source_changes)
            .sum()
    }

    #[inline]
    pub fn max_solution_delta(&self) -> f64 {
        self.iterations
            .iter()
            .map(|iteration| iteration.max_solution_delta)
            .fold(0.0, f64::max)
    }

    #[inline]
    pub(crate) fn begin_tick(
        &mut self,
        nonlinear: bool,
        qualified_discrete_drivers: usize,
        discrete_matrix_barriers: usize,
        discrete_rhs_barriers: usize,
    ) {
        self.island_index = 0;
        self.nonlinear = nonlinear;
        self.slept = false;
        self.mna_solves = 0;
        self.matrix_factorizations = 0;
        self.discrete.reset(
            qualified_discrete_drivers,
            discrete_matrix_barriers,
            discrete_rhs_barriers,
        );
        self.iterations.clear();
    }

    #[inline]
    pub(crate) fn discrete_mut(&mut self) -> &mut SolverDiscreteProfile {
        &mut self.discrete
    }

    #[inline]
    pub(crate) fn mark_slept(&mut self) {
        self.slept = true;
    }

    #[inline]
    pub(crate) fn record_mna_solve(&mut self) {
        self.mna_solves += 1;
    }

    #[inline]
    pub(crate) fn record_matrix_factorization(&mut self) {
        self.matrix_factorizations += 1;
    }

    #[inline]
    pub(crate) fn record_iteration(
        &mut self,
        stability_changes: usize,
        matrix_source_changes: usize,
        max_solution_delta: f64,
    ) {
        self.iterations.push(SolverIterationProfile {
            stability_changes,
            matrix_source_changes,
            max_solution_delta,
        });
    }

    #[inline]
    pub(crate) fn with_island_index(mut self, island_index: usize) -> Self {
        self.island_index = island_index;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SolverTickProfile {
    islands: Vec<SolverIslandProfile>,
}

impl SolverTickProfile {
    #[inline]
    pub fn islands(&self) -> &[SolverIslandProfile] {
        &self.islands
    }

    #[inline]
    pub fn total_mna_solves(&self) -> usize {
        self.islands
            .iter()
            .map(SolverIslandProfile::mna_solves)
            .sum()
    }

    #[inline]
    pub fn total_matrix_factorizations(&self) -> usize {
        self.islands
            .iter()
            .map(SolverIslandProfile::matrix_factorizations)
            .sum()
    }

    #[inline]
    pub fn total_nonlinear_iterations(&self) -> usize {
        self.islands
            .iter()
            .map(SolverIslandProfile::nonlinear_iterations)
            .sum()
    }

    #[inline]
    pub fn total_stability_changes(&self) -> usize {
        self.islands
            .iter()
            .map(SolverIslandProfile::stability_changes)
            .sum()
    }

    #[inline]
    pub fn total_matrix_source_changes(&self) -> usize {
        self.islands
            .iter()
            .map(SolverIslandProfile::matrix_source_changes)
            .sum()
    }

    #[inline]
    pub fn planned_discrete_islands(&self) -> usize {
        self.islands
            .iter()
            .filter(|island| island.discrete.has_plan())
            .count()
    }

    #[inline]
    pub fn total_qualified_discrete_drivers(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.qualified_drivers())
            .sum()
    }

    #[inline]
    pub fn total_discrete_closure_attempts(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.closure_attempts())
            .sum()
    }

    #[inline]
    pub fn total_discrete_closure_rounds(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.closure_rounds())
            .sum()
    }

    #[inline]
    pub fn total_discrete_driver_scans(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.driver_scans())
            .sum()
    }

    #[inline]
    pub fn total_discrete_output_updates(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.output_updates())
            .sum()
    }

    #[inline]
    pub fn total_discrete_zero_update_attempts(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.zero_update_attempts())
            .sum()
    }

    #[inline]
    pub fn total_discrete_barrier_exits(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.barrier_exits())
            .sum()
    }

    #[inline]
    pub fn total_discrete_verification_solves(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.verification_solves())
            .sum()
    }

    #[inline]
    pub fn total_discrete_fallbacks(&self) -> usize {
        self.islands
            .iter()
            .map(|island| island.discrete.fallbacks())
            .sum()
    }

    #[inline]
    pub fn max_solution_delta(&self) -> f64 {
        self.islands
            .iter()
            .map(SolverIslandProfile::max_solution_delta)
            .fold(0.0, f64::max)
    }

    #[inline]
    pub(crate) fn from_islands(islands: Vec<SolverIslandProfile>) -> Self {
        Self { islands }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discrete_profile_accumulates_and_resets_tick_work() {
        let mut profile = SolverIslandProfile::default();

        profile.begin_tick(true, 3, 1, 2);

        let discrete = profile.discrete_mut();

        discrete.record_closure(4, 12, 2);
        discrete.record_closure(1, 0, 0);
        discrete.record_barrier_exit();
        discrete.record_verification_solve();
        discrete.record_budget_fallback();
        discrete.record_invalid_fallback();

        let discrete = profile.discrete();

        assert!(discrete.has_plan());
        assert_eq!(discrete.qualified_drivers(), 3);
        assert_eq!(discrete.matrix_barriers(), 1);
        assert_eq!(discrete.rhs_barriers(), 2);
        assert_eq!(discrete.closure_attempts(), 2);
        assert_eq!(discrete.closure_rounds(), 5);
        assert_eq!(discrete.driver_scans(), 12);
        assert_eq!(discrete.output_updates(), 2);
        assert_eq!(discrete.zero_update_attempts(), 1);
        assert_eq!(discrete.barrier_exits(), 1);
        assert_eq!(discrete.verification_solves(), 1);
        assert_eq!(discrete.budget_fallbacks(), 1);
        assert_eq!(discrete.invalid_fallbacks(), 1);
        assert_eq!(discrete.fallbacks(), 2);

        profile.begin_tick(true, 0, 0, 0);

        assert_eq!(profile.discrete(), SolverDiscreteProfile::default());
    }

    #[test]
    fn tick_profile_aggregates_discrete_work() {
        let mut first = SolverIslandProfile::default();
        first.begin_tick(true, 4, 1, 0);
        first.discrete_mut().record_closure(3, 12, 4);
        first.discrete_mut().record_verification_solve();

        let mut second = SolverIslandProfile::default();
        second.begin_tick(true, 2, 0, 1);
        second.discrete_mut().record_closure(1, 2, 0);
        second.discrete_mut().record_barrier_exit();
        second.discrete_mut().record_verification_solve();
        second.discrete_mut().record_invalid_fallback();

        let tick = SolverTickProfile::from_islands(vec![first, second]);

        assert_eq!(tick.planned_discrete_islands(), 2);
        assert_eq!(tick.total_qualified_discrete_drivers(), 6);
        assert_eq!(tick.total_discrete_closure_attempts(), 2);
        assert_eq!(tick.total_discrete_closure_rounds(), 4);
        assert_eq!(tick.total_discrete_driver_scans(), 14);
        assert_eq!(tick.total_discrete_output_updates(), 4);
        assert_eq!(tick.total_discrete_zero_update_attempts(), 1);
        assert_eq!(tick.total_discrete_barrier_exits(), 1);
        assert_eq!(tick.total_discrete_verification_solves(), 2);
        assert_eq!(tick.total_discrete_fallbacks(), 1);
    }
}
