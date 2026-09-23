use crate::compile::definition::DefinitionStateId;
use crate::compile::discrete::CompiledDiscretePlan;
use crate::compile::island::{
    CompiledIsland, CompiledIslandParts, CompiledObserverOutput, DeviceObserver, DeviceState,
};
use crate::compile::island_ir::CompiledIslandIr;
#[cfg(feature = "solver-profiling")]
use crate::profiling::SolverIslandProfile;
use hynergy_ir::ValueWorkspace;
use hynergy_mna::system::{MnaError, MnaSystem};
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::Network;
use hynergy_model::parameter::ParameterId;

use thiserror::Error;

use crate::runtime::bindings::IslandBindings;
use crate::runtime::discrete::{ClosureOutcome, DiscreteScratch, run_discrete_closure_evaluated};
use crate::state::{PhysicalStateError, PhysicalStateStore};

#[cfg(test)]
use crate::state::PhysicalStateAddress;
#[cfg(test)]
use {
    crate::compile::island::{IslandNode, IslandUnknownLayout},
    std::cell::Cell,
};

const NONLINEAR_ABSOLUTE_TOLERANCE: f64 = 1.0e-9;
const NONLINEAR_RELATIVE_TOLERANCE: f64 = 1.0e-6;
const NONLINEAR_MAX_ITERATIONS: usize = 128;

#[derive(Debug)]
struct FastDiscreteScratch {
    closure: DiscreteScratch,
    rhs_barrier_reference: Box<[f64]>,
    rhs_reference_valid: bool,
}

impl FastDiscreteScratch {
    #[inline]
    fn new(plan: &CompiledDiscretePlan) -> Self {
        Self {
            closure: DiscreteScratch::new(plan),
            rhs_barrier_reference: vec![0.0; plan.rhs_barriers().len()].into_boxed_slice(),
            rhs_reference_valid: plan.rhs_barriers().is_empty(),
        }
    }

    #[inline]
    fn capture_rhs_reference(&mut self, plan: &CompiledDiscretePlan, workspace: &ValueWorkspace) {
        debug_assert_eq!(self.rhs_barrier_reference.len(), plan.rhs_barriers().len(),);

        self.rhs_reference_valid = false;

        for (target, &source) in self
            .rhs_barrier_reference
            .iter_mut()
            .zip(plan.rhs_barriers())
        {
            *target = workspace.value(source);
        }
    }

    #[inline]
    fn rhs_reference_matches(
        &self,
        plan: &CompiledDiscretePlan,
        workspace: &ValueWorkspace,
    ) -> bool {
        debug_assert_eq!(self.rhs_barrier_reference.len(), plan.rhs_barriers().len(),);

        self.rhs_reference_valid
            && self
                .rhs_barrier_reference
                .iter()
                .copied()
                .zip(plan.rhs_barriers().iter().copied())
                .all(|(expected, source)| expected == workspace.value(source))
    }
}

#[derive(Debug)]
struct NonlinearScratch {
    current: Box<[f64]>,
    next: Box<[f64]>,
    stability: Box<[f64]>,
    discrete: Option<Box<FastDiscreteScratch>>,
}

