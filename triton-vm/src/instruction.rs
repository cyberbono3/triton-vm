use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Display;
use std::result;

use anyhow::anyhow;
use anyhow::Result;
use get_size::GetSize;
use itertools::Itertools;
use lazy_static::lazy_static;
use serde_derive::Deserialize;
use serde_derive::Serialize;
use strum::EnumCount;
use strum::IntoEnumIterator;
use strum_macros::EnumCount;
use strum_macros::EnumIter;
use twenty_first::shared_math::b_field_element::BFieldElement;
use twenty_first::shared_math::b_field_element::BFIELD_ZERO;

use AnInstruction::*;

use crate::instruction::InstructionBit::*;
use crate::op_stack::OpStackElement;
use crate::op_stack::OpStackElement::*;

/// An `Instruction` has `call` addresses encoded as absolute integers.
pub type Instruction = AnInstruction<BFieldElement>;
pub const ALL_INSTRUCTIONS: [Instruction; Instruction::COUNT] = all_instructions_without_args();
pub const ALL_INSTRUCTION_NAMES: [&str; Instruction::COUNT] = all_instruction_names();

lazy_static! {
    pub static ref OPCODE_TO_INSTRUCTION_MAP: HashMap<u32, Instruction> = {
        let mut opcode_to_instruction_map = HashMap::new();
        for instruction in Instruction::iter() {
            opcode_to_instruction_map.insert(instruction.opcode(), instruction);
        }
        opcode_to_instruction_map
    };
}

/// A `LabelledInstruction` has `call` addresses encoded as label names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LabelledInstruction {
    /// Instructions belong to the ISA:
    ///
    /// <https://triton-vm.org/spec/isa.html>
    Instruction(AnInstruction<String>),

    /// Labels look like "`<name>:`" and are translated into absolute addresses.
    Label(String),
}

impl LabelledInstruction {
    pub const fn grows_op_stack(&self) -> bool {
        self.op_stack_size_influence() > 0
    }

    pub const fn does_not_change_op_stack_size(&self) -> bool {
        self.op_stack_size_influence() == 0
    }

    pub const fn shrinks_op_stack(&self) -> bool {
        self.op_stack_size_influence() < 0
    }

    pub const fn op_stack_size_influence(&self) -> i32 {
        match self {
            LabelledInstruction::Instruction(instruction) => instruction.op_stack_size_influence(),
            LabelledInstruction::Label(_) => 0,
        }
    }
}

impl Display for LabelledInstruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LabelledInstruction::Instruction(instr) => write!(f, "{instr}"),
            LabelledInstruction::Label(label_name) => write!(f, "{label_name}:"),
        }
    }
}

/// Helps printing instructions with their labels.
pub fn stringify_instructions(instructions: &[LabelledInstruction]) -> String {
    instructions.iter().join("\n")
}

/// A Triton VM instruction. See the
/// [Instruction Set Architecture](https://triton-vm.org/spec/isa.html)
/// for more details.
///
/// The type parameter `Dest` describes the type of addresses (absolute or labels).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumCount, EnumIter, GetSize, Serialize, Deserialize,
)]
pub enum AnInstruction<Dest: PartialEq + Default> {
    // OpStack manipulation
    Pop,
    Push(BFieldElement),
    Divine,
    Dup(OpStackElement),
    Swap(OpStackElement),

    // Control flow
    Nop,
    Skiz,
    Call(Dest),
    Return,
    Recurse,
    Assert,
    Halt,

    // Memory access
    ReadMem,
    WriteMem,

    // Hashing-related
    Hash,
    DivineSibling,
    AssertVector,
    AbsorbInit,
    Absorb,
    Squeeze,

    // Base field arithmetic on stack
    Add,
    Mul,
    Invert,
    Eq,

    // Bitwise arithmetic on stack
    Split,
    Lt,
    And,
    Xor,
    Log2Floor,
    Pow,
    Div,
    PopCount,

