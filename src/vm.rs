use std::error::Error as StdError;
use std::fmt;

use gc_arena::{Collect, Gc, MutationContext};

use crate::{
    thread::LuaFrame, Closure, ClosureState, Error, Function, OpCode, RegisterIndex, String, Table,
    ThreadError, TypeError, UpValueDescriptor, Value, VarCount,
};

#[derive(Debug, Clone, Copy, Collect)]
#[collect(require_static)]
pub enum BinaryOperatorError {
    Add,
    Subtract,
    Multiply,
    FloatDivide,
    FloorDivide,
    Modulo,
    Exponentiate,
    UnaryNegate,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    LessThan,
    LessEqual,
}

impl StdError for BinaryOperatorError {}

impl fmt::Display for BinaryOperatorError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BinaryOperatorError::Add => write!(fmt, "cannot add values"),
            BinaryOperatorError::Subtract => write!(fmt, "cannot subtract values"),
            BinaryOperatorError::Multiply => write!(fmt, "cannot multiply values"),
            BinaryOperatorError::FloatDivide => write!(fmt, "cannot float divide values"),
            BinaryOperatorError::FloorDivide => write!(fmt, "cannot floor divide values"),
            BinaryOperatorError::Modulo => write!(fmt, "cannot modulo values"),
            BinaryOperatorError::Exponentiate => write!(fmt, "cannot exponentiate values"),
            BinaryOperatorError::UnaryNegate => write!(fmt, "cannot negate value"),
            BinaryOperatorError::BitAnd => write!(fmt, "cannot bitwise AND values"),
            BinaryOperatorError::BitOr => write!(fmt, "cannot bitwise OR values"),
            BinaryOperatorError::BitXor => write!(fmt, "cannot bitwise XOR values"),
            BinaryOperatorError::ShiftLeft => write!(fmt, "cannot shift value left"),
            BinaryOperatorError::ShiftRight => write!(fmt, "cannot shift value right"),
            BinaryOperatorError::LessThan => write!(fmt, "cannot compare values with <"),
            BinaryOperatorError::LessEqual => write!(fmt, "cannot compare values with <="),
        }
    }
}

