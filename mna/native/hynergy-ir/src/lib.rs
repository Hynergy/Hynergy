mod matrix;
mod rhs;
mod value;

pub use matrix::{MatrixAdd, MatrixProgram};
pub use rhs::{RhsAdd, RhsProgram};

pub use value::{
    EvaluationRate, InputSlot, ValueBuildError, ValueProgram, ValueProgramBuilder, ValueSlot,
    ValueWorkspace,
};