    // Extension field arithmetic on stack
    XxAdd,
    XxMul,
    XInvert,
    XbMul,

    // Read/write
    ReadIo,
    WriteIo,
}

impl<Dest: PartialEq + Default> AnInstruction<Dest> {
    /// Assign a unique positive integer to each `Instruction`.
    pub const fn opcode(&self) -> u32 {
        match self {
            Pop => 2,
            Push(_) => 1,
            Divine => 8,
            Dup(_) => 9,
            Swap(_) => 17,
            Nop => 16,
            Skiz => 10,
            Call(_) => 25,
            Return => 24,
            Recurse => 32,
            Assert => 18,
            Halt => 0,
            ReadMem => 40,
            WriteMem => 26,
            Hash => 48,
            DivineSibling => 56,
            AssertVector => 64,
            AbsorbInit => 72,
            Absorb => 80,
            Squeeze => 88,
            Add => 34,
            Mul => 42,
            Invert => 96,
            Eq => 50,
            Split => 4,
            Lt => 6,
            And => 14,
            Xor => 22,
            Log2Floor => 12,
            Pow => 30,
            Div => 20,
            PopCount => 28,
            XxAdd => 104,
            XxMul => 112,
            XInvert => 120,
            XbMul => 58,
            ReadIo => 128,
            WriteIo => 66,
        }
    }

    const fn name(&self) -> &'static str {
        match self {
            Pop => "pop",
            Push(_) => "push",
            Divine => "divine",
            Dup(_) => "dup",
            Swap(_) => "swap",
            Nop => "nop",
            Skiz => "skiz",
            Call(_) => "call",
            Return => "return",
            Recurse => "recurse",
            Assert => "assert",
            Halt => "halt",
            ReadMem => "read_mem",
            WriteMem => "write_mem",
            Hash => "hash",
            DivineSibling => "divine_sibling",
            AssertVector => "assert_vector",
            AbsorbInit => "absorb_init",
            Absorb => "absorb",
            Squeeze => "squeeze",
            Add => "add",
            Mul => "mul",
            Invert => "invert",
            Eq => "eq",
            Split => "split",
            Lt => "lt",
            And => "and",
            Xor => "xor",
            Log2Floor => "log_2_floor",
            Pow => "pow",
            Div => "div",
            PopCount => "pop_count",
            XxAdd => "xxadd",
            XxMul => "xxmul",
            XInvert => "xinvert",
            XbMul => "xbmul",
            ReadIo => "read_io",
            WriteIo => "write_io",
        }
    }

    pub fn opcode_b(&self) -> BFieldElement {
        self.opcode().into()
    }

    pub fn size(&self) -> usize {
        match matches!(self, Push(_) | Dup(_) | Swap(_) | Call(_)) {
            true => 2,
            false => 1,
        }
    }

    /// Get the i'th instruction bit
    pub fn ib(&self, arg: InstructionBit) -> BFieldElement {
        let opcode = self.opcode();
        let bit_number: usize = arg.into();

        ((opcode >> bit_number) & 1).into()
    }

    fn map_call_address<F, NewDest: PartialEq + Default>(&self, f: F) -> AnInstruction<NewDest>
    where
        F: Fn(&Dest) -> NewDest,
    {
        match self {
            Pop => Pop,
            Push(x) => Push(*x),
            Divine => Divine,
            Dup(x) => Dup(*x),
            Swap(x) => Swap(*x),
            Nop => Nop,
            Skiz => Skiz,
            Call(label) => Call(f(label)),
            Return => Return,
            Recurse => Recurse,
            Assert => Assert,
            Halt => Halt,
            ReadMem => ReadMem,
            WriteMem => WriteMem,
            Hash => Hash,
            DivineSibling => DivineSibling,
            AssertVector => AssertVector,
            AbsorbInit => AbsorbInit,
            Absorb => Absorb,
            Squeeze => Squeeze,
            Add => Add,
            Mul => Mul,
            Invert => Invert,
            Eq => Eq,
            Split => Split,
            Lt => Lt,
            And => And,
            Xor => Xor,
            Log2Floor => Log2Floor,
            Pow => Pow,
            Div => Div,
            PopCount => PopCount,
            XxAdd => XxAdd,
            XxMul => XxMul,
            XInvert => XInvert,
            XbMul => XbMul,
            ReadIo => ReadIo,
            WriteIo => WriteIo,
        }
    }

    pub const fn grows_op_stack(&self) -> bool {
        self.op_stack_size_influence() > 0
    }

    pub const fn does_not_change_op_stack_size(&self) -> bool {
        self.op_stack_size_influence() == 0
    }

    pub const fn shrinks_op_stack(&self) -> bool {
        self.op_stack_size_influence() < 0
    }

    pub const fn op_stack_size_influence(&self) -> i32 {
        match self {
            Pop => -1,
            Push(_) => 1,
            Divine => 1,
            Dup(_) => 1,
            Swap(_) => 0,
            Nop => 0,
            Skiz => -1,
            Call(_) => 0,
            Return => 0,
            Recurse => 0,
            Assert => -1,
            Halt => 0,
            ReadMem => 1,
            WriteMem => -1,
            Hash => 0,
            DivineSibling => 0,
            AssertVector => 0,
            AbsorbInit => 0,
            Absorb => 0,
            Squeeze => 0,
            Add => -1,
            Mul => -1,
            Invert => 0,
            Eq => -1,
            Split => 1,
            Lt => -1,
            And => -1,
            Xor => -1,
            Log2Floor => 0,
            Pow => -1,
            Div => 0,
            PopCount => 0,
            XxAdd => 0,
            XxMul => 0,
            XInvert => 0,
            XbMul => -1,
            ReadIo => 1,
            WriteIo => -1,
        }
    }

    /// Indicates whether the instruction operates on base field elements that are also u32s.
    pub fn is_u32_instruction(&self) -> bool {
        matches!(
            self,
            Split | Lt | And | Xor | Log2Floor | Pow | Div | PopCount
        )
    }
}

