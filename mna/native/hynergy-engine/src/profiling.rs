#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverIterationProfile {
    stability_changes: usize,
    max_solution_delta: f64,
}

impl SolverIterationProfile {
    #[inline]
    pub const fn stability_changes(self) -> usize {
        self.stability_changes
    }

    #[inline]
    pub const fn max_solution_delta(self) -> f64 {
        self.max_solution_delta
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolverIslandProfile {
    island_index: usize,
    nonlinear: bool,
    slept: bool,
    mna_solves: usize,
    matrix_factorizations: usize,
    iterations: Vec<SolverIterationProfile>,
}

impl Default for SolverIslandProfile {
    fn default() -> Self {
        Self {
            island_index: 0,
            nonlinear: false,
            slept: false,
            mna_solves: 0,
            matrix_factorizations: 0,
            iterations: Vec::new(),
        }
    }
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
    pub fn max_solution_delta(&self) -> f64 {
        self.iterations
            .iter()
            .map(|iteration| iteration.max_solution_delta)
            .fold(0.0, f64::max)
    }

    #[inline]
    pub(crate) fn begin_tick(&mut self, nonlinear: bool) {
        self.island_index = 0;
        self.nonlinear = nonlinear;
        self.slept = false;
        self.mna_solves = 0;
        self.matrix_factorizations = 0;
        self.iterations.clear();
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
        max_solution_delta: f64,
    ) {
        self.iterations.push(SolverIterationProfile {
            stability_changes,
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
        self.islands.iter().map(SolverIslandProfile::mna_solves).sum()
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