impl NonlinearScratch {
    fn new(
        dimension: usize,
        stability_count: usize,
        discrete_plan: Option<&CompiledDiscretePlan>,
    ) -> Self {
        Self {
            current: vec![0.0; dimension].into_boxed_slice(),
            next: vec![0.0; dimension].into_boxed_slice(),
            stability: vec![0.0; stability_count].into_boxed_slice(),
            discrete: discrete_plan.map(|plan| Box::new(FastDiscreteScratch::new(plan))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FastSolveResult {
    Solved,
    Fallback { iterations_used: usize },
}

#[derive(Debug, Error, PartialEq)]
pub(crate) enum IslandRuntimeError {
    #[error("device {device:?} no longer exists")]
    MissingDevice { device: DeviceId },

    #[error("device {device:?} parameter {parameter:?} is not assigned")]
    MissingParameter {
        device: DeviceId,
        parameter: ParameterId,
    },

    #[error(transparent)]
    Mna(#[from] MnaError),

    #[error("state {state:?} for device {device:?} is not available")]
    MissingState {
        device: DeviceId,
        state: DefinitionStateId,
    },

    #[error("nonlinear island did not converge after {iterations} iterations")]
    NonlinearDidNotConverge { iterations: usize },

    #[error("island matrix contains a non-finite value")]
    NonFiniteMatrix,

    #[error("island solution contains a non-finite value")]
    NonFiniteSolution,

    #[error("next-state value for {state:?} is non-finite")]
    NonFiniteState { state: DeviceState },
}

#[derive(Debug)]
pub(crate) struct IslandRuntime {
    system: MnaSystem,
    ir: CompiledIslandIr,
    discrete_plan: Option<Box<CompiledDiscretePlan>>,
    bindings: IslandBindings,
    workspace: ValueWorkspace,
    solution: Box<[f64]>,
    solution_valid: bool,
    static_initialized: bool,
    matrix_dirty: bool,
    factorized_iteration_matrix_sources: Box<[f64]>,
    static_inputs_dirty: bool,
    observer_outputs: Box<[CompiledObserverOutput]>,
    observer_outputs_dirty: bool,
    sleepable: bool,
    needs_solve: bool,
    nonlinear_scratch: Option<Box<NonlinearScratch>>,

    #[cfg(feature = "solver-profiling")]
    solver_tick_profile: SolverIslandProfile,

    #[cfg(test)]
    observer_read_count: Cell<usize>,
    #[cfg(test)]
    matrix_stamp_count: usize,
    #[cfg(test)]
    solve_count: usize,
    #[cfg(test)]
    physical_parameter_read_count: usize,
    #[cfg(test)]
    binding_rebind_count: usize,
    #[cfg(test)]
    unknowns: IslandUnknownLayout,
}

impl IslandRuntime {
    pub(crate) fn new(
        compiled: CompiledIsland,
        network: &Network,
        timestep: f64,
    ) -> Result<Self, IslandRuntimeError> {
        debug_assert!(timestep.is_finite());
        debug_assert!(timestep > 0.0);

        let CompiledIslandParts {
            pattern,
            ir,
            discrete_plan,
            #[cfg(test)]
            unknowns,
            states,
            partition_inputs,
            observer_outputs,
        } = compiled.into_parts();

        let bindings = IslandBindings::new(
            network,
            &states,
            &partition_inputs,
            ir.state_inputs(),
            ir.state_transition().writes(),
        )?;

        let dimension = pattern.dimension();

        let mut workspace = ir.value_program().new_workspace();

        if let Some(input) = ir.timestep_input() {
            workspace.set_input(input, timestep);
        }

        let system = MnaSystem::new(pattern)?;
        let factorized_iteration_matrix_sources =
            vec![0.0; ir.iteration_matrix_sources().len()].into_boxed_slice();

        debug_assert!(
            discrete_plan.is_none() || ir.requires_nonlinear_iteration(),
            "discrete plan requires nonlinear island iteration",
        );

        let nonlinear_scratch = ir.requires_nonlinear_iteration().then(|| {
            Box::new(NonlinearScratch::new(
                dimension,
                ir.iteration_stability_values().len(),
                discrete_plan.as_deref(),
            ))
        });

        let sleepable = ir.timestep_input().is_none()
            && ir.state_inputs().is_empty()
            && ir.state_transition().is_empty();

        Ok(Self {
            system,
            ir,
            discrete_plan,
            bindings,
            observer_outputs,
            observer_outputs_dirty: false,
            workspace,
            solution: vec![0.0; dimension].into_boxed_slice(),
            solution_valid: false,
            static_initialized: false,
            matrix_dirty: true,
            factorized_iteration_matrix_sources,
            static_inputs_dirty: true,
            sleepable,
            needs_solve: true,
            nonlinear_scratch,

            #[cfg(feature = "solver-profiling")]
            solver_tick_profile: SolverIslandProfile::default(),

            #[cfg(test)]
            observer_read_count: Cell::new(0),
            #[cfg(test)]
            matrix_stamp_count: 0,
            #[cfg(test)]
            solve_count: 0,
            #[cfg(test)]
            physical_parameter_read_count: 0,
            #[cfg(test)]
            binding_rebind_count: 0,
            #[cfg(test)]
            unknowns,
        })
    }

    #[inline]
    pub(crate) fn validate_state_outputs(
        &self,
        physical_state: &PhysicalStateStore,
    ) -> Result<(), IslandRuntimeError> {
        for binding in self.bindings.state_outputs() {
            binding.validate_source(&self.workspace)?;

            physical_state
                .validate_address(binding.state(), binding.address())
                .map_err(|error| match error {
                    PhysicalStateError::MissingInitialParameter { device, parameter } => {
                        IslandRuntimeError::MissingParameter { device, parameter }
                    }

                    PhysicalStateError::StateNotInitialized { state } => {
                        IslandRuntimeError::MissingState {
                            device: state.device(),
                            state: state.state(),
                        }
                    }
                })?;
        }

        Ok(())
    }

    #[inline]
    pub(crate) fn scatter_state_outputs(&self, physical_state: &mut PhysicalStateStore) {
        for binding in self.bindings.state_outputs() {
            physical_state.write_prevalidated(binding.address(), binding.value(&self.workspace));
        }
    }

    #[inline]
    pub(crate) fn finalize_state_outputs(&self, physical_state: &mut PhysicalStateStore) {
        for binding in self.bindings.state_outputs() {
            physical_state.mark_initialized_prevalidated(binding.address().location());
        }
    }

    pub(crate) fn observer_value(&self, observer: DeviceObserver) -> Option<f64> {
        #[cfg(test)]
        self.observer_read_count
            .set(self.observer_read_count.get() + 1);

        if !self.solution_valid {
            return None;
        }

        let index = self
            .observer_outputs
            .binary_search_by_key(&observer, |output| output.observer())
            .ok()?;

        Some(self.workspace.value(self.observer_outputs[index].value()))
    }

    fn factorize_matrix_if_dirty(&mut self) -> Result<(), IslandRuntimeError> {
        if self.matrix_dirty {
            {
                let mut matrix = self.system.values_mut();

                matrix.clear();

                self.ir
                    .matrix_program()
                    .execute(&mut matrix, self.workspace.values());
            }

            if !all_finite(self.system.values()) {
                return Err(IslandRuntimeError::NonFiniteMatrix);
            }

            self.system.factorize()?;

            capture_iteration_matrix_sources(
                &self.ir,
                &self.workspace,
                &mut self.factorized_iteration_matrix_sources,
            );

            self.matrix_dirty = false;

            #[cfg(feature = "solver-profiling")]
            self.solver_tick_profile.record_matrix_factorization();

            #[cfg(test)]
            {
                self.matrix_stamp_count += 1;
            }
        }

        debug_assert!(
            self.system.is_factorized(),
            "clean island matrix must remain factorized",
        );

        Ok(())
    }

    pub(crate) fn prepare_tick_state_inputs(
        &mut self,
        network: &Network,
        physical_state: &PhysicalStateStore,
    ) -> Result<(), IslandRuntimeError> {
        for binding in self.bindings.state_inputs() {
            let value = binding.read_logical(network, physical_state)?;

            self.workspace.set_input(binding.input(), value);
        }

        Ok(())
    }

    pub(crate) fn solve_prepared_tick(
        &mut self,
        network: &Network,
    ) -> Result<(), IslandRuntimeError> {
        #[cfg(feature = "solver-profiling")]
        {
            let nonlinear = self.nonlinear_scratch.is_some();

            let (qualified_drivers, matrix_barriers, rhs_barriers) =
                self.discrete_plan.as_deref().map_or((0, 0, 0), |plan| {
                    (
                        plan.drivers().len(),
                        plan.matrix_barriers().len(),
                        plan.rhs_barriers().len(),
                    )
                });

            self.solver_tick_profile.begin_tick(
                nonlinear,
                qualified_drivers,
                matrix_barriers,
                rhs_barriers,
            );
        }

        if self.sleepable && !self.needs_solve {
            debug_assert!(
                self.solution_valid,
                "sleeping island must retain a valid solution",
            );

            #[cfg(feature = "solver-profiling")]
            self.solver_tick_profile.mark_slept();

            return Ok(());
        }

        let had_authoritative_solution = self.solution_valid;

        self.solution_valid = false;

        self.prepare_static(network)?;

        self.ir.value_program().execute_tick(&mut self.workspace);

        initialize_iteration_latches(&self.ir, &mut self.workspace);

        if self.nonlinear_scratch.is_none() {
            self.solve_linear()?;
        } else {
            let mut scratch = self
                .nonlinear_scratch
                .take()
                .expect("nonlinear scratch was checked above");

            let result = self.solve_nonlinear(&mut scratch, had_authoritative_solution);

            self.nonlinear_scratch = Some(scratch);

            result?;
        }

        self.observer_outputs_dirty = true;

        if self.sleepable {
            self.needs_solve = false;
        }

        Ok(())
    }

    fn solve_linear(&mut self) -> Result<(), IslandRuntimeError> {
        self.factorize_matrix_if_dirty()?;

        self.ir
            .rhs_program()
            .execute(&mut self.solution, self.workspace.values());

        self.system.solve_in_place(&mut self.solution)?;

        #[cfg(feature = "solver-profiling")]
        self.solver_tick_profile.record_mna_solve();

        #[cfg(test)]
        {
            self.solve_count += 1;
        }

        if !all_finite(&self.solution) {
            return Err(IslandRuntimeError::NonFiniteSolution);
        }

        evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

        self.solution_valid = true;

        Ok(())
    }

    fn solve_nonlinear_candidate(
        &mut self,
        candidate: &mut [f64],
        mut discrete: Option<&mut FastDiscreteScratch>,
    ) -> Result<(), IslandRuntimeError> {
        self.factorize_matrix_if_dirty()?;

        if let (Some(plan), Some(discrete)) =
            (self.discrete_plan.as_deref(), discrete.as_deref_mut())
        {
            discrete.capture_rhs_reference(plan, &self.workspace);
        }

        self.ir
            .rhs_program()
            .execute(candidate, self.workspace.values());

        self.system.solve_in_place(candidate)?;

        #[cfg(feature = "solver-profiling")]
        self.solver_tick_profile.record_mna_solve();

        #[cfg(test)]
        {
            self.solve_count += 1;
        }

        if !all_finite(candidate) {
            return Err(IslandRuntimeError::NonFiniteSolution);
        }

        if let Some(discrete) = discrete {
            discrete.rhs_reference_valid = true;
        }

        Ok(())
    }

    #[inline]
    fn update_iteration_matrix_dirty(&mut self) -> usize {
        if self.ir.iteration_matrix_sources().is_empty() {
            return 0;
        }

        #[cfg(not(feature = "solver-profiling"))]
        if self.matrix_dirty {
            return 0;
        }

        if !self.system.is_factorized() {
            debug_assert!(
                self.matrix_dirty,
                "an unfactorized matrix must already be marked dirty",
            );

            return 0;
        }

        let source_changes = iteration_matrix_source_change_count(
            &self.ir,
            &self.workspace,
            &self.factorized_iteration_matrix_sources,
        );

        self.matrix_dirty |= source_changes != 0;

        source_changes
    }

    fn solve_nonlinear(
        &mut self,
        scratch: &mut NonlinearScratch,
        had_authoritative_solution: bool,
    ) -> Result<(), IslandRuntimeError> {
        if self.discrete_plan.is_some() {
            return match self.solve_nonlinear_fast(scratch, had_authoritative_solution)? {
                FastSolveResult::Solved => Ok(()),
                FastSolveResult::Fallback { iterations_used } => {
                    self.solve_nonlinear_generic(scratch, iterations_used)
                }
            };
        }

        self.solve_nonlinear_generic(scratch, 0)
    }

    fn solve_nonlinear_fast(
        &mut self,
        scratch: &mut NonlinearScratch,
        had_authoritative_solution: bool,
    ) -> Result<FastSolveResult, IslandRuntimeError> {
        debug_assert!(self.discrete_plan.is_some());
        debug_assert!(
            self.ir.iteration_latches().is_empty(),
            "v1 discrete plan must exclude iteration latches",
        );

        let rhs_reference_valid = scratch
            .discrete
            .as_deref()
            .expect("discrete plan must allocate discrete scratch")
            .rhs_reference_valid;

        let bootstrap = !had_authoritative_solution
            || self.matrix_dirty
            || !self.system.is_factorized()
            || !rhs_reference_valid;

        let mut iterations_used = 0usize;

        if bootstrap {
            evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

            let converged = self.solve_discrete_verification(scratch)?;
            iterations_used = 1;

            if converged {
                return Ok(FastSolveResult::Solved);
            }
        }

        while iterations_used < NONLINEAR_MAX_ITERATIONS {
            evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

            let closure_needed = !self.iteration_stability_matches(&scratch.stability);

            let outcome = if closure_needed {
                scratch.current.copy_from_slice(&self.solution);

                let outcome = {
                    let plan = self
                        .discrete_plan
                        .as_deref()
                        .expect("fast nonlinear path requires a discrete plan");

                    let discrete = scratch
                        .discrete
                        .as_deref_mut()
                        .expect("discrete plan must allocate discrete scratch");

                    run_discrete_closure_evaluated(
                        plan,
                        &self.ir,
                        &mut self.workspace,
                        &mut scratch.current,
                        &self.factorized_iteration_matrix_sources,
                        &discrete.rhs_barrier_reference,
                        &mut discrete.closure,
                    )
                };

                #[cfg(feature = "solver-profiling")]
                {
                    let closure_profile = scratch
                        .discrete
                        .as_deref()
                        .expect("discrete plan must allocate discrete scratch")
                        .closure
                        .profile();

                    self.solver_tick_profile.discrete_mut().record_closure(
                        closure_profile.rounds(),
                        closure_profile.driver_scans(),
                        closure_profile.output_updates(),
                    );
                }

                outcome
            } else {
                ClosureOutcome::Settled
            };

            match outcome {
                ClosureOutcome::Settled => {}

                ClosureOutcome::Barrier => {
                    #[cfg(feature = "solver-profiling")]
                    self.solver_tick_profile
                        .discrete_mut()
                        .record_barrier_exit();
                }

                ClosureOutcome::BudgetExceeded => {
                    #[cfg(feature = "solver-profiling")]
                    self.solver_tick_profile
                        .discrete_mut()
                        .record_budget_fallback();

                    evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

                    return Ok(FastSolveResult::Fallback { iterations_used });
                }

                ClosureOutcome::InvalidPrediction => {
                    #[cfg(feature = "solver-profiling")]
                    self.solver_tick_profile
                        .discrete_mut()
                        .record_invalid_fallback();

                    evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

                    return Ok(FastSolveResult::Fallback { iterations_used });
                }
            }

            let converged = self.solve_discrete_verification(scratch)?;
            iterations_used += 1;

            if converged {
                return Ok(FastSolveResult::Solved);
            }
        }

        Err(IslandRuntimeError::NonlinearDidNotConverge {
            iterations: NONLINEAR_MAX_ITERATIONS,
        })
    }

    fn solve_discrete_verification(
        &mut self,
        scratch: &mut NonlinearScratch,
    ) -> Result<bool, IslandRuntimeError> {
        capture_iteration_stability(&self.ir, &self.workspace, &mut scratch.stability);

        let _matrix_source_changes = self.update_iteration_matrix_dirty();

        self.solve_nonlinear_candidate(&mut scratch.next, scratch.discrete.as_deref_mut())?;

        #[cfg(feature = "solver-profiling")]
        self.solver_tick_profile
            .discrete_mut()
            .record_verification_solve();

        evaluate_iteration(&self.ir, &mut self.workspace, &scratch.next);

        #[cfg(feature = "solver-profiling")]
        {
            let stability_changes = iteration_stability_change_count(
                &scratch.stability,
                self.ir
                    .iteration_stability_values()
                    .iter()
                    .map(|&slot| self.workspace.value(slot)),
            );
            let max_solution_delta = maximum_solution_delta(&self.solution, &scratch.next);

            self.solver_tick_profile.record_iteration(
                stability_changes,
                _matrix_source_changes,
                max_solution_delta,
            );
        }

        let stability_matches = self.iteration_stability_matches(&scratch.stability);

        // If the candidate reproduces the exact matrix, RHS, and stability state used to
        // produce it, another verification solve would solve the identical linear system.
        let exact_system_matches = self.discrete_linear_system_matches(scratch);

        let exact_fixed_point = stability_matches && exact_system_matches;

        let converged = exact_fixed_point
            || (stability_matches && solutions_converged(&self.solution, &scratch.next));

        std::mem::swap(&mut self.solution, &mut scratch.next);

        if converged {
            self.solution_valid = true;
        }

        Ok(converged)
    }

    fn solve_nonlinear_generic(
        &mut self,
        scratch: &mut NonlinearScratch,
        iterations_used: usize,
    ) -> Result<(), IslandRuntimeError> {
        if iterations_used >= NONLINEAR_MAX_ITERATIONS {
            return Err(IslandRuntimeError::NonlinearDidNotConverge {
                iterations: NONLINEAR_MAX_ITERATIONS,
            });
        }

        evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

        capture_iteration_stability(&self.ir, &self.workspace, &mut scratch.stability);
        advance_iteration_latches(&self.ir, &mut self.workspace);

        let _matrix_source_changes = self.update_iteration_matrix_dirty();

        self.solve_nonlinear_candidate(&mut scratch.current, scratch.discrete.as_deref_mut())?;

        evaluate_iteration(&self.ir, &mut self.workspace, &scratch.current);

        #[cfg(feature = "solver-profiling")]
        {
            let stability_changes = iteration_stability_change_count(
                &scratch.stability,
                self.ir
                    .iteration_stability_values()
                    .iter()
                    .map(|&slot| self.workspace.value(slot)),
            );
            let max_solution_delta = maximum_solution_delta(&self.solution, &scratch.current);
            self.solver_tick_profile.record_iteration(
                stability_changes,
                _matrix_source_changes,
                max_solution_delta,
            );
        }

        if solutions_converged(&self.solution, &scratch.current)
            && self.iteration_stability_matches(&scratch.stability)
        {
            std::mem::swap(&mut self.solution, &mut scratch.current);

            self.solution_valid = true;

            return Ok(());
        }

        for _ in (iterations_used + 1)..NONLINEAR_MAX_ITERATIONS {
            capture_iteration_stability(&self.ir, &self.workspace, &mut scratch.stability);
            advance_iteration_latches(&self.ir, &mut self.workspace);

            let _matrix_source_changes = self.update_iteration_matrix_dirty();

            self.solve_nonlinear_candidate(&mut scratch.next, scratch.discrete.as_deref_mut())?;

            evaluate_iteration(&self.ir, &mut self.workspace, &scratch.next);

            #[cfg(feature = "solver-profiling")]
            {
                let stability_changes = iteration_stability_change_count(
                    &scratch.stability,
                    self.ir
                        .iteration_stability_values()
                        .iter()
                        .map(|&slot| self.workspace.value(slot)),
                );
                let max_solution_delta = maximum_solution_delta(&scratch.current, &scratch.next);
                self.solver_tick_profile.record_iteration(
                    stability_changes,
                    _matrix_source_changes,
                    max_solution_delta,
                );
            }

            if solutions_converged(&scratch.current, &scratch.next)
                && self.iteration_stability_matches(&scratch.stability)
            {
                std::mem::swap(&mut self.solution, &mut scratch.next);

                self.solution_valid = true;

                return Ok(());
            }

            std::mem::swap(&mut scratch.current, &mut scratch.next);
        }

        Err(IslandRuntimeError::NonlinearDidNotConverge {
            iterations: NONLINEAR_MAX_ITERATIONS,
        })
    }

    #[cfg(test)]
    pub(crate) fn solve_tick(
        &mut self,
        network: &Network,
        physical_state: &PhysicalStateStore,
    ) -> Result<(), IslandRuntimeError> {
        self.prepare_tick_state_inputs(network, physical_state)?;
        self.solve_prepared_tick(network)
    }

    #[cfg(test)]
    fn solve_tick_with_state_reader<F>(
        &mut self,
        network: &Network,
        mut old_state: F,
    ) -> Result<(), IslandRuntimeError>
    where
        F: FnMut(PhysicalStateAddress) -> Option<f64>,
    {
        for binding in self.bindings.state_inputs() {
            let state = binding.state();

            let value = old_state(binding.address()).ok_or(IslandRuntimeError::MissingState {
                device: state.device(),
                state: state.state(),
            })?;

            self.workspace.set_input(binding.input(), value);
        }

        self.solve_prepared_tick(network)
    }

    fn load_parameters(&mut self, network: &Network) -> Result<StaticChanges, IslandRuntimeError> {
        let mut changes = StaticChanges::default();

        for binding in self.bindings.parameters() {
            #[cfg(test)]
            {
                self.physical_parameter_read_count += 1;
            }

            let device = binding.device();
            let parameter = binding.parameter();
            let input = binding.input();

            let value = network
                .parameter_at_location(binding.location(), parameter)
                .ok_or(IslandRuntimeError::MissingDevice { device })?
                .ok_or(IslandRuntimeError::MissingParameter { device, parameter })?;

            if self.workspace.value(input.value()) == value {
                continue;
            }

            self.workspace.set_input(input, value);

            changes.record(self.ir.static_input_affects_matrix(input));
        }

        Ok(changes)
    }

    fn prepare_static(&mut self, network: &Network) -> Result<(), IslandRuntimeError> {
        let mut changes = StaticChanges::default();

        if self.static_inputs_dirty {
            changes = self.load_parameters(network)?;
            self.static_inputs_dirty = false;
        }

        let initialize = !self.static_initialized;

        if initialize || changes.values {
            self.ir.value_program().execute_static(&mut self.workspace);
            self.static_initialized = true;
        }

        if initialize || changes.matrix {
            self.matrix_dirty = true;
        }

        Ok(())
    }

    #[inline]
    pub(crate) fn mark_numerical_dirty(&mut self) {
        self.static_inputs_dirty = true;
        self.needs_solve = true;
    }

    #[inline]
    fn iteration_stability_matches(&self, expected: &[f64]) -> bool {
        iteration_stability_matches(
            expected,
            self.ir
                .iteration_stability_values()
                .iter()
                .map(|&slot| self.workspace.value(slot)),
        )
    }

    #[inline]
    fn discrete_linear_system_matches(&self, scratch: &NonlinearScratch) -> bool {
        let plan = self
            .discrete_plan
            .as_deref()
            .expect("discrete system comparison requires a discrete plan");

        let discrete = scratch
            .discrete
            .as_deref()
            .expect("discrete plan must allocate discrete scratch");

        iteration_matrix_sources_match(
            &self.ir,
            &self.workspace,
            &self.factorized_iteration_matrix_sources,
        ) && discrete.rhs_reference_matches(plan, &self.workspace)
    }

    #[inline]
    pub(crate) fn rebind(&mut self, network: &Network) -> Result<(), IslandRuntimeError> {
        self.bindings.rebind(network)?;

        #[cfg(test)]
        {
            self.binding_rebind_count += 1;
        }

        Ok(())
    }

    #[cfg(debug_assertions)]
    #[inline]
    pub(crate) fn debug_assert_bindings_valid(
        &self,
        network: &Network,
        physical_state: &PhysicalStateStore,
    ) {
        self.bindings.debug_assert_valid(network, physical_state);
    }

    #[cfg(feature = "solver-profiling")]
    #[inline]
    pub(crate) fn solver_tick_profile(&self) -> &SolverIslandProfile {
        &self.solver_tick_profile
    }

    #[inline]
    pub(crate) const fn observer_outputs_dirty(&self) -> bool {
        self.observer_outputs_dirty
    }

    #[inline]
    pub(crate) fn mark_observer_outputs_clean(&mut self) {
        self.observer_outputs_dirty = false;
    }

    #[cfg(test)]
    pub(crate) fn set_state_output_address_for_test(
        &mut self,
        index: usize,
        address: PhysicalStateAddress,
    ) {
        self.bindings
            .set_state_output_address_for_test(index, address);
    }
}

#[cfg(test)]
impl IslandRuntime {
    #[inline]
    pub(crate) fn observer_read_count(&self) -> usize {
        self.observer_read_count.get()
    }

    #[inline]
    fn matrix_stamp_count(&self) -> usize {
        self.matrix_stamp_count
    }

    #[inline]
    fn solve_count(&self) -> usize {
        self.solve_count
    }

    #[inline]
    pub(crate) fn node_voltage(&self, node: IslandNode) -> Option<f64> {
        if !self.solution_valid {
            return None;
        }

        Some(match self.unknowns.node_unknown(node) {
            None => 0.0,

            Some(unknown) => self.solution[unknown.index()],
        })
    }

    #[inline]
    fn reset_parameter_read_count(&mut self) {
        self.physical_parameter_read_count = 0;
    }

    #[inline]
    fn physical_parameter_read_count(&self) -> usize {
        self.physical_parameter_read_count
    }

    #[inline]
    pub(crate) fn binding_rebind_count(&self) -> usize {
        self.binding_rebind_count
    }
}

#[inline]
fn initialize_iteration_latches(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace) {
    for &latch in ir.iteration_latches() {
        let initial = workspace.value(latch.initial());

        workspace.set_input(latch.input(), initial);
    }
}

#[inline]
fn advance_iteration_latches(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace) {
    for &latch in ir.iteration_latches() {
        let next = workspace.value(latch.update());

        workspace.set_input(latch.input(), next);
    }
}

#[inline]
fn evaluate_iteration(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace, solution: &[f64]) {
    for &(unknown, input) in ir.solution_inputs() {
        workspace.set_input(input, solution[unknown.index()]);
    }

    ir.value_program().execute_iteration(workspace);
}

#[inline]
fn capture_iteration_stability(
    ir: &CompiledIslandIr,
    workspace: &ValueWorkspace,
    target: &mut [f64],
) {
    debug_assert_eq!(target.len(), ir.iteration_stability_values().len(),);

    for (target, &slot) in target.iter_mut().zip(ir.iteration_stability_values()) {
        *target = workspace.value(slot);
    }
}

#[inline]
fn capture_iteration_matrix_sources(
    ir: &CompiledIslandIr,
    workspace: &ValueWorkspace,
    target: &mut [f64],
) {
    debug_assert_eq!(target.len(), ir.iteration_matrix_sources().len());

    for (target, &slot) in target.iter_mut().zip(ir.iteration_matrix_sources()) {
        *target = workspace.value(slot);
    }
}

#[inline]
fn iteration_matrix_sources_match(
    ir: &CompiledIslandIr,
    workspace: &ValueWorkspace,
    expected: &[f64],
) -> bool {
    debug_assert_eq!(expected.len(), ir.iteration_matrix_sources().len());

    expected
        .iter()
        .copied()
        .zip(ir.iteration_matrix_sources().iter().copied())
        .all(|(expected, source)| expected == workspace.value(source))
}

#[inline]
fn iteration_matrix_source_change_count(
    ir: &CompiledIslandIr,
    workspace: &ValueWorkspace,
    expected: &[f64],
) -> usize {
    debug_assert_eq!(expected.len(), ir.iteration_matrix_sources().len());

    let actual = ir
        .iteration_matrix_sources()
        .iter()
        .map(|&slot| workspace.value(slot));

    #[cfg(feature = "solver-profiling")]
    {
        expected
            .iter()
            .copied()
            .zip(actual)
            .filter(|(expected, actual)| expected != actual)
            .count()
    }

    #[cfg(not(feature = "solver-profiling"))]
    {
        if expected
            .iter()
            .copied()
            .zip(actual)
            .any(|(expected, actual)| expected != actual)
        {
            1
        } else {
            0
        }
    }
}

#[inline]
fn iteration_stability_matches(
    expected: &[f64],
    actual: impl ExactSizeIterator<Item = f64>,
) -> bool {
    debug_assert_eq!(expected.len(), actual.len());

    expected
        .iter()
        .copied()
        .zip(actual)
        .all(|(expected, actual)| expected == actual)
}

#[cfg(feature = "solver-profiling")]
#[inline]
fn iteration_stability_change_count(
    expected: &[f64],
    actual: impl ExactSizeIterator<Item = f64>,
) -> usize {
    debug_assert_eq!(expected.len(), actual.len());

    expected
        .iter()
        .copied()
        .zip(actual)
        .filter(|(expected, actual)| expected != actual)
        .count()
}

#[cfg(feature = "solver-profiling")]
#[inline]
fn maximum_solution_delta(previous: &[f64], current: &[f64]) -> f64 {
    debug_assert_eq!(previous.len(), current.len());

    previous
        .iter()
        .zip(current)
        .map(|(&previous, &current)| (current - previous).abs())
        .fold(0.0, f64::max)
}

#[inline]
fn solutions_converged(previous: &[f64], current: &[f64]) -> bool {
    debug_assert_eq!(previous.len(), current.len());
    debug_assert!(all_finite(previous));
    debug_assert!(all_finite(current));

    previous.iter().zip(current).all(|(&previous, &current)| {
        let delta = (current - previous).abs();

        let limit = NONLINEAR_ABSOLUTE_TOLERANCE
            + NONLINEAR_RELATIVE_TOLERANCE * current.abs().max(previous.abs());

        delta <= limit
    })
}

#[inline]
fn all_finite(values: &[f64]) -> bool {
    values.iter().all(|value| value.is_finite())
}

#[derive(Debug, Default, Clone, Copy)]
struct StaticChanges {
    values: bool,
    matrix: bool,
}

impl StaticChanges {
    #[inline]
    fn record(&mut self, affects_matrix: bool) {
        self.values = true;
        self.matrix |= affects_matrix;
    }
}

#[cfg(test)]
mod test {
    use crate::compile::definition::{CompiledDefinition, DefinitionStateId};
    use crate::compile::island::{
        DeviceObserver, DeviceState, IslandNode, IslandPartitionSpec, compile_island_parts,
        compile_topology_island,
    };
    use crate::compile::island_ir::IslandIrBuilder;
    use crate::runtime::island::{
        IslandRuntime, advance_iteration_latches, initialize_iteration_latches,
        iteration_stability_matches, solutions_converged,
    };
    use crate::state::{PhysicalStateAddress, PhysicalStateStore};
    use crate::topology::{DerivedTopology, DeviceComponent};
    use hynergy_ir::StateSlot;
    use hynergy_mna::pattern::PatternBuilder;
    use hynergy_model::circuit::{Element, ValueRef};
    use hynergy_model::device::builder::DeviceDefinitionBuilder;
    use hynergy_model::device::definition::{
        DefinitionId, DefinitionObserverId, DeviceId, DevicePartitionId, PrimitiveElementKind,
        TerminalId,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::{Network, WireId};
    use hynergy_model::parameter::ParameterId;

    const DEFAULT_TIMESTEP: f64 = 1.0;

    #[test]
    fn runtime_retains_compiled_discrete_plan_and_scratch() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let device = DeviceId::try_from(1).unwrap();

        network
            .add_device(
                &definitions,
                device,
                DefinitionId::from(PrimitiveElementKind::Not),
            )
            .unwrap();

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 2.5)
            .unwrap();
        network
            .set_device_parameter(&definitions, device, ParameterId::new(1), 1.0)
            .unwrap();
        network
            .set_device_parameter(&definitions, device, ParameterId::new(2), 0.0)
            .unwrap();

        let definition = definitions
            .get(DefinitionId::from(PrimitiveElementKind::Not))
            .unwrap();

        let compiled_definition = CompiledDefinition::compile(&definitions, definition).unwrap();

        let partition = compiled_definition
            .partition(DevicePartitionId::new(0))
            .unwrap();

        let output = IslandNode::terminal(device, TerminalId::new(0));
        let vdd = IslandNode::terminal(device, TerminalId::new(1));
        let vss = IslandNode::terminal(device, TerminalId::new(2));
        let input = IslandNode::terminal(device, TerminalId::new(3));

        let terminal_nodes = [output, vdd, vss, input];

        let parts = [IslandPartitionSpec::new(
            device,
            partition,
            compiled_definition.state_initializers(),
            &terminal_nodes,
        )];

        let compiled = compile_island_parts(&[vss, output, vdd, input], &parts).unwrap();

        assert!(compiled.discrete_plan().is_some());

        let runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        assert!(runtime.discrete_plan.is_some());
        assert!(
            runtime
                .nonlinear_scratch
                .as_ref()
                .unwrap()
                .discrete
                .is_some()
        );
    }

    fn voltage_source_island() -> (Network, crate::compile::island::CompiledIsland) {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();

        network.add_wire(negative).unwrap();
        network.add_wire(positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        network
            .attach_terminal(positive, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(negative, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(source, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        (network, compiled)
    }

    fn add_device_with_physical_state(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        physical_state: &mut PhysicalStateStore,
        device: DeviceId,
        definition_id: DefinitionId,
    ) {
        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(definitions, device, definition_id)
            .unwrap();

        let state_insert = physical_state.prepare_add_device(definition, &model_insert);
        let insert = network.commit_add_device(model_insert);

        physical_state.commit_add_device(state_insert, insert);
    }

    fn capacitor_runtime_with_physical_state() -> (
        Network,
        PhysicalStateStore,
        IslandRuntime,
        PhysicalStateAddress,
    ) {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let wire_a = WireId::try_from(1).unwrap();
        let wire_b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();
        let capacitor = DeviceId::try_from(2).unwrap();
        let source = DeviceId::try_from(3).unwrap();

        network.add_wire(wire_a).unwrap();
        network.add_wire(wire_b).unwrap();

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut physical_state,
            conductance,
            DefinitionId::from(PrimitiveElementKind::Conductance),
        );

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut physical_state,
            capacitor,
            DefinitionId::from(PrimitiveElementKind::Capacitor),
        );

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut physical_state,
            source,
            DefinitionId::from(PrimitiveElementKind::CurrentSource),
        );

        network
            .attach_terminal(wire_a, conductance, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, conductance, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_a, capacitor, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, capacitor, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_b, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_a, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(capacitor, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();
        let runtime = IslandRuntime::new(compiled, &network, 0.5).unwrap();
        let state = PhysicalStateAddress::new(network.device_location(capacitor).unwrap(), 0);

        (network, physical_state, runtime, state)
    }

    #[test]
    fn nonlinear_island_reuses_previous_solution_after_invalidation() {
        let (network, mut compiled) = voltage_source_island();

        compiled.force_nonlinear_iteration_for_test(false);

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.solve_count(), 2);

        runtime.mark_numerical_dirty();
        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            3,
            "woken nonlinear island should reuse its previous solution as its initial guess",
        );
    }

    #[cfg(feature = "solver-profiling")]
    #[test]
    fn nonlinear_solver_profile_records_iteration_work() {
        let (network, mut compiled) = voltage_source_island();

        compiled.force_nonlinear_iteration_for_test(false);

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let profile = runtime.solver_tick_profile();

        assert!(profile.is_nonlinear());
        assert!(!profile.slept());
        assert_eq!(profile.mna_solves(), 2);
        assert_eq!(profile.matrix_factorizations(), 1);
        assert_eq!(profile.matrix_source_changes(), 0);
        assert_eq!(profile.nonlinear_iterations(), 2);
        assert_eq!(profile.iterations().len(), 2);
        assert!(profile.iterations()[0].max_solution_delta() > 0.0);
    }

    #[test]
    fn clean_stateless_island_sleeps_after_first_solve() {
        let (network, compiled) = voltage_source_island();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.solve_count(), 1);

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            1,
            "clean stateless island should reuse its previous solution",
        );
    }

    #[test]
    fn numerical_invalidation_wakes_sleeping_island() {
        let (network, compiled) = voltage_source_island();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();
        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.solve_count(), 1);

        runtime.mark_numerical_dirty();
        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            2,
            "numerical invalidation must wake a sleeping island",
        );

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            2,
            "island should sleep again after the wake-up solve",
        );
    }