impl<Dest: Display + PartialEq + Default> Display for AnInstruction<Dest> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())?;
        match self {
            Push(arg) => write!(f, " {arg}"),
            Dup(arg) | Swap(arg) => write!(f, " {arg}"),
            Call(arg) => write!(f, " {arg}"),
            _ => Ok(()),
        }
    }
}

impl Instruction {
    /// Get the argument of the instruction, if it has one.
    pub fn arg(&self) -> Option<BFieldElement> {
        match self {
            Push(arg) | Call(arg) => Some(*arg),
            Dup(arg) | Swap(arg) => Some(arg.into()),
            _ => None,
        }
    }

    /// `true` iff the instruction has an argument.
    pub fn has_arg(&self) -> bool {
        self.arg().is_some()
    }

    /// Change the argument of the instruction, if it has one.
    /// Returns `None` if the instruction does not have an argument or
    /// if the argument is out of range.
    #[must_use]
    pub fn change_arg(&self, new_arg: BFieldElement) -> Option<Self> {
        let maybe_ord_16 = new_arg.value().try_into();
        match self {
            Push(_) => Some(Push(new_arg)),
            Dup(_) => match maybe_ord_16 {
                Ok(ord_16) => Some(Dup(ord_16)),
                Err(_) => None,
            },
            Swap(_) => match maybe_ord_16 {
                Ok(ord_16) => Some(Swap(ord_16)),
                Err(_) => None,
            },
            Call(_) => Some(Call(new_arg)),
            _ => None,
        }
    }
}

impl TryFrom<u32> for Instruction {
    type Error = anyhow::Error;

    fn try_from(opcode: u32) -> Result<Self> {
        OPCODE_TO_INSTRUCTION_MAP
            .get(&opcode)
            .copied()
            .ok_or(anyhow!("No instruction with opcode {opcode} exists."))
    }
}

