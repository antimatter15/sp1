use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::AbstractField;

use super::add_rc::AddRcOperation;
use super::columns::{
    Poseidon2ExternalCols, NUM_POSEIDON2_EXTERNAL_COLS, POSEIDON2_DEFAULT_FIRST_EXTERNAL_ROUNDS,
    POSEIDON2_ROUND_CONSTANTS,
};
use super::external_linear_permute::ExternalLinearPermuteOperation;
use super::sbox::SBoxOperation;
use super::Poseidon2ExternalChip;
use crate::air::{CurtaAirBuilder, WORD_SIZE};

use core::borrow::Borrow;
use p3_matrix::MatrixRowSlices;

impl<F, const N: usize> BaseAir<F> for Poseidon2ExternalChip<N> {
    fn width(&self) -> usize {
        NUM_POSEIDON2_EXTERNAL_COLS
    }
}

impl<AB, const NUM_WORDS_STATE: usize> Air<AB> for Poseidon2ExternalChip<NUM_WORDS_STATE>
where
    AB: CurtaAirBuilder,
{
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local: &Poseidon2ExternalCols<AB::Var> = main.row_slice(0).borrow();
        let next: &Poseidon2ExternalCols<AB::Var> = main.row_slice(1).borrow();

        self.constrain_control_flow_flags(builder, local, next);

        self.constrain_memory(builder, local);

        self.constraint_external_ops(builder, local);
    }
}

impl<const NUM_WORDS_STATE: usize> Poseidon2ExternalChip<NUM_WORDS_STATE> {
    fn constrain_control_flow_flags<AB: CurtaAirBuilder>(
        &self,
        builder: &mut AB,
        local: &Poseidon2ExternalCols<AB::Var>,
        next: &Poseidon2ExternalCols<AB::Var>,
    ) {
        // If this is the i-th round, then the next row should be the (i+1)-th round.
        for i in 0..(POSEIDON2_DEFAULT_FIRST_EXTERNAL_ROUNDS - 1) {
            builder
                .when_transition()
                .when(next.0.is_real)
                .assert_eq(local.0.is_round_n[i], next.0.is_round_n[i + 1]);
            builder.assert_bool(local.0.is_round_n[i]);
        }

        // Calculate the current round number.
        {
            let round = {
                let mut acc: AB::Expr = AB::F::zero().into();

                for i in 0..POSEIDON2_DEFAULT_FIRST_EXTERNAL_ROUNDS {
                    acc += local.0.is_round_n[i] * AB::F::from_canonical_usize(i);
                }
                acc
            };
            builder.assert_eq(round, local.0.round_number);
        }

        // Calculate the round constants for this round.
        {
            for i in 0..NUM_WORDS_STATE {
                let round_constant = {
                    let mut acc: AB::Expr = AB::F::zero().into();

                    for j in 0..POSEIDON2_DEFAULT_FIRST_EXTERNAL_ROUNDS {
                        acc += local.0.is_round_n[j].into()
                            * AB::F::from_canonical_u32(POSEIDON2_ROUND_CONSTANTS[j][i]);
                    }
                    acc
                };
                builder.assert_eq(round_constant, local.0.round_constant[i]);
            }
        }
    }

    fn constrain_memory<AB: CurtaAirBuilder>(
        &self,
        builder: &mut AB,
        local: &Poseidon2ExternalCols<AB::Var>,
    ) {
        for round in 0..NUM_WORDS_STATE {
            builder.constraint_memory_access(
                local.0.segment,
                local.0.mem_read_clk[round],
                local.0.mem_addr[round],
                &local.0.mem_reads[round],
                local.0.is_external,
            );
            builder.constraint_memory_access(
                local.0.segment,
                local.0.mem_write_clk[round],
                local.0.mem_addr[round],
                &local.0.mem_writes[round],
                local.0.is_external,
            );
        }
    }

    fn constraint_external_ops<AB: CurtaAirBuilder>(
        &self,
        builder: &mut AB,
        local: &Poseidon2ExternalCols<AB::Var>,
    ) {
        // Convert each Word into one field element. The MemoryRead struct returns an array of Words
        // , but we need to perform operations within the field.
        let input_state = local.0.mem_reads.map(|read| {
            let mut acc: AB::Expr = AB::F::zero().into();
            for i in 0..WORD_SIZE {
                let shift: AB::Expr = AB::F::from_canonical_usize(1 << (8 * i)).into();
                acc += read.access.value[i].into() * shift;
            }
            acc
        });
        AddRcOperation::<AB::F>::eval(
            builder,
            input_state,
            local.0.is_round_n,
            local.0.round_constant,
            local.0.add_rc,
            local.0.is_external,
        );

        SBoxOperation::<AB::F>::eval(
            builder,
            local.0.add_rc.result,
            local.0.sbox,
            local.0.is_external,
        );

        ExternalLinearPermuteOperation::<AB::F>::eval(
            builder,
            local.0.sbox.acc.map(|x| *x.last().unwrap()),
            local.0.external_linear_permute,
            local.0.is_external,
        );
    }
}
