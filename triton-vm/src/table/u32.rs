use std::cmp::max;
use std::ops::Mul;

use air::challenge_id::ChallengeId::*;
use air::cross_table_argument::CrossTableArg;
use air::cross_table_argument::LookupArg;
use air::table::u32::U32Table;
use air::table_column::MasterBaseTableColumn;
use air::table_column::MasterExtTableColumn;
use air::table_column::U32BaseTableColumn;
use air::table_column::U32BaseTableColumn::*;
use air::table_column::U32ExtTableColumn;
use air::table_column::U32ExtTableColumn::*;
use arbitrary::Arbitrary;
use constraint_circuit::ConstraintCircuitBuilder;
use constraint_circuit::ConstraintCircuitMonad;
use constraint_circuit::DualRowIndicator;
use constraint_circuit::DualRowIndicator::*;
use constraint_circuit::InputIndicator;
use constraint_circuit::SingleRowIndicator;
use constraint_circuit::SingleRowIndicator::*;
use isa::instruction::Instruction;
use ndarray::parallel::prelude::*;
use ndarray::s;
use ndarray::Array1;
use ndarray::Array2;
use ndarray::ArrayView2;
use ndarray::ArrayViewMut2;
use ndarray::Axis;
use num_traits::One;
use num_traits::Zero;
use strum::EnumCount;
use twenty_first::prelude::*;

use crate::aet::AlgebraicExecutionTrace;
use crate::challenges::Challenges;
use crate::profiler::profiler;
use crate::table::TraceTable;

/// An executed u32 instruction as well as its operands.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Arbitrary)]
pub struct U32TableEntry {
    pub instruction: Instruction,
    pub left_operand: BFieldElement,
    pub right_operand: BFieldElement,
}

impl U32TableEntry {
    pub fn new<L, R>(instruction: Instruction, left_operand: L, right_operand: R) -> Self
    where
        L: Into<BFieldElement>,
        R: Into<BFieldElement>,
    {
        Self {
            instruction,
            left_operand: left_operand.into(),
            right_operand: right_operand.into(),
        }
    }

    /// The number of rows this entry contributes to the U32 Table.
    pub(crate) fn table_height_contribution(&self) -> u32 {
        let lhs = self.left_operand.value();
        let rhs = self.right_operand.value();
        let dominant_operand = match self.instruction {
            Instruction::Pow => rhs, // left-hand side doesn't change between rows
            _ => max(lhs, rhs),
        };
        match dominant_operand {
            0 => 2 - 1,
            _ => 2 + dominant_operand.ilog2(),
        }
    }
}

impl TraceTable for U32Table {
    type FillParam = ();
    type FillReturnInfo = ();

    fn fill(mut u32_table: ArrayViewMut2<BFieldElement>, aet: &AlgebraicExecutionTrace, _: ()) {
        let mut next_section_start = 0;
        for (&u32_table_entry, &multiplicity) in &aet.u32_entries {
            let mut first_row = Array2::zeros([1, Self::MainColumn::COUNT]);
            first_row[[0, CopyFlag.base_table_index()]] = bfe!(1);
            first_row[[0, Bits.base_table_index()]] = bfe!(0);
            first_row[[0, BitsMinus33Inv.base_table_index()]] = bfe!(-33).inverse();
            first_row[[0, CI.base_table_index()]] = u32_table_entry.instruction.opcode_b();
            first_row[[0, LHS.base_table_index()]] = u32_table_entry.left_operand;
            first_row[[0, RHS.base_table_index()]] = u32_table_entry.right_operand;
            first_row[[0, LookupMultiplicity.base_table_index()]] = multiplicity.into();
            let u32_section = u32_section_next_row(first_row);

            let next_section_end = next_section_start + u32_section.nrows();
            u32_table
                .slice_mut(s![next_section_start..next_section_end, ..])
                .assign(&u32_section);
            next_section_start = next_section_end;
        }
    }