impl TryFrom<u64> for Instruction {
    type Error = anyhow::Error;

    fn try_from(opcode: u64) -> Result<Self> {
        let opcode = u32::try_from(opcode)?;
        opcode.try_into()
    }
}

impl TryFrom<usize> for Instruction {
    type Error = anyhow::Error;

    fn try_from(opcode: usize) -> Result<Self> {
        let opcode = u32::try_from(opcode)?;
        opcode.try_into()
    }
}

/// Convert a program with labels to a program with absolute addresses.
pub fn convert_all_labels_to_addresses(program: &[LabelledInstruction]) -> Vec<Instruction> {
    let label_map = build_label_to_address_map(program);
    program
        .iter()
        .flat_map(|instruction| convert_label_to_address_for_instruction(instruction, &label_map))
        .collect()
}

pub(crate) fn build_label_to_address_map(
    program: &[LabelledInstruction],
) -> HashMap<String, usize> {
    use LabelledInstruction::*;

    let mut label_map = HashMap::new();
    let mut instruction_pointer = 0;

    for labelled_instruction in program.iter() {
        match labelled_instruction {
            Label(label) => match label_map.entry(label.clone()) {
                Entry::Occupied(_) => panic!("Duplicate label: {label}"),
                Entry::Vacant(entry) => {
                    entry.insert(instruction_pointer);
                }
            },
            Instruction(instruction) => instruction_pointer += instruction.size(),
        }
    }
    label_map
}

/// Convert a single instruction with a labelled call target to an instruction with an absolute
/// address as the call target. Discards all labels.
fn convert_label_to_address_for_instruction(
    labelled_instruction: &LabelledInstruction,
    label_map: &HashMap<String, usize>,
) -> Option<Instruction> {
    match labelled_instruction {
        LabelledInstruction::Label(_) => None,
        LabelledInstruction::Instruction(instruction) => {
            let instruction_with_absolute_address = instruction.map_call_address(|label| {
                let &absolute_address = label_map
                    .get(label)
                    .unwrap_or_else(|| panic!("Label not found: {label}"));
                BFieldElement::new(absolute_address as u64)
            });
            Some(instruction_with_absolute_address)
        }
    }
}

const fn all_instructions_without_args() -> [AnInstruction<BFieldElement>; Instruction::COUNT] {
    [
        Pop,
        Push(BFIELD_ZERO),
        Divine,
        Dup(ST0),
        Swap(ST1),
        Nop,
        Skiz,
        Call(BFIELD_ZERO),
        Return,
        Recurse,
        Assert,
        Halt,
        ReadMem,
        WriteMem,
        Hash,
        DivineSibling,
        AssertVector,
        AbsorbInit,
        Absorb,
        Squeeze,
        Add,
        Mul,
        Invert,
        Eq,
        Split,
        Lt,
        And,
        Xor,
        Log2Floor,
        Pow,
        Div,
        PopCount,
        XxAdd,
        XxMul,
        XInvert,
        XbMul,
        ReadIo,
        WriteIo,
    ]
}

const fn all_instruction_names() -> [&'static str; Instruction::COUNT] {
    let mut names = [""; Instruction::COUNT];
    let mut i = 0;
    while i < Instruction::COUNT {
        names[i] = ALL_INSTRUCTIONS[i].name();
        i += 1;
    }
    names
}

/// Indicators for all the possible bits in an [`Instruction`](Instruction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, EnumCount, EnumIter)]
pub enum InstructionBit {
    #[default]
    IB0,
    IB1,
    IB2,
    IB3,
    IB4,
    IB5,
    IB6,
    IB7,
}

impl Display for InstructionBit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bit_index = usize::from(*self);
        write!(f, "{bit_index}")
    }
}

impl From<InstructionBit> for usize {
    fn from(instruction_bit: InstructionBit) -> Self {
        match instruction_bit {
            IB0 => 0,
            IB1 => 1,
            IB2 => 2,
            IB3 => 3,
            IB4 => 4,
            IB5 => 5,
            IB6 => 6,
            IB7 => 7,
        }
    }
}

