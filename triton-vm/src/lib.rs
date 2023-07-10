//! Triton Virtual Machine is a Zero-Knowledge Proof System (ZKPS) for proving correct execution
//! of programs written in Triton assembly. The proof system is a zk-STARK, which is a
//! state-of-the-art ZKPS.

use anyhow::bail;
use anyhow::Result;
pub use twenty_first::shared_math::b_field_element::BFieldElement;
pub use twenty_first::shared_math::tip5::Digest;

use crate::program::Program;
pub use crate::proof::Claim;
pub use crate::proof::Proof;
use crate::stark::Stark;
use crate::stark::StarkHasher;
pub use crate::stark::StarkParameters;

pub mod aet;
pub mod arithmetic_domain;
pub mod error;
pub mod example_programs;
pub mod fri;
pub mod instruction;
pub mod op_stack;
pub mod parser;
pub mod profiler;
pub mod program;
pub mod proof;
pub mod proof_item;
pub mod proof_stream;
mod shared_tests;
pub mod stark;
pub mod table;
pub mod vm;

/// Prove correct execution of a program written in Triton assembly.
/// This is a convenience function, abstracting away the details of the STARK construction.
/// If you want to have more control over the STARK construction, this method can serve as a
/// reference for how to use Triton VM.
///
/// Note that all arithmetic is in the prime field with 2^64 - 2^32 + 1 elements. If the
/// provided public input or secret input contains elements larger than this, proof generation
/// will be aborted.
///
/// The program executed by Triton VM must terminate gracefully, i.e., with instruction `halt`.
/// If the program crashes, _e.g._, due to an out-of-bounds instruction pointer or a failing
/// `assert` instruction, proof generation will fail.
///
/// The default STARK parameters used by Triton VM give a (conjectured) security level of 160 bits.
pub fn prove_program(
    program: &Program,
    public_input: &[u64],
    secret_input: &[u64],
) -> Result<(StarkParameters, Claim, Proof)> {
    let canonical_representation_error =
        "input must contain only elements in canonical representation, i.e., \
        elements smaller than the prime field's modulus 2^64 - 2^32 + 1.";
    if public_input.iter().any(|&e| e > BFieldElement::MAX) {
        bail!("Public {canonical_representation_error})");
    }
    if secret_input.iter().any(|&e| e > BFieldElement::MAX) {
        bail!("Secret {canonical_representation_error}");
    }

    // Convert the public and secret inputs to BFieldElements.
    let public_input = public_input
        .iter()
        .map(|&e| BFieldElement::new(e))
        .collect::<Vec<_>>();
    let secret_input = secret_input
        .iter()
        .map(|&e| BFieldElement::new(e))
        .collect::<Vec<_>>();

    // Generate
    // - the witness required for proof generation, i.e., the Algebraic Execution Trace (AET), and
    // - the (public) output of the program.
    //
    // Crashes in the VM can occur for many reasons. For example:
    // - due to failing `assert` instructions,
    // - due to an out-of-bounds instruction pointer,
    // - if the program does not terminate gracefully, _i.e._, with instruction `halt`,
    // - if any of the two inputs does not conform to the program,
    // - because of a bug in the program, among other things.
    // If the VM crashes, proof generation will fail.
    let (aet, public_output) = program.trace_execution(public_input.clone(), secret_input)?;

    // Hash the program to obtain its digest.
    let program_digest = program.hash::<StarkHasher>();

    // The default parameters give a (conjectured) security level of 160 bits.
    let parameters = StarkParameters::default();

    // Set up the claim that is to be proven. The claim contains all public information. The
    // proof is zero-knowledge with respect to everything else.
    let claim = Claim {
        program_digest,
        input: public_input,
        output: public_output,
    };

    // Generate the proof.
    let proof = Stark::prove(&parameters, &claim, &aet, &mut None);

    Ok((parameters, claim, proof))
}

/// A convenience function for proving a [`Claim`] and the program that claim corresponds to.
/// Method [`prove_program`] gives a simpler interface with less control.
pub fn prove(
    parameters: &StarkParameters,
    claim: &Claim,
    program: &Program,
    secret_input: &[BFieldElement],
) -> Result<Proof> {
    let program_digest = program.hash::<StarkHasher>();
    if program_digest != claim.program_digest {
        bail!("Program digest must match claimed program digest.");
    }
    let (aet, public_output) =
        program.trace_execution(claim.input.clone(), secret_input.to_vec())?;
    if public_output != claim.output {
        bail!("Program output must match claimed program output.");
    }
    let proof = Stark::prove(parameters, claim, &aet, &mut None);
    Ok(proof)
}

/// Verify a proof generated by [`prove`] or [`prove_program`].
#[must_use]
pub fn verify(parameters: &StarkParameters, claim: &Claim, proof: &Proof) -> bool {
    Stark::verify(parameters, claim, proof, &mut None).unwrap_or(false)
}

#[cfg(test)]
mod public_interface_tests {
    use crate::shared_tests::create_proofs_directory;
    use crate::shared_tests::load_proof;
    use crate::shared_tests::proof_file_exists;
    use crate::shared_tests::save_proof;
    use crate::stark::StarkHasher;

    use super::*;

    #[test]
    pub fn lockscript_test() {
        // Program proves the knowledge of a hash preimage
        let program = triton_program!(
            divine divine divine divine divine
            hash pop pop pop pop pop
            push 09456474867485907852
            push 12765666850723567758
            push 08551752384389703074
            push 03612858832443241113
            push 12064501419749299924
            assert_vector
            read_io read_io read_io read_io read_io
            halt
        );

        let secret_input = vec![
            7534225252725590272,
            10242377928140984092,
            4934077665495234419,
            1344204945079929819,
            2308095244057597075,
        ];
        let public_input = vec![
            4541691341642414223,
            488727826369776966,
            18398227966153280881,
            6431838875748878863,
            17174585125955027015,
        ];

        let (parameters, claim, proof) =
            prove_program(&program, &public_input, &secret_input).unwrap();
        assert_eq!(
            StarkParameters::default(),
            parameters,
            "Prover must return default STARK parameters"
        );
        let expected_program_digest = program.hash::<StarkHasher>();
        assert_eq!(
            expected_program_digest, claim.program_digest,
            "program digest must match program"
        );
        assert_eq!(
            public_input,
            claim.public_input(),
            "Claimed input must match supplied input"
        );
        assert!(
            claim.output.is_empty(),
            "Output must be empty for program that doesn't write to output"
        );
        let verdict = verify(&parameters, &claim, &proof);
        assert!(verdict);
    }

    #[test]
    fn lib_prove_verify() {
        let parameters = StarkParameters::default();
        let program = triton_program!(push 1 assert halt);
        let claim = Claim {
            program_digest: program.hash::<StarkHasher>(),
            input: vec![],
            output: vec![],
        };

        let proof = prove(&parameters, &claim, &program, &[]).unwrap();
        let verdict = verify(&parameters, &claim, &proof);
        assert!(verdict);
    }

    #[test]
    fn save_proof_to_and_load_from_disk_test() {
        let filename = "nop_halt.tsp";
        if !proof_file_exists(filename) {
            create_proofs_directory().unwrap();
        }

        let program = triton_program!(nop halt);
        let (_, _, proof) = prove_program(&program, &[], &[]).unwrap();

        save_proof(filename, proof.clone()).unwrap();
        let loaded_proof = load_proof(filename).unwrap();

        assert_eq!(proof, loaded_proof);
    }
}