    fn pad(mut main_table: ArrayViewMut2<BFieldElement>, table_len: usize) {
        let mut padding_row = Array1::zeros([Self::MainColumn::COUNT]);
        padding_row[[CI.base_table_index()]] = Instruction::Split.opcode_b();
        padding_row[[BitsMinus33Inv.base_table_index()]] = bfe!(-33).inverse();

        if table_len > 0 {
            let last_row = main_table.row(table_len - 1);
            padding_row[[CI.base_table_index()]] = last_row[CI.base_table_index()];
            padding_row[[LHS.base_table_index()]] = last_row[LHS.base_table_index()];
            padding_row[[LhsInv.base_table_index()]] = last_row[LhsInv.base_table_index()];
            padding_row[[Result.base_table_index()]] = last_row[Result.base_table_index()];

            // In the edge case that the last non-padding row comes from executing instruction
            // `lt` on operands 0 and 0, the `Result` column is 0. For the padding section,
            // where the `CopyFlag` is always 0, the `Result` needs to be set to 2 instead.
            if padding_row[[CI.base_table_index()]] == Instruction::Lt.opcode_b() {
                padding_row[[Result.base_table_index()]] = bfe!(2);
            }
        }

        main_table
            .slice_mut(s![table_len.., ..])
            .axis_iter_mut(Axis(0))
            .into_par_iter()
            .for_each(|mut row| row.assign(&padding_row));
    }

    fn extend(
        base_table: ArrayView2<BFieldElement>,
        mut ext_table: ArrayViewMut2<XFieldElement>,
        challenges: &Challenges,
    ) {
        profiler!(start "u32 table");
        assert_eq!(Self::MainColumn::COUNT, base_table.ncols());
        assert_eq!(Self::AuxColumn::COUNT, ext_table.ncols());
        assert_eq!(base_table.nrows(), ext_table.nrows());

        let ci_weight = challenges[U32CiWeight];
        let lhs_weight = challenges[U32LhsWeight];
        let rhs_weight = challenges[U32RhsWeight];
        let result_weight = challenges[U32ResultWeight];
        let lookup_indeterminate = challenges[U32Indeterminate];

        let mut running_sum_log_derivative = LookupArg::default_initial();
        for row_idx in 0..base_table.nrows() {
            let current_row = base_table.row(row_idx);
            if current_row[CopyFlag.base_table_index()].is_one() {
                let lookup_multiplicity = current_row[LookupMultiplicity.base_table_index()];
                let compressed_row = ci_weight * current_row[CI.base_table_index()]
                    + lhs_weight * current_row[LHS.base_table_index()]
                    + rhs_weight * current_row[RHS.base_table_index()]
                    + result_weight * current_row[Result.base_table_index()];
                running_sum_log_derivative +=
                    lookup_multiplicity * (lookup_indeterminate - compressed_row).inverse();
            }

            let mut extension_row = ext_table.row_mut(row_idx);
            extension_row[LookupServerLogDerivative.ext_table_index()] = running_sum_log_derivative;
        }
        profiler!(stop "u32 table");
    }
}