// Runs the VM until the current LuaFrame may have been changed.  Returns the number of instructions
// left to run (or 0 if all requested instructions were run).
pub fn run_vm<'gc>(
    mc: MutationContext<'gc, '_>,
    mut lua_frame: LuaFrame<'gc, '_>,
    mut instructions: u32,
) -> Result<u32, Error<'gc>> {
    assert!(instructions != 0);
    let current_function = lua_frame.closure();
    if let Some(mut registers) = lua_frame.constant_registers() {
        loop {
            let op = current_function.0.proto.opcodes[*registers.pc];
            *registers.pc += 1;
            instructions -= 1;

            match op {
                OpCode::Move { dest, source } => {
                    registers.stack_frame[dest.0 as usize] =
                        registers.stack_frame[source.0 as usize];
                }

                OpCode::LoadConstant { dest, constant } => {
                    registers.stack_frame[dest.0 as usize] =
                        current_function.0.proto.constants[constant.0 as usize].to_value();
                }

                OpCode::LoadBool {
                    dest,
                    value,
                    skip_next,
                } => {
                    registers.stack_frame[dest.0 as usize] = Value::Boolean(value);
                    if skip_next {
                        *registers.pc += 1;
                    }
                }

                OpCode::LoadNil { dest, count } => {
                    for i in dest.0..dest.0 + count {
                        registers.stack_frame[i as usize] = Value::Nil;
                    }
                }

                OpCode::NewTable { dest } => {
                    registers.stack_frame[dest.0 as usize] = Value::Table(Table::new(mc));
                }

                OpCode::GetTableR { dest, table, key } => {
                    registers.stack_frame[dest.0 as usize] =
                        get_table(registers.stack_frame[table.0 as usize])?
                            .get(registers.stack_frame[key.0 as usize]);
                }

                OpCode::GetTableC { dest, table, key } => {
                    registers.stack_frame[dest.0 as usize] =
                        get_table(registers.stack_frame[table.0 as usize])?
                            .get(current_function.0.proto.constants[key.0 as usize].to_value())
                }

                OpCode::SetTableRR { table, key, value } => {
                    get_table(registers.stack_frame[table.0 as usize])?.set(
                        mc,
                        registers.stack_frame[key.0 as usize],
                        registers.stack_frame[value.0 as usize],
                    )?;
                }

                OpCode::SetTableRC { table, key, value } => {
                    get_table(registers.stack_frame[table.0 as usize])?.set(
                        mc,
                        registers.stack_frame[key.0 as usize],
                        current_function.0.proto.constants[value.0 as usize].to_value(),
                    )?;
                }

                OpCode::SetTableCR { table, key, value } => {
                    get_table(registers.stack_frame[table.0 as usize])?.set(
                        mc,
                        current_function.0.proto.constants[key.0 as usize].to_value(),
                        registers.stack_frame[value.0 as usize],
                    )?;
                }

                OpCode::SetTableCC { table, key, value } => {
                    get_table(registers.stack_frame[table.0 as usize])?.set(
                        mc,
                        current_function.0.proto.constants[key.0 as usize].to_value(),
                        current_function.0.proto.constants[value.0 as usize].to_value(),
                    )?;
                }

                OpCode::GetUpTableR { dest, table, key } => {
                    registers.stack_frame[dest.0 as usize] = get_table(
                        registers.get_upvalue(current_function.0.upvalues[table.0 as usize]),
                    )?
                    .get(registers.stack_frame[key.0 as usize]);
                }

                OpCode::GetUpTableC { dest, table, key } => {
                    registers.stack_frame[dest.0 as usize] = get_table(
                        registers.get_upvalue(current_function.0.upvalues[table.0 as usize]),
                    )?
                    .get(current_function.0.proto.constants[key.0 as usize].to_value())
                }

                OpCode::SetUpTableRR { table, key, value } => {
                    get_table(
                        registers.get_upvalue(current_function.0.upvalues[table.0 as usize]),
                    )?
                    .set(
                        mc,
                        registers.stack_frame[key.0 as usize],
                        registers.stack_frame[value.0 as usize],
                    )?;
                }

                OpCode::SetUpTableRC { table, key, value } => {
                    get_table(
                        registers.get_upvalue(current_function.0.upvalues[table.0 as usize]),
                    )?
                    .set(
                        mc,
                        registers.stack_frame[key.0 as usize],
                        current_function.0.proto.constants[value.0 as usize].to_value(),
                    )?;
                }

                OpCode::SetUpTableCR { table, key, value } => {
                    get_table(
                        registers.get_upvalue(current_function.0.upvalues[table.0 as usize]),
                    )?
                    .set(
                        mc,
                        current_function.0.proto.constants[key.0 as usize].to_value(),
                        registers.stack_frame[value.0 as usize],
                    )?;
                }

                OpCode::SetUpTableCC { table, key, value } => {
                    get_table(
                        registers.get_upvalue(current_function.0.upvalues[table.0 as usize]),
                    )?
                    .set(
                        mc,
                        current_function.0.proto.constants[key.0 as usize].to_value(),
                        current_function.0.proto.constants[value.0 as usize].to_value(),
                    )?;
                }

                OpCode::Call {
                    func,
                    args,
                    returns,
                } => {
                    lua_frame.call_function(func, args, returns)?;
                    break;
                }

                OpCode::TailCall { func, args } => {
                    lua_frame.tail_call_function(func, args)?;
                    break;
                }

                OpCode::Return { start, count } => {
                    lua_frame.return_upper(start, count)?;
                    break;
                }

                OpCode::VarArgs { dest, count } => {
                    lua_frame.varargs(dest, count)?;
                    break;
                }

                OpCode::Jump {
                    offset,
                    close_upvalues,
                } => {
                    *registers.pc = add_offset(*registers.pc, offset);
                    if let Some(r) = close_upvalues.to_u8() {
                        registers.close_upvalues(RegisterIndex(r));
                    }
                }

                OpCode::Test { value, is_true } => {
                    let value = registers.stack_frame[value.0 as usize];
                    if value.to_bool() == is_true {
                        *registers.pc += 1;
                    }
                }

                OpCode::TestSet {
                    dest,
                    value,
                    is_true,
                } => {
                    let value = registers.stack_frame[value.0 as usize];
                    if value.to_bool() == is_true {
                        *registers.pc += 1;
                    } else {
                        registers.stack_frame[dest.0 as usize] = value;
                    }
                }

                OpCode::Closure { proto, dest } => {
                    let proto = current_function.0.proto.prototypes[proto.0 as usize];
                    let mut upvalues = Vec::new();
                    for &desc in &proto.upvalues {
                        match desc {
                            UpValueDescriptor::Environment => {
                                panic!("_ENV upvalue is only allowed on top-level closure");
                            }
                            UpValueDescriptor::ParentLocal(reg) => {
                                upvalues.push(registers.open_upvalue(reg));
                            }
                            UpValueDescriptor::Outer(uvindex) => {
                                upvalues.push(current_function.0.upvalues[uvindex.0 as usize]);
                            }
                        }
                    }

                    let closure = Closure(Gc::allocate(mc, ClosureState { proto, upvalues }));
                    registers.stack_frame[dest.0 as usize] =
                        Value::Function(Function::Closure(closure));
                }

                OpCode::NumericForPrep { base, jump } => {
                    registers.stack_frame[base.0 as usize] = registers.stack_frame[base.0 as usize]
                        .subtract(registers.stack_frame[base.0 as usize + 2])
                        .ok_or(BinaryOperatorError::Subtract)?;
                    *registers.pc = add_offset(*registers.pc, jump);
                }

                OpCode::NumericForLoop { base, jump } => {
                    match (
                        registers.stack_frame[base.0 as usize],
                        registers.stack_frame[base.0 as usize + 1],
                        registers.stack_frame[base.0 as usize + 2],
                    ) {
                        (Value::Integer(index), Value::Integer(limit), Value::Integer(step)) => {
                            let index = index + step;
                            registers.stack_frame[base.0 as usize] = Value::Integer(index);

                            let past_end = if step < 0 {
                                index < limit
                            } else {
                                limit < index
                            };
                            if !past_end {
                                *registers.pc = add_offset(*registers.pc, jump);
                                registers.stack_frame[base.0 as usize + 3] = Value::Integer(index);
                            }
                        }
                        (index, limit, step) => {
                            if let (Some(mut index), Some(limit), Some(step)) =
                                (index.to_number(), limit.to_number(), step.to_number())
                            {
                                index = index + step;
                                registers.stack_frame[base.0 as usize] = Value::Number(index);

                                let past_end = if step < 0.0 {
                                    index < limit
                                } else {
                                    limit < index
                                };
                                if !past_end {
                                    *registers.pc = add_offset(*registers.pc, jump);
                                    registers.stack_frame[base.0 as usize + 3] =
                                        Value::Number(index);
                                }
                            } else {
                                return Err(BinaryOperatorError::Add.into());
                            }
                        }
                    }
                }

                OpCode::GenericForCall { base, var_count } => {
                    lua_frame.call_function_non_destructive(
                        base,
                        2,
                        VarCount::constant(var_count),
                    )?;
                    break;
                }

                OpCode::GenericForLoop { base, jump } => {
                    if registers.stack_frame[base.0 as usize + 1].to_bool() {
                        registers.stack_frame[base.0 as usize] =
                            registers.stack_frame[base.0 as usize + 1];
                        *registers.pc = add_offset(*registers.pc, jump);
                    }
                }

                OpCode::SelfR { base, table, key } => {
                    let table = registers.stack_frame[table.0 as usize];
                    let key = current_function.0.proto.constants[key.0 as usize].to_value();
                    registers.stack_frame[base.0 as usize + 1] = table;
                    registers.stack_frame[base.0 as usize] = get_table(table)?.get(key);
                }

                OpCode::SelfC { base, table, key } => {
                    let table = registers.stack_frame[table.0 as usize];
                    let key = current_function.0.proto.constants[key.0 as usize].to_value();
                    registers.stack_frame[base.0 as usize + 1] = table;
                    registers.stack_frame[base.0 as usize] = get_table(table)?.get(key);
                }

                OpCode::Concat {
                    dest,
                    source,
                    count,
                } => {
                    registers.stack_frame[dest.0 as usize] = Value::String(
                        String::concat(
                            mc,
                            &registers.stack_frame
                                [source.0 as usize..source.0 as usize + count as usize],
                        )
                        .unwrap(),
                    );
                }

                OpCode::GetUpValue { source, dest } => {
                    registers.stack_frame[dest.0 as usize] =
                        registers.get_upvalue(current_function.0.upvalues[source.0 as usize]);
                }

                OpCode::SetUpValue { source, dest } => {
                    registers.set_upvalue(
                        current_function.0.upvalues[dest.0 as usize],
                        registers.stack_frame[source.0 as usize],
                    );
                }

                OpCode::Length { dest, source } => {
                    registers.stack_frame[dest.0 as usize] = Value::Integer(
                        get_table(registers.stack_frame[source.0 as usize])?.length(),
                    );
                }

                OpCode::EqRR {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    if (left == right) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::EqRC {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    if (left == right) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::EqCR {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    if (left == right) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::EqCC {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    if (left == right) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessRR {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    if (left.less_than(right).ok_or(BinaryOperatorError::LessThan)?) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessRC {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    if (left.less_than(right).ok_or(BinaryOperatorError::LessThan)?) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessCR {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    if (left.less_than(right).ok_or(BinaryOperatorError::LessThan)?) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessCC {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    if (left.less_than(right).ok_or(BinaryOperatorError::LessThan)?) == skip_if {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessEqRR {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    if (left
                        .less_equal(right)
                        .ok_or(BinaryOperatorError::LessEqual)?)
                        == skip_if
                    {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessEqRC {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    if (left
                        .less_equal(right)
                        .ok_or(BinaryOperatorError::LessEqual)?)
                        == skip_if
                    {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessEqCR {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    if (left
                        .less_equal(right)
                        .ok_or(BinaryOperatorError::LessEqual)?)
                        == skip_if
                    {
                        *registers.pc += 1;
                    }
                }

                OpCode::LessEqCC {
                    skip_if,
                    left,
                    right,
                } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    if (left
                        .less_equal(right)
                        .ok_or(BinaryOperatorError::LessEqual)?)
                        == skip_if
                    {
                        *registers.pc += 1;
                    }
                }

                OpCode::Not { dest, source } => {
                    let source = registers.stack_frame[source.0 as usize];
                    registers.stack_frame[dest.0 as usize] = source.not();
                }

                OpCode::Minus { dest, source } => {
                    let value = registers.stack_frame[source.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        value.negate().ok_or(BinaryOperatorError::UnaryNegate)?;
                }

                OpCode::BitNot { dest, source } => {
                    let value = registers.stack_frame[source.0 as usize];
                    registers.stack_frame[dest.0 as usize] = value
                        .bitwise_not()
                        .ok_or(BinaryOperatorError::UnaryNegate)?;
                }

                OpCode::AddRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.add(right).ok_or(BinaryOperatorError::Add)?;
                }

                OpCode::AddRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.add(right).ok_or(BinaryOperatorError::Add)?;
                }

                OpCode::AddCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.add(right).ok_or(BinaryOperatorError::Add)?;
                }

                OpCode::AddCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.add(right).ok_or(BinaryOperatorError::Add)?;
                }

                OpCode::SubRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.subtract(right).ok_or(BinaryOperatorError::Add)?;
                }

                OpCode::SubRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.subtract(right).ok_or(BinaryOperatorError::Subtract)?;
                }

                OpCode::SubCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.subtract(right).ok_or(BinaryOperatorError::Subtract)?;
                }

                OpCode::SubCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.subtract(right).ok_or(BinaryOperatorError::Subtract)?;
                }

                OpCode::MulRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.multiply(right).ok_or(BinaryOperatorError::Multiply)?;
                }

                OpCode::MulRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.multiply(right).ok_or(BinaryOperatorError::Multiply)?;
                }

                OpCode::MulCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.multiply(right).ok_or(BinaryOperatorError::Multiply)?;
                }

                OpCode::MulCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.multiply(right).ok_or(BinaryOperatorError::Multiply)?;
                }

                OpCode::DivRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .float_divide(right)
                        .ok_or(BinaryOperatorError::FloatDivide)?;
                }

                OpCode::DivRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .float_divide(right)
                        .ok_or(BinaryOperatorError::FloatDivide)?;
                }

                OpCode::DivCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .float_divide(right)
                        .ok_or(BinaryOperatorError::FloatDivide)?;
                }

                OpCode::DivCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .float_divide(right)
                        .ok_or(BinaryOperatorError::FloatDivide)?;
                }

                OpCode::IDivRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .floor_divide(right)
                        .ok_or(BinaryOperatorError::FloorDivide)?;
                }

                OpCode::IDivRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .floor_divide(right)
                        .ok_or(BinaryOperatorError::FloorDivide)?;
                }

                OpCode::IDivCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .floor_divide(right)
                        .ok_or(BinaryOperatorError::FloorDivide)?;
                }

                OpCode::IDivCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .floor_divide(right)
                        .ok_or(BinaryOperatorError::FloorDivide)?;
                }

                OpCode::ModRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.modulo(right).ok_or(BinaryOperatorError::Modulo)?;
                }

                OpCode::ModRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.modulo(right).ok_or(BinaryOperatorError::Modulo)?;
                }

                OpCode::ModCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.modulo(right).ok_or(BinaryOperatorError::Modulo)?;
                }

                OpCode::ModCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.modulo(right).ok_or(BinaryOperatorError::Modulo)?;
                }

                OpCode::PowRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .exponentiate(right)
                        .ok_or(BinaryOperatorError::Exponentiate)?;
                }

                OpCode::PowRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .exponentiate(right)
                        .ok_or(BinaryOperatorError::Exponentiate)?;
                }

                OpCode::PowCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .exponentiate(right)
                        .ok_or(BinaryOperatorError::Exponentiate)?;
                }

                OpCode::PowCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .exponentiate(right)
                        .ok_or(BinaryOperatorError::Exponentiate)?;
                }

                OpCode::BitAndRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_and(right).ok_or(BinaryOperatorError::BitAnd)?;
                }

                OpCode::BitAndRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_and(right).ok_or(BinaryOperatorError::BitAnd)?;
                }

                OpCode::BitAndCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_and(right).ok_or(BinaryOperatorError::BitAnd)?;
                }

                OpCode::BitAndCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_and(right).ok_or(BinaryOperatorError::BitAnd)?;
                }

                OpCode::BitOrRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_or(right).ok_or(BinaryOperatorError::BitOr)?;
                }

                OpCode::BitOrRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_or(right).ok_or(BinaryOperatorError::BitOr)?;
                }

                OpCode::BitOrCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_or(right).ok_or(BinaryOperatorError::BitOr)?;
                }

                OpCode::BitOrCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_or(right).ok_or(BinaryOperatorError::BitOr)?;
                }

                OpCode::BitXorRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_xor(right).ok_or(BinaryOperatorError::BitXor)?;
                }

                OpCode::BitXorRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_xor(right).ok_or(BinaryOperatorError::BitXor)?;
                }

                OpCode::BitXorCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_xor(right).ok_or(BinaryOperatorError::BitXor)?;
                }

                OpCode::BitXorCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] =
                        left.bitwise_xor(right).ok_or(BinaryOperatorError::BitXor)?;
                }

                OpCode::ShiftLeftRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_left(right)
                        .ok_or(BinaryOperatorError::ShiftLeft)?;
                }

                OpCode::ShiftLeftRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_left(right)
                        .ok_or(BinaryOperatorError::ShiftLeft)?;
                }

                OpCode::ShiftLeftCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_left(right)
                        .ok_or(BinaryOperatorError::ShiftLeft)?;
                }

                OpCode::ShiftLeftCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_left(right)
                        .ok_or(BinaryOperatorError::ShiftLeft)?;
                }

                OpCode::ShiftRightRR { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_right(right)
                        .ok_or(BinaryOperatorError::ShiftRight)?;
                }

                OpCode::ShiftRightRC { dest, left, right } => {
                    let left = registers.stack_frame[left.0 as usize];
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_right(right)
                        .ok_or(BinaryOperatorError::ShiftRight)?;
                }

                OpCode::ShiftRightCR { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = registers.stack_frame[right.0 as usize];
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_right(right)
                        .ok_or(BinaryOperatorError::ShiftRight)?;
                }

                OpCode::ShiftRightCC { dest, left, right } => {
                    let left = current_function.0.proto.constants[left.0 as usize].to_value();
                    let right = current_function.0.proto.constants[right.0 as usize].to_value();
                    registers.stack_frame[dest.0 as usize] = left
                        .shift_right(right)
                        .ok_or(BinaryOperatorError::ShiftRight)?;
                }
            }

            if instructions == 0 {
                break;
            }
        }
    } else {
        let pc = lua_frame.get_pc();
        let op = current_function.0.proto.opcodes[pc];
        lua_frame.set_pc(pc + 1);
        instructions -= 1;

        // Variable stack frame, only varargs consuming operators are allowed
        match op {
            OpCode::Call {
                func,
                args,
                returns,
            } => {
                lua_frame.call_function(func, args, returns)?;
            }

            OpCode::TailCall { func, args } => {
                lua_frame.tail_call_function(func, args)?;
            }

            OpCode::Return { start, count } => {
                lua_frame.return_upper(start, count)?;
            }

            OpCode::VarArgs { dest, count } => {
                lua_frame.varargs(dest, count)?;
            }

            OpCode::GenericForCall { base, var_count } => {
                lua_frame.call_function_non_destructive(base, 2, VarCount::constant(var_count))?;
            }
            _ => return Err(ThreadError::ExpectedVariable(false).into()),
        }
    }

    Ok(instructions)
}

fn get_table<'gc>(value: Value<'gc>) -> Result<Table<'gc>, TypeError> {
    match value {
        Value::Table(t) => Ok(t),
        val => Err(TypeError {
            expected: "table",
            found: val.type_name(),
        }),
    }
}

fn add_offset(pc: usize, offset: i16) -> usize {
    if offset > 0 {
        pc.checked_add(offset as usize).unwrap()
    } else if offset < 0 {
        pc.checked_sub(-offset as usize).unwrap()
    } else {
        pc
    }
}