impl TryFrom<usize> for InstructionBit {
    type Error = String;

    fn try_from(bit_index: usize) -> result::Result<Self, Self::Error> {
        match bit_index {
            0 => Ok(IB0),
            1 => Ok(IB1),
            2 => Ok(IB2),
            3 => Ok(IB3),
            4 => Ok(IB4),
            5 => Ok(IB5),
            6 => Ok(IB6),
            7 => Ok(IB7),
            _ => Err(format!(
                "Index {bit_index} is out of range for `InstructionBit`."
            )),
        }
    }
}

#[cfg(test)]
mod instruction_tests {
    use std::cmp::Ordering;
    use std::collections::HashMap;

    use itertools::Itertools;
    use num_traits::One;
    use num_traits::Zero;
    use rand::thread_rng;
    use rand::Rng;
    use strum::EnumCount;
    use strum::IntoEnumIterator;
    use twenty_first::shared_math::b_field_element::BFieldElement;
    use twenty_first::shared_math::b_field_element::BFIELD_ZERO;
    use twenty_first::shared_math::digest::Digest;

    use crate::instruction::*;
    use crate::op_stack::NUM_OP_STACK_REGISTERS;
    use crate::triton_asm;
    use crate::triton_program;
    use crate::vm::triton_vm_tests::test_program_for_call_recurse_return;
    use crate::NonDeterminism;
    use crate::Program;

    #[test]
    fn opcodes_are_unique() {
        let mut opcodes_to_instruction_map = HashMap::new();
        for instruction in Instruction::iter() {
            let opcode = instruction.opcode();
            let maybe_entry = opcodes_to_instruction_map.insert(opcode, instruction);
            if let Some(other_instruction) = maybe_entry {
                panic!("{other_instruction} and {instruction} both have opcode {opcode}.",);
            }
        }
    }

    #[test]
    fn number_of_instruction_bits_is_correct() {
        let all_opcodes = Instruction::iter().map(|instruction| instruction.opcode());
        let highest_opcode = all_opcodes.max().unwrap();
        let num_required_bits_for_highest_opcode = highest_opcode.ilog2() + 1;
        assert_eq!(
            InstructionBit::COUNT,
            num_required_bits_for_highest_opcode as usize
        );
    }

    #[test]
    fn parse_push_pop_test() {
        let program = triton_program!(push 1 push 1 add pop);
        let instructions = program.into_iter().collect_vec();
        let expected = vec![
            Push(BFieldElement::one()),
            Push(BFieldElement::one()),
            Add,
            Pop,
        ];

        assert_eq!(expected, instructions);
    }

    #[test]
    #[should_panic(expected = "Duplicate label: foo")]
    fn fail_on_duplicate_labels_test() {
        triton_program!(
            push 2
            call foo
            bar: push 2
            foo: push 3
            foo: push 4
            halt
        );
    }

    #[test]
    fn ib_registers_are_binary_test() {
        for instruction in ALL_INSTRUCTIONS {
            for instruction_bit in InstructionBit::iter() {
                let ib_value = instruction.ib(instruction_bit);
                assert!(ib_value.is_zero() || ib_value.is_one(),);
            }
        }
    }

    #[test]
    fn instruction_to_opcode_to_instruction_is_consistent_test() {
        for instr in ALL_INSTRUCTIONS {
            assert_eq!(instr, instr.opcode().try_into().unwrap());
        }
    }

    #[test]
    fn print_all_instructions_and_opcodes() {
        for instr in ALL_INSTRUCTIONS {
            println!("{:>3} {: <10}", instr.opcode(), format!("{}", instr.name()));
        }
    }

    #[test]
    /// Serves no other purpose than to increase code coverage results.
    fn run_constant_methods_test() {
        all_instructions_without_args();
        all_instruction_names();
    }