    #[test]
    fn sleeping_island_keeps_cached_observer_values_available() {
        let (network, compiled) = voltage_source_island();
        let source = DeviceId::try_from(1).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let observer = DeviceObserver::new(source, DefinitionObserverId::new(0));
        let first = runtime.observer_value(observer).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.solve_count(), 1);
        assert_eq!(runtime.observer_value(observer), Some(first));
        assert!((first - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn linear_island_solves_once() {
        let (network, compiled) = voltage_source_island();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        let physical_state = PhysicalStateStore::default();

        runtime.solve_tick(&network, &physical_state).unwrap();

        assert_eq!(runtime.solve_count(), 1,);
    }

    #[test]
    fn nonlinear_rhs_iteration_reuses_matrix_factorization() {
        let (network, mut compiled) = voltage_source_island();

        compiled.force_nonlinear_iteration_for_test(false);

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.solve_count(), 2);
        assert_eq!(runtime.matrix_stamp_count(), 1);
    }

    #[test]
    fn voltage_source_and_conductance_solve_node_voltage() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();
        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();
        network.add_wire(positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                DefinitionId::from(PrimitiveElementKind::VoltageSource),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        for device in [source, conductance] {
            network
                .attach_terminal(positive, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(negative, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(source, DevicePartitionId::new(0)),
        );

        let positive_node = IslandNode::net(topology.wire_net(positive));
        let negative_node = IslandNode::net(topology.wire_net(negative));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        let (): () = runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let voltage = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((voltage - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn stateful_island_does_not_sleep_between_ticks() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let a = WireId::try_from(1).unwrap();
        let b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();
        let capacitor = DeviceId::try_from(2).unwrap();

        network.add_wire(a).unwrap();
        network.add_wire(b).unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                capacitor,
                PrimitiveElementKind::Capacitor.into(),
            )
            .unwrap();

        for device in [conductance, capacitor] {
            network
                .attach_terminal(a, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(b, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(capacitor, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, 0.5).unwrap();

        let state = PhysicalStateAddress::new(network.device_location(capacitor).unwrap(), 0);

        runtime
            .solve_tick_with_state_reader(&network, |candidate| (candidate == state).then_some(0.0))
            .unwrap();

        assert_eq!(runtime.solve_count(), 1);

        runtime
            .solve_tick_with_state_reader(&network, |candidate| (candidate == state).then_some(0.0))
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            2,
            "state-dependent islands must remain awake",
        );
    }

    #[test]
    fn capacitor_tick_reads_old_state_and_retains_next_state() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let wire_a = WireId::try_from(1).unwrap();
        let wire_b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();
        let capacitor = DeviceId::try_from(2).unwrap();
        let source = DeviceId::try_from(3).unwrap();

        network.add_wire(wire_a).unwrap();
        network.add_wire(wire_b).unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                capacitor,
                DefinitionId::from(PrimitiveElementKind::Capacitor),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                DefinitionId::from(PrimitiveElementKind::CurrentSource),
            )
            .unwrap();

        network
            .attach_terminal(wire_a, conductance, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, conductance, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_a, capacitor, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, capacitor, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_b, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_a, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(capacitor, DevicePartitionId::new(0)),
        );

        let node_a = IslandNode::net(topology.wire_net(wire_a));
        let node_b = IslandNode::net(topology.wire_net(wire_b));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, 0.5).unwrap();

        let capacitor_state =
            PhysicalStateAddress::new(network.device_location(capacitor).unwrap(), 0);

        runtime
            .solve_tick_with_state_reader(&network, |state| {
                (state == capacitor_state).then_some(3.0)
            })
            .unwrap();

        let voltage = runtime.node_voltage(node_a).unwrap() - runtime.node_voltage(node_b).unwrap();

        assert!((voltage - 2.8).abs() < 1.0e-12);

        let outputs = runtime.bindings.state_outputs();

        assert_eq!(outputs.len(), 1);

        let output = &outputs[0];
        let semantic_state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        assert_eq!(output.state(), semantic_state,);

        assert!(
            (output.value(&runtime.workspace) - 2.8).abs() < 1.0e-12,
            "next state must remain retained in the runtime workspace",
        );
    }

    #[test]
    fn fixed_timestep_capacitor_reuses_matrix_factorization() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let a = WireId::try_from(1).unwrap();
        let b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();
        let capacitor = DeviceId::try_from(2).unwrap();

        network.add_wire(a).unwrap();
        network.add_wire(b).unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                capacitor,
                PrimitiveElementKind::Capacitor.into(),
            )
            .unwrap();

        for device in [conductance, capacitor] {
            network
                .attach_terminal(a, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(b, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(capacitor, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, 0.5).unwrap();

        let state = PhysicalStateAddress::new(network.device_location(capacitor).unwrap(), 0);

        runtime
            .solve_tick_with_state_reader(&network, |candidate| (candidate == state).then_some(0.0))
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        runtime
            .solve_tick_with_state_reader(&network, |candidate| (candidate == state).then_some(0.0))
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);
    }

    #[test]
    fn matrix_affecting_parameter_change_restamps_matrix() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();
        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();
        network.add_wire(positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        for device in [source, conductance] {
            network
                .attach_terminal(positive, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(negative, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(source, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        runtime.mark_numerical_dirty();
        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 2);
    }

    #[test]
    fn rhs_only_parameter_change_reuses_matrix_factorization() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();
        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();
        network.add_wire(positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        for device in [source, conductance] {
            network
                .attach_terminal(positive, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(negative, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(source, DevicePartitionId::new(0)),
        );

        let positive_node = IslandNode::net(topology.wire_net(positive));
        let negative_node = IslandNode::net(topology.wire_net(negative));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 9.0)
            .unwrap();

        runtime.mark_numerical_dirty();
        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        let voltage = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((voltage - 9.0).abs() < 1.0e-12);
    }

    #[test]
    fn controlled_conductance_converges_to_self_controlled_operating_point() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();

        let controlled = DeviceId::try_from(1).unwrap();
        let current_source = DeviceId::try_from(2).unwrap();

        network.add_wire(common).unwrap();
        network.add_wire(output).unwrap();

        network
            .add_device(
                &definitions,
                controlled,
                PrimitiveElementKind::VoltageControlledConductance.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                current_source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();

        network
            .attach_terminal(output, controlled, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(common, controlled, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(output, controlled, TerminalId::new(2))
            .unwrap();

        network
            .attach_terminal(common, current_source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(output, current_source, TerminalId::new(1))
            .unwrap();

        for (index, value) in [2.0, 2.0, 1.0, 5.0].into_iter().enumerate() {
            network
                .set_device_parameter(
                    &definitions,
                    controlled,
                    ParameterId::new(index as u32),
                    value,
                )
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, current_source, ParameterId::new(0), 6.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(controlled, DevicePartitionId::new(0)),
        );

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 2.0).abs() < 1.0e-6);
        assert!(runtime.solve_count() > 1);
    }

    #[test]
    fn diode_converges_between_forward_and_reverse_conductance() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();

        let diode = DeviceId::try_from(1).unwrap();
        let current_source = DeviceId::try_from(2).unwrap();

        network.add_wire(common).unwrap();
        network.add_wire(output).unwrap();

        network
            .add_device(&definitions, diode, PrimitiveElementKind::Diode.into())
            .unwrap();

        network
            .add_device(
                &definitions,
                current_source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();
        network
            .attach_terminal(output, diode, TerminalId::new(0))
            .unwrap();
        network
            .attach_terminal(common, diode, TerminalId::new(1))
            .unwrap();
        network
            .attach_terminal(common, current_source, TerminalId::new(0))
            .unwrap();
        network
            .attach_terminal(output, current_source, TerminalId::new(1))
            .unwrap();
        network
            .set_device_parameter(&definitions, diode, ParameterId::new(0), 4.0)
            .unwrap();
        network
            .set_device_parameter(&definitions, diode, ParameterId::new(1), 0.25)
            .unwrap();
        network
            .set_device_parameter(&definitions, current_source, ParameterId::new(0), 8.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);
        let island = topology.component_island(
            &network,
            DeviceComponent::new(diode, DevicePartitionId::new(0)),
        );

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();
        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 2.0).abs() < 1.0e-9);
        assert_eq!(runtime.solve_count(), 3);
        assert_eq!(
            runtime.matrix_stamp_count(),
            2,
            "diode mode change should refactor once, then reuse the settled matrix",
        );

        #[cfg(feature = "solver-profiling")]
        {
            let profile = runtime.solver_tick_profile();

            assert_eq!(profile.matrix_source_changes(), 1);
            assert_eq!(
                profile
                    .iterations()
                    .iter()
                    .map(|iteration| { iteration.matrix_source_changes() })
                    .collect::<Vec<_>>(),
                vec![0, 1, 0],
            );
        }
    }

    fn logic_gate_island(
        kind: PrimitiveElementKind,
        input_a_voltage: f64,
        input_b_voltage: Option<f64>,
    ) -> (
        Network,
        crate::compile::island::CompiledIsland,
        IslandNode,
        IslandNode,
    ) {
        let binary = match kind {
            PrimitiveElementKind::Not => false,
            PrimitiveElementKind::And
            | PrimitiveElementKind::Nand
            | PrimitiveElementKind::Or
            | PrimitiveElementKind::Nor => true,
            _ => panic!("logic_gate_island requires a logic gate primitive"),
        };

        assert_eq!(binary, input_b_voltage.is_some());

        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();
        let vdd = WireId::try_from(3).unwrap();
        let input_a = WireId::try_from(4).unwrap();
        let input_b = binary.then(|| WireId::try_from(5).unwrap());

        for wire in [
            Some(common),
            Some(output),
            Some(vdd),
            Some(input_a),
            input_b,
        ]
        .into_iter()
        .flatten()
        {
            network.add_wire(wire).unwrap();
        }

        let gate = DeviceId::try_from(1).unwrap();
        let vdd_source = DeviceId::try_from(2).unwrap();
        let input_a_source = DeviceId::try_from(3).unwrap();
        let input_b_source = binary.then(|| DeviceId::try_from(4).unwrap());

        network.add_device(&definitions, gate, kind.into()).unwrap();

        for source in [Some(vdd_source), Some(input_a_source), input_b_source]
            .into_iter()
            .flatten()
        {
            network
                .add_device(
                    &definitions,
                    source,
                    PrimitiveElementKind::VoltageSource.into(),
                )
                .unwrap();
        }

        for (terminal, wire) in [(0, output), (1, vdd), (2, common), (3, input_a)] {
            network
                .attach_terminal(wire, gate, TerminalId::new(terminal))
                .unwrap();
        }

        if let Some(input_b) = input_b {
            network
                .attach_terminal(input_b, gate, TerminalId::new(4))
                .unwrap();
        }

        for (source, positive) in [(vdd_source, vdd), (input_a_source, input_a)] {
            network
                .attach_terminal(positive, source, TerminalId::new(0))
                .unwrap();
            network
                .attach_terminal(common, source, TerminalId::new(1))
                .unwrap();
        }

        if let (Some(source), Some(input_b)) = (input_b_source, input_b) {
            network
                .attach_terminal(input_b, source, TerminalId::new(0))
                .unwrap();
            network
                .attach_terminal(common, source, TerminalId::new(1))
                .unwrap();
        }

        for (index, value) in [2.5, 10.0, 0.01].into_iter().enumerate() {
            network
                .set_device_parameter(&definitions, gate, ParameterId::new(index as u32), value)
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, vdd_source, ParameterId::new(0), 5.0)
            .unwrap();
        network
            .set_device_parameter(
                &definitions,
                input_a_source,
                ParameterId::new(0),
                input_a_voltage,
            )
            .unwrap();

        if let (Some(source), Some(voltage)) = (input_b_source, input_b_voltage) {
            network
                .set_device_parameter(&definitions, source, ParameterId::new(0), voltage)
                .unwrap();
        }

        let topology = DerivedTopology::from_network(&network, &definitions);
        let island = topology.component_island(
            &network,
            DeviceComponent::new(gate, DevicePartitionId::new(0)),
        );

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));
        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        (network, compiled, output_node, common_node)
    }

    fn fast_not_island(input_voltage: f64) -> (Network, crate::compile::island::CompiledIsland) {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let gate = DeviceId::try_from(1).unwrap();
        let vdd_source = DeviceId::try_from(2).unwrap();
        let input_source = DeviceId::try_from(3).unwrap();

        network
            .add_device(&definitions, gate, PrimitiveElementKind::Not.into())
            .unwrap();

        for source in [vdd_source, input_source] {
            network
                .add_device(
                    &definitions,
                    source,
                    PrimitiveElementKind::VoltageSource.into(),
                )
                .unwrap();
        }

        for (index, value) in [2.5, 10.0, 0.01].into_iter().enumerate() {
            network
                .set_device_parameter(&definitions, gate, ParameterId::new(index as u32), value)
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, vdd_source, ParameterId::new(0), 5.0)
            .unwrap();
        network
            .set_device_parameter(
                &definitions,
                input_source,
                ParameterId::new(0),
                input_voltage,
            )
            .unwrap();

        let gate_definition = CompiledDefinition::compile(
            &definitions,
            definitions
                .get(DefinitionId::from(PrimitiveElementKind::Not))
                .unwrap(),
        )
        .unwrap();

        let source_definition = CompiledDefinition::compile(
            &definitions,
            definitions
                .get(DefinitionId::from(PrimitiveElementKind::VoltageSource))
                .unwrap(),
        )
        .unwrap();

        let gate_partition = gate_definition
            .partition(DevicePartitionId::new(0))
            .unwrap();
        let source_partition = source_definition
            .partition(DevicePartitionId::new(0))
            .unwrap();

        let output = IslandNode::terminal(gate, TerminalId::new(0));
        let vdd = IslandNode::terminal(gate, TerminalId::new(1));
        let vss = IslandNode::terminal(gate, TerminalId::new(2));
        let input = IslandNode::terminal(gate, TerminalId::new(3));

        let gate_nodes = [output, vdd, vss, input];
        let vdd_source_nodes = [vdd, vss];
        let input_source_nodes = [input, vss];

        let parts = [
            IslandPartitionSpec::new(
                gate,
                gate_partition,
                gate_definition.state_initializers(),
                &gate_nodes,
            ),
            IslandPartitionSpec::new(
                vdd_source,
                source_partition,
                source_definition.state_initializers(),
                &vdd_source_nodes,
            ),
            IslandPartitionSpec::new(
                input_source,
                source_partition,
                source_definition.state_initializers(),
                &input_source_nodes,
            ),
        ];

        let compiled = compile_island_parts(&[vss, output, vdd, input], &parts).unwrap();

        assert!(
            compiled.discrete_plan().is_some(),
            "H1 test fixture must compile a discrete fast-path plan",
        );

        (network, compiled)
    }

    fn solve_logic_gate(
        kind: PrimitiveElementKind,
        input_a_voltage: f64,
        input_b_voltage: Option<f64>,
    ) -> (f64, usize) {
        let (network, compiled, output_node, common_node) =
            logic_gate_island(kind, input_a_voltage, input_b_voltage);

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        (voltage, runtime.solve_count())
    }

    #[test]
    fn exact_first_verification_fixed_point_needs_no_confirmation_solve() {
        let (network, compiled) = fast_not_island(0.0);

        assert!(
            compiled.discrete_plan().is_some(),
            "H1 test fixture must compile a discrete fast-path plan",
        );

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            1,
            "an exact matrix/RHS/stability fixed point needs no confirmation solve",
        );
        assert_eq!(
            runtime.matrix_stamp_count(),
            1,
            "the accepted fixed point should use the factorization that produced it",
        );
    }

    #[test]
    fn changed_discrete_matrix_still_requires_corrective_verification_solve() {
        let (network, compiled) = fast_not_island(5.0);

        assert!(
            compiled.discrete_plan().is_some(),
            "H1 test fixture must compile a discrete fast-path plan",
        );

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            2,
            "a candidate that changes its discrete matrix must be solved again",
        );
        assert_eq!(
            runtime.matrix_stamp_count(),
            2,
            "the changed discrete matrix must be factorized before acceptance",
        );
    }

    #[test]
    fn logic_gates_follow_boolean_truth_tables() {
        let g_max = 10.0;
        let g_min = 0.01;
        let vdd = 5.0;

        let high_voltage = vdd * g_max / (g_max + g_min);
        let low_voltage = vdd * g_min / (g_max + g_min);

        for (input, expected_high) in [(0.0, true), (5.0, false)] {
            let (voltage, _) = solve_logic_gate(PrimitiveElementKind::Not, input, None);
            let expected = if expected_high {
                high_voltage
            } else {
                low_voltage
            };

            assert!((voltage - expected).abs() < 1.0e-9);
        }

        for (kind, truth_table) in [
            (PrimitiveElementKind::And, [false, false, false, true]),
            (PrimitiveElementKind::Nand, [true, true, true, false]),
            (PrimitiveElementKind::Or, [false, true, true, true]),
            (PrimitiveElementKind::Nor, [true, false, false, false]),
        ] {
            for (index, expected_high) in truth_table.into_iter().enumerate() {
                let input_a = if index & 0b10 == 0 { 0.0 } else { 5.0 };
                let input_b = if index & 0b01 == 0 { 0.0 } else { 5.0 };

                let (voltage, _) = solve_logic_gate(kind, input_a, Some(input_b));
                let expected = if expected_high {
                    high_voltage
                } else {
                    low_voltage
                };

                assert!(
                    (voltage - expected).abs() < 1.0e-9,
                    "{kind:?} with A={input_a}, B={input_b}: {voltage} != {expected}",
                );
            }
        }
    }

    #[test]
    fn logic_gate_threshold_is_inclusive() {
        let (voltage, _) = solve_logic_gate(PrimitiveElementKind::Not, 2.5, None);
        let expected_low = 5.0 * 0.01 / (10.0 + 0.01);

        assert!((voltage - expected_low).abs() < 1.0e-9);
    }

    #[test]
    fn settled_logic_gate_island_sleeps() {
        let (network, compiled, _, _) =
            logic_gate_island(PrimitiveElementKind::Nand, 5.0, Some(5.0));

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();
        let first_solve_count = runtime.solve_count();

        assert!(
            first_solve_count > 1,
            "initial nonlinear solve should iterate"
        );

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert_eq!(
            runtime.solve_count(),
            first_solve_count,
            "stateless settled logic should sleep until invalidated",
        );
    }

    #[test]
    fn iteration_stability_requires_exact_values() {
        assert!(solutions_converged(&[0.0], &[5.0e-10]));

        assert!(!iteration_stability_matches(&[0.0], [5.0e-10].into_iter(),));

        assert!(iteration_stability_matches(
            &[0.0, 1.0],
            [0.0, 1.0].into_iter(),
        ));
    }

    #[test]
    fn iteration_latch_advances_to_computed_value() {
        let pattern = PatternBuilder::new(1).unwrap().finish().unwrap();

        let mut builder = IslandIrBuilder::new(&pattern);

        let initial = builder.state_value(StateSlot::new(0)).unwrap();
        let latch = builder.iteration_latch(initial).unwrap();
        let one = builder.constant_value(1.0).unwrap();
        let update = builder.sub_value(one, latch).unwrap();

        builder.update_iteration_latch(latch, update);

        let ir = builder.finish().unwrap();
        let mut workspace = ir.value_program().new_workspace();

        let state_input = ir.state_inputs()[0].1;

        workspace.set_input(state_input, 0.0);

        ir.value_program().execute_tick(&mut workspace);

        initialize_iteration_latches(&ir, &mut workspace);

        ir.value_program().execute_iteration(&mut workspace);

        assert_eq!(workspace.value(update), 1.0);

        advance_iteration_latches(&ir, &mut workspace);

        ir.value_program().execute_iteration(&mut workspace);

        assert_eq!(workspace.value(update), 0.0);
    }

    fn schmitt_buffer_island() -> (
        DefinitionRegistry,
        Network,
        crate::compile::island::CompiledIsland,
        IslandNode,
        IslandNode,
        DeviceId,
        DeviceId,
    ) {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();
        let input = WireId::try_from(3).unwrap();
        let supply = WireId::try_from(4).unwrap();

        let buffer = DeviceId::try_from(1).unwrap();
        let input_source = DeviceId::try_from(2).unwrap();
        let supply_source = DeviceId::try_from(3).unwrap();

        for wire in [common, output, input, supply] {
            network.add_wire(wire).unwrap();
        }

        network
            .add_device(
                &definitions,
                buffer,
                PrimitiveElementKind::SchmittBuffer.into(),
            )
            .unwrap();

        for source in [input_source, supply_source] {
            network
                .add_device(
                    &definitions,
                    source,
                    PrimitiveElementKind::VoltageSource.into(),
                )
                .unwrap();
        }

        for (wire, terminal) in [(output, 0), (supply, 1), (common, 2), (input, 3)] {
            network
                .attach_terminal(wire, buffer, TerminalId::new(terminal))
                .unwrap();
        }

        network
            .attach_terminal(input, input_source, TerminalId::new(0))
            .unwrap();
        network
            .attach_terminal(common, input_source, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(supply, supply_source, TerminalId::new(0))
            .unwrap();
        network
            .attach_terminal(common, supply_source, TerminalId::new(1))
            .unwrap();

        for (index, value) in [2.0, 2.0, 4.0, 1.0].into_iter().enumerate() {
            network
                .set_device_parameter(&definitions, buffer, ParameterId::new(index as u32), value)
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, input_source, ParameterId::new(0), 0.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, supply_source, ParameterId::new(0), 5.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(buffer, DevicePartitionId::new(0)),
        );

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        (
            definitions,
            network,
            compiled,
            output_node,
            common_node,
            buffer,
            input_source,
        )
    }

    #[test]
    fn schmitt_buffer_applies_hysteresis_across_ticks() {
        let (definitions, mut network, compiled, output_node, common_node, buffer, input_source) =
            schmitt_buffer_island();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        let state = PhysicalStateAddress::new(network.device_location(buffer).unwrap(), 0);
        let semantic_state = DeviceState::new(buffer, DefinitionStateId::new(0));

        let read_next_mode = |runtime: &IslandRuntime| {
            let outputs = runtime.bindings.state_outputs();

            assert_eq!(outputs.len(), 1);

            let output = &outputs[0];

            assert_eq!(output.state(), semantic_state,);

            output.value(&runtime.workspace)
        };

        let mut mode = 0.0;

        runtime
            .solve_tick_with_state_reader(&network, |candidate| {
                (candidate == state).then_some(mode)
            })
            .unwrap();

        let next_mode = read_next_mode(&runtime);

        assert_eq!(next_mode, 0.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 1.0).abs() < 1.0e-9);

        mode = next_mode;

        network
            .set_device_parameter(&definitions, input_source, ParameterId::new(0), 4.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        runtime
            .solve_tick_with_state_reader(&network, |candidate| {
                (candidate == state).then_some(mode)
            })
            .unwrap();

        let next_mode = read_next_mode(&runtime);

        assert_eq!(next_mode, 1.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 4.0).abs() < 1.0e-9);

        mode = next_mode;

        network
            .set_device_parameter(&definitions, input_source, ParameterId::new(0), 2.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        runtime
            .solve_tick_with_state_reader(&network, |candidate| {
                (candidate == state).then_some(mode)
            })
            .unwrap();

        let next_mode = read_next_mode(&runtime);

        assert_eq!(next_mode, 1.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 4.0).abs() < 1.0e-9);

        mode = next_mode;

        network
            .set_device_parameter(&definitions, input_source, ParameterId::new(0), 0.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        runtime
            .solve_tick_with_state_reader(&network, |candidate| {
                (candidate == state).then_some(mode)
            })
            .unwrap();

        let next_mode = read_next_mode(&runtime);

        assert_eq!(next_mode, 0.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn stateless_composite_solves_through_island_runtime() {
        let mut definitions = DefinitionRegistry::new();

        let resistance = DefinitionId::from(PrimitiveElementKind::Resistance);

        let resistance_constraint = definitions.get(resistance).unwrap().parameters()[0];

        let composite_definition = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();
            let middle = builder.add_node().unwrap();

            let r2 = builder.add_parameter(resistance_constraint).unwrap();

            builder
                .add_element(Element::new(
                    resistance,
                    vec![positive, middle],
                    vec![ValueRef::Literal(2.0)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    resistance,
                    vec![middle, negative],
                    vec![ValueRef::Parameter(r2)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let composite_definition = definitions.register(composite_definition).unwrap();

        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();

        let composite = DeviceId::try_from(1).unwrap();
        let source = DeviceId::try_from(2).unwrap();

        network.add_wire(common).unwrap();
        network.add_wire(output).unwrap();

        network
            .add_device(&definitions, composite, composite_definition)
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();

        network
            .attach_terminal(output, composite, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(common, composite, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(common, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(output, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, composite, ParameterId::new(0), 3.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(composite, DevicePartitionId::new(0)),
        );

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 10.0).abs() < 1.0e-12);
        assert_eq!(runtime.matrix_stamp_count(), 1);

        network
            .set_device_parameter(&definitions, composite, ParameterId::new(0), 8.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 20.0).abs() < 1.0e-12);
        assert_eq!(runtime.matrix_stamp_count(), 2);
    }

    #[test]
    fn composite_voltage_observer_is_available_after_solve() {
        let mut definitions = DefinitionRegistry::new();

        let observed_definition = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Conductance.into(),
                    vec![positive, negative],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            let observer = builder.add_voltage_observer(positive, negative).unwrap();

            assert_eq!(observer, DefinitionObserverId::new(0));

            builder.build_definition().unwrap()
        };

        let observed_definition = definitions.register(observed_definition).unwrap();

        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let observed = DeviceId::try_from(1).unwrap();
        let source = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();
        network.add_wire(positive).unwrap();

        network
            .add_device(&definitions, observed, observed_definition)
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        for device in [observed, source] {
            network
                .attach_terminal(positive, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(negative, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(observed, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, 1.0).unwrap();

        let observer = DeviceObserver::new(observed, DefinitionObserverId::new(0));

        assert_eq!(runtime.observer_value(observer), None);

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        let value = runtime.observer_value(observer).unwrap();

        assert!((value - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn numerical_dirty_parameter_refresh_uses_physical_bindings() {
        let (mut network, compiled) = voltage_source_island();

        let source = DeviceId::try_from(1).unwrap();

        let mut runtime = IslandRuntime::new(compiled, &network, DEFAULT_TIMESTEP).unwrap();

        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();
        runtime.reset_parameter_read_count();

        network
            .set_device_parameter(&DefinitionRegistry::new(), source, ParameterId::new(0), 9.0)
            .unwrap();

        runtime.mark_numerical_dirty();
        runtime
            .solve_tick_with_state_reader(&network, |_| None)
            .unwrap();

        assert!(
            runtime.physical_parameter_read_count() > 0,
            "numerical refresh must load parameters through physical bindings",
        );

        let observer = DeviceObserver::new(source, DefinitionObserverId::new(0));

        assert_eq!(runtime.observer_value(observer), Some(9.0),);
    }

    #[test]
    fn stateful_runtime_validates_bound_state_outputs() {
        let (network, physical_state, mut runtime, _state) =
            capacitor_runtime_with_physical_state();

        runtime.solve_tick(&network, &physical_state).unwrap();
        runtime.validate_state_outputs(&physical_state).unwrap();
    }

    #[test]
    fn scattering_state_outputs_does_not_finalize_state_rows() {
        let (network, mut physical_state, mut runtime, state) =
            capacitor_runtime_with_physical_state();

        runtime.solve_tick(&network, &physical_state).unwrap();
        runtime.validate_state_outputs(&physical_state).unwrap();

        assert!(!physical_state.is_initialized_at(state.location(),),);

        runtime.scatter_state_outputs(&mut physical_state);

        assert!(
            (physical_state.get_at(state).unwrap() - 0.4).abs() < 1.0e-12,
            "scatter must copy the retained next-state value into physical storage",
        );

        assert!(
            !physical_state.is_initialized_at(state.location(),),
            "scattering values must not finalize state rows",
        );
    }

    #[test]
    fn finalizing_state_outputs_marks_state_rows_initialized() {
        let (network, mut physical_state, mut runtime, state) =
            capacitor_runtime_with_physical_state();

        runtime.solve_tick(&network, &physical_state).unwrap();
        runtime.validate_state_outputs(&physical_state).unwrap();
        runtime.scatter_state_outputs(&mut physical_state);

        assert!(!physical_state.is_initialized_at(state.location(),),);

        runtime.finalize_state_outputs(&mut physical_state);

        assert!(
            physical_state.is_initialized_at(state.location(),),
            "finalization must make scattered state visible as committed state",
        );

        assert!(
            (physical_state.get_at(state).unwrap() - 0.4).abs() < 1.0e-12,
            "finalization must not modify the already-scattered scalar",
        );
    }
}