fn u32_section_next_row(mut section: Array2<BFieldElement>) -> Array2<BFieldElement> {
    let row_idx = section.nrows() - 1;
    let current_instruction: Instruction = section[[row_idx, CI.base_table_index()]]
        .value()
        .try_into()
        .expect("Unknown instruction");

    // Is the last row in this section reached?
    if (section[[row_idx, LHS.base_table_index()]].is_zero()
        || current_instruction == Instruction::Pow)
        && section[[row_idx, RHS.base_table_index()]].is_zero()
    {
        section[[row_idx, Result.base_table_index()]] = match current_instruction {
            Instruction::Split => bfe!(0),
            Instruction::Lt => bfe!(2),
            Instruction::And => bfe!(0),
            Instruction::Log2Floor => bfe!(-1),
            Instruction::Pow => bfe!(1),
            Instruction::PopCount => bfe!(0),
            _ => panic!("Must be u32 instruction, not {current_instruction}."),
        };

        // If instruction `lt` is executed on operands 0 and 0, the result is known to be 0.
        // The edge case can be reliably detected by checking whether column `Bits` is 0.
        let both_operands_are_0 = section[[row_idx, Bits.base_table_index()]].is_zero();
        if current_instruction == Instruction::Lt && both_operands_are_0 {
            section[[row_idx, Result.base_table_index()]] = bfe!(0);
        }

        // The right hand side is guaranteed to be 0. However, if the current instruction is
        // `pow`, then the left hand side might be non-zero.
        let lhs_inv_or_0 = section[[row_idx, LHS.base_table_index()]].inverse_or_zero();
        section[[row_idx, LhsInv.base_table_index()]] = lhs_inv_or_0;

        return section;
    }

    let lhs_lsb = bfe!(section[[row_idx, LHS.base_table_index()]].value() % 2);
    let rhs_lsb = bfe!(section[[row_idx, RHS.base_table_index()]].value() % 2);
    let mut next_row = section.row(row_idx).to_owned();
    next_row[CopyFlag.base_table_index()] = bfe!(0);
    next_row[Bits.base_table_index()] += bfe!(1);
    next_row[BitsMinus33Inv.base_table_index()] =
        (next_row[Bits.base_table_index()] - bfe!(33)).inverse();
    next_row[LHS.base_table_index()] = match current_instruction == Instruction::Pow {
        true => section[[row_idx, LHS.base_table_index()]],
        false => (section[[row_idx, LHS.base_table_index()]] - lhs_lsb) / bfe!(2),
    };
    next_row[RHS.base_table_index()] =
        (section[[row_idx, RHS.base_table_index()]] - rhs_lsb) / bfe!(2);
    next_row[LookupMultiplicity.base_table_index()] = bfe!(0);

    section.push_row(next_row.view()).unwrap();
    section = u32_section_next_row(section);
    let (mut row, next_row) = section.multi_slice_mut((s![row_idx, ..], s![row_idx + 1, ..]));

    row[LhsInv.base_table_index()] = row[LHS.base_table_index()].inverse_or_zero();
    row[RhsInv.base_table_index()] = row[RHS.base_table_index()].inverse_or_zero();

    let next_row_result = next_row[Result.base_table_index()];
    row[Result.base_table_index()] = match current_instruction {
        Instruction::Split => next_row_result,
        Instruction::Lt => {
            match (
                next_row_result.value(),
                lhs_lsb.value(),
                rhs_lsb.value(),
                row[CopyFlag.base_table_index()].value(),
            ) {
                (0 | 1, _, _, _) => next_row_result, // result already known
                (2, 0, 1, _) => bfe!(1),             // LHS < RHS
                (2, 1, 0, _) => bfe!(0),             // LHS > RHS
                (2, _, _, 1) => bfe!(0),             // LHS == RHS
                (2, _, _, 0) => bfe!(2),             // result still unknown
                _ => panic!("Invalid state"),
            }
        }
        Instruction::And => bfe!(2) * next_row_result + lhs_lsb * rhs_lsb,
        Instruction::Log2Floor => {
            if row[LHS.base_table_index()].is_zero() {
                bfe!(-1)
            } else if !next_row[LHS.base_table_index()].is_zero() {
                next_row_result
            } else {
                // LHS != 0 && LHS' == 0
                row[Bits.base_table_index()]
            }
        }
        Instruction::Pow => match rhs_lsb.is_zero() {
            true => next_row_result * next_row_result,
            false => next_row_result * next_row_result * row[LHS.base_table_index()],
        },
        Instruction::PopCount => next_row_result + lhs_lsb,
        _ => panic!("Must be u32 instruction, not {current_instruction}."),
    };

    section
}