    #[test]
    fn convert_various_types_to_instructions() {
        let _push = Instruction::try_from(1_usize).unwrap();
        let _dup = Instruction::try_from(9_u64).unwrap();
        let _swap = Instruction::try_from(17_u32).unwrap();
        let _pop = Instruction::try_from(2_usize).unwrap();
    }

    #[test]
    fn change_arguments_of_various_instructions() {
        let push = Instruction::Push(0_u64.into()).change_arg(7_u64.into());
        let dup = Instruction::Dup(ST0).change_arg(1024_u64.into());
        let swap = Instruction::Swap(ST0).change_arg(1337_u64.into());
        let pop = Instruction::Pop.change_arg(7_u64.into());

        assert!(push.is_some());
        assert!(dup.is_none());
        assert!(swap.is_none());
        assert!(pop.is_none());
    }

    #[test]
    fn print_various_instructions() {
        println!("instruction_push: {:?}", Instruction::Push(7_u64.into()));
        println!("instruction_assert: {}", Instruction::Assert);
        println!("instruction_invert: {:?}", Instruction::Invert);
        println!("instruction_dup: {}", Instruction::Dup(ST14));
    }

    #[test]
    fn instruction_size_is_consistent_with_having_arguments() {
        for instruction in Instruction::iter() {
            match instruction.has_arg() {
                true => assert_eq!(2, instruction.size()),
                false => assert_eq!(1, instruction.size()),
            }
        }
    }

    #[test]
    fn opcodes_are_consistent_with_argument_indication_bit() {
        let argument_indicator_bit_mask = 1;
        for instruction in Instruction::iter() {
            let opcode = instruction.opcode();
            println!("Testing instruction {instruction} with opcode {opcode}.");
            assert_eq!(
                instruction.has_arg(),
                opcode & argument_indicator_bit_mask != 0
            );
        }
    }

    #[test]
    fn opcodes_are_consistent_with_shrink_stack_indication_bit() {
        let shrink_stack_indicator_bit_mask = 2;
        for instruction in Instruction::iter() {
            let opcode = instruction.opcode();
            println!("Testing instruction {instruction} with opcode {opcode}.");
            assert_eq!(
                instruction.shrinks_op_stack(),
                opcode & shrink_stack_indicator_bit_mask != 0
            );
        }
    }

    #[test]
    fn opcodes_are_consistent_with_u32_indication_bit() {
        let u32_indicator_bit_mask = 4;
        for instruction in Instruction::iter() {
            let opcode = instruction.opcode();
            println!("Testing instruction {instruction} with opcode {opcode}.");
            assert_eq!(
                instruction.is_u32_instruction(),
                opcode & u32_indicator_bit_mask != 0
            );
        }
    }

    #[test]
    fn instruction_bits_are_consistent() {
        for instruction_bit in InstructionBit::iter() {
            println!("Testing instruction bit {instruction_bit}.");
            let bit_index = usize::from(instruction_bit);
            let recovered_instruction_bit = InstructionBit::try_from(bit_index).unwrap();
            assert_eq!(instruction_bit, recovered_instruction_bit);
        }
    }

    #[test]
    fn instruction_bit_conversion_fails_for_invalid_bit_index() {
        let invalid_bit_index = thread_rng().gen_range(InstructionBit::COUNT..=usize::MAX);
        let maybe_instruction_bit = InstructionBit::try_from(invalid_bit_index);
        assert!(maybe_instruction_bit.is_err());
    }

    #[test]
    fn stringify_some_instructions() {
        let instructions = triton_asm!(push 3 invert push 2 mul push 1 add write_io halt);
        let code = stringify_instructions(&instructions);
        println!("{code}");
    }

    #[test]
    fn instructions_act_on_op_stack_as_indicated() {
        for test_instruction in all_instructions_without_args() {
            let (program, stack_size_before_test_instruction) =
                construct_test_program_for_instruction(test_instruction);
            let stack_size_after_test_instruction = terminal_op_stack_size_for_program(program);

            let stack_size_difference =
                stack_size_after_test_instruction.cmp(&stack_size_before_test_instruction);
            assert_op_stack_size_changed_as_instruction_indicates(
                test_instruction,
                stack_size_difference,
            );
        }
    }

    fn construct_test_program_for_instruction(
        instruction: AnInstruction<BFieldElement>,
    ) -> (Program, usize) {
        match instruction_requires_jump_stack_setup(instruction) {
            true => program_with_jump_stack_setup_for_instruction(),
            false => program_without_jump_stack_setup_for_instruction(instruction),
        }
    }

    fn instruction_requires_jump_stack_setup(instruction: Instruction) -> bool {
        matches!(instruction, Call(_) | Return | Recurse)
    }

    fn program_with_jump_stack_setup_for_instruction() -> (Program, usize) {
        let program = test_program_for_call_recurse_return().program;
        let stack_size = NUM_OP_STACK_REGISTERS;
        (program, stack_size)
    }

    fn program_without_jump_stack_setup_for_instruction(
        test_instruction: AnInstruction<BFieldElement>,
    ) -> (Program, usize) {
        let num_push_instructions = 10;
        let push_instructions = triton_asm![push 1; num_push_instructions];
        let program = triton_program!({&push_instructions} {test_instruction} nop halt);

        let stack_size_when_reaching_test_instruction =
            NUM_OP_STACK_REGISTERS + num_push_instructions;
        (program, stack_size_when_reaching_test_instruction)
    }

    fn terminal_op_stack_size_for_program(program: Program) -> usize {
        let public_input = vec![BFIELD_ZERO].into();
        let mock_digests = vec![Digest::default()];
        let non_determinism: NonDeterminism<_> = vec![BFIELD_ZERO].into();
        let non_determinism = non_determinism.with_digests(mock_digests);

        let terminal_state = program
            .debug_terminal_state(public_input, non_determinism, None, None)
            .unwrap();
        terminal_state.op_stack.stack.len()
    }

    fn assert_op_stack_size_changed_as_instruction_indicates(
        test_instruction: AnInstruction<BFieldElement>,
        stack_size_difference: Ordering,
    ) {
        assert_eq!(
            stack_size_difference == Ordering::Less,
            test_instruction.shrinks_op_stack(),
            "{test_instruction}"
        );
        assert_eq!(
            stack_size_difference == Ordering::Equal,
            test_instruction.does_not_change_op_stack_size(),
            "{test_instruction}"
        );
        assert_eq!(
            stack_size_difference == Ordering::Greater,
            test_instruction.grows_op_stack(),
            "{test_instruction}"
        );
    }

    #[test]
    fn labelled_instructions_act_on_op_stack_as_indicated() {
        for test_instruction in all_instructions_without_args() {
            let labelled_instruction =
                test_instruction.map_call_address(|_| "dummy_label".to_string());
            let labelled_instruction = LabelledInstruction::Instruction(labelled_instruction);

            assert_eq!(
                test_instruction.op_stack_size_influence(),
                labelled_instruction.op_stack_size_influence()
            );
            assert_eq!(
                test_instruction.grows_op_stack(),
                labelled_instruction.grows_op_stack()
            );
            assert_eq!(
                test_instruction.does_not_change_op_stack_size(),
                labelled_instruction.does_not_change_op_stack_size()
            );
            assert_eq!(
                test_instruction.shrinks_op_stack(),
                labelled_instruction.shrinks_op_stack()
            );
        }
    }

    #[test]
    fn labels_indicate_no_change_to_op_stack() {
        let labelled_instruction = LabelledInstruction::Label("dummy_label".to_string());
        assert_eq!(0, labelled_instruction.op_stack_size_influence());
        assert!(!labelled_instruction.grows_op_stack());
        assert!(labelled_instruction.does_not_change_op_stack_size());
        assert!(!labelled_instruction.shrinks_op_stack());
    }
}
