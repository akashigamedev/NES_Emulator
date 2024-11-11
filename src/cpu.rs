#![allow(unused_variables)]
#![allow(dead_code)]
use std::collections::HashMap;

use crate::opcodes;

bitflags! {
    #[derive(Clone, Copy)]
    pub struct CpuFlags: u8 {
        const CARRY                     = 0b0000_0001;
        const ZERO                      = 0b0000_0010;
        const INTERRUPT_DISABLE         = 0b0000_0100;
        const DECIMAL_MODE              = 0b0000_1000;
        const BREAK                     = 0b0001_0000;
        const BREAK2                    = 0b0010_0000;
        const OVERFLOW                  = 0b0100_0000;
        const NEGATIVE                  = 0b1000_0000;
    }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPage_X,
    ZeroPage_Y,
    Absolute,
    Absolute_X,
    Absolute_Y,
    Indirect_X,
    Indirect_Y,
    NoneAddressing,
}

pub struct CPU {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: CpuFlags,
    pub program_counter: u16,
    pub stack_pointer: u8,
    pub memory: [u8; 0xFFFF],
}

trait Mem {
    fn mem_read(&self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

impl Mem for CPU {
    fn mem_read(&self, addr: u16) -> u8 {
        self.memory[addr as usize]
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.memory[addr as usize] = data;
    }
}

impl CPU {
    pub fn new() -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            status: CpuFlags::from_bits_truncate(0b100100),
            program_counter: 0,
            stack_pointer: STACK_RESET,
            memory: [0; 0xFFFF],
        }
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.program_counter = self.mem_read_u16(0xFFFC);
        self.stack_pointer = STACK_RESET;
        self.status = CpuFlags::from_bits_truncate(0b100100);
    }

    fn get_operand_address(&self, mode: &AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => self.program_counter,
            AddressingMode::ZeroPage => self.mem_read(self.program_counter) as u16,
            AddressingMode::Absolute => self.mem_read_u16(self.program_counter),
            AddressingMode::ZeroPage_X => {
                let pos = self.mem_read(self.program_counter);
                let addr = pos.wrapping_add(self.register_x) as u16;
                addr
            }
            AddressingMode::ZeroPage_Y => {
                let pos = self.mem_read(self.program_counter);
                let addr = pos.wrapping_add(self.register_y) as u16;
                addr
            }
            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(self.program_counter);
                let addr = base.wrapping_add(self.register_x as u16);
                addr
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(self.program_counter);
                let addr = base.wrapping_add(self.register_y as u16);
                addr
            }
            AddressingMode::Indirect_X => {
                let base = self.mem_read(self.program_counter);
                let ptr: u8 = (base as u8).wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                (hi as u16) << 8 | (lo as u16)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(self.program_counter);

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);
                let deref = deref_base.wrapping_add(self.register_y as u16);
                deref
            }
            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode);
            }
        }
    }

    fn set_register_a(&mut self, value: u8) {
        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn add_to_register_a(&mut self, data: u8) {
        let sum = self.register_a as u16
            + data as u16
            + (if self.status.contains(CpuFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry = sum > 0xFF;

        if carry {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.register_a) & 0x80 != 0 {
            self.status.insert(CpuFlags::OVERFLOW);
        } else {
            self.status.remove(CpuFlags::OVERFLOW);
        }

        self.set_register_a(result);
    }

    fn stack_pop(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.mem_read(STACK as u16 + self.stack_pointer as u16)
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.stack_pointer as u16, data);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;

        hi << 8 | lo
    }

    fn branch(&mut self, condition: bool) {
        if condition {
            let jump: i8 = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add(jump as u16);

            self.program_counter = jump_addr;
        }
    }

    pub fn load(&mut self, program: Vec<u8>) {
        self.memory[0x8000..(0x8000 + program.len())].copy_from_slice(&program);
        self.mem_write_u16(0xFFFC, 0x8000);
    }

    pub fn load_and_run(&mut self, program: Vec<u8>) {
        self.load(program);
        self.reset();
        self.run();
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        if result & 0b1000_0000 != 0 {
            self.status.insert(CpuFlags::NEGATIVE);
        } else {
            self.status.remove(CpuFlags::NEGATIVE);
        }
    }

    // a9 c0 aa e8 00
    pub fn run(&mut self) {
        let ref opcodes: HashMap<u8, &'static opcodes::OpCode> = *opcodes::OPCODES_MAP;

        loop {
            let code = self.mem_read(self.program_counter);
            self.program_counter += 1;
            let program_counter_state = self.program_counter;

            let opcode: &&opcodes::OpCode = opcodes
                .get(&code)
                .expect(&format!("Opcode {:?} is not recognized", code));

            match code {
                // LDA
                0xa9 | 0xa5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let value = self.mem_read(addr);
                    self.register_a = value;
                    self.update_zero_and_negative_flags(self.register_a);
                }
                // LDX
                0xA2 | 0xA6 | 0xB6 | 0xAE | 0xBE => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let value = self.mem_read(addr);
                    self.register_x = value;
                    self.update_zero_and_negative_flags(self.register_x);
                }
                // LDY
                0xA0 | 0xA4 | 0xB4 | 0xAC | 0xBC => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let value = self.mem_read(addr);
                    self.register_y = value;
                    self.update_zero_and_negative_flags(self.register_y);
                }
                // STA
                0x85 | 0x95 | 0x8D | 0x9D | 0x99 | 0x81 | 0x91 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_a);
                }
                // STX
                0x86 | 0x96 | 0x8E => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_x);
                }
                // STY
                0x84 | 0x94 | 0x8C => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_y);
                }
                // TAX
                0xAA => {
                    self.register_x = self.register_a;
                    self.update_zero_and_negative_flags(self.register_x);
                }
                // TAY
                0xA8 => {
                    self.register_y = self.register_a;
                    self.update_zero_and_negative_flags(self.register_y);
                }
                // TSX
                0xBA => {
                    self.register_x = self.stack_pointer;
                    self.update_zero_and_negative_flags(self.register_x);
                }
                // TXA
                0x8A => {
                    self.register_a = self.register_x;
                    self.update_zero_and_negative_flags(self.register_a);
                }
                // TYA
                0x98 => {
                    self.register_a = self.register_y;
                    self.update_zero_and_negative_flags(self.register_a);
                }
                // TXS
                0x9A => {
                    self.stack_pointer = self.register_x;
                }
                // INX
                0xe8 => {
                    self.register_x = self.register_x.wrapping_add(1);
                    self.update_zero_and_negative_flags(self.register_x);
                }
                // INY
                0xC8 => {
                    self.register_y = self.register_y.wrapping_add(1);
                    self.update_zero_and_negative_flags(self.register_y);
                }
                // PHA
                0x48 => {
                    self.stack_push(self.register_a);
                }
                // PHP
                0x08 => {
                    let mut flags = self.status.clone();
                    flags.insert(CpuFlags::BREAK);
                    flags.insert(CpuFlags::BREAK2);
                    self.stack_push(flags.bits());
                }
                // PLA
                0x68 => {
                    let data = self.stack_pop();
                    self.set_register_a(data);
                }
                // PLP
                0x28 => {
                    let bits = self.stack_pop();
                    self.status = CpuFlags::from_bits(bits).unwrap();
                    self.status.remove(CpuFlags::BREAK);
                    self.status.insert(CpuFlags::BREAK2);
                }
                // DEX
                0xCA => {
                    self.register_x = self.register_x.wrapping_sub(1);
                    self.update_zero_and_negative_flags(self.register_x);
                }
                // DEY
                0x88 => {
                    self.register_y = self.register_y.wrapping_sub(1);
                    self.update_zero_and_negative_flags(self.register_y);
                }
                // SEC
                0x38 => {
                    self.status.insert(CpuFlags::CARRY);
                }
                // CLC
                0x18 => {
                    self.status.remove(CpuFlags::CARRY);
                }
                // SEI
                0x78 => {
                    self.status.insert(CpuFlags::INTERRUPT_DISABLE);
                }
                // CLI
                0x58 => {
                    self.status.remove(CpuFlags::INTERRUPT_DISABLE);
                }
                // SED
                0xF8 => {
                    self.status.insert(CpuFlags::DECIMAL_MODE);
                }
                // CLD
                0xD8 => {
                    self.status.remove(CpuFlags::DECIMAL_MODE);
                }
                // CLV
                0xB8 => {
                    self.status.remove(CpuFlags::OVERFLOW);
                }
                // ADC
                0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.add_to_register_a(data);
                }
                // SBC
                0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.add_to_register_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
                }
                // CMP
                0xC9 | 0xC5 | 0xD5 | 0xCD | 0xDD | 0xD9 | 0xC1 | 0xD1 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    if self.register_a >= data {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }

                    self.update_zero_and_negative_flags(self.register_a.wrapping_sub(data));
                }
                // CPX
                0xE0 | 0xE4 | 0xEC => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    if self.register_x >= data {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }

                    self.update_zero_and_negative_flags(self.register_x.wrapping_sub(data));
                }
                // CPY
                0xC0 | 0xC4 | 0xCC => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    if self.register_y >= data {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }

                    self.update_zero_and_negative_flags(self.register_y.wrapping_sub(data));
                }
                // AND
                0x29 | 0x25 | 0x35 | 0x2D | 0x3D | 0x39 | 0x21 | 0x31 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.set_register_a(data & self.register_a);
                }
                // ORA
                0x09 | 0x05 | 0x15 | 0x0D | 0x1D | 0x19 | 0x01 | 0x11 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.set_register_a(data | self.register_a);
                }
                // EOR
                0x49 | 0x45 | 0x55 | 0x4D | 0x5D | 0x59 | 0x41 | 0x51 => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.set_register_a(data ^ self.register_a);
                }
                // BIT
                0x24 | 0x2C => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    let and = self.register_a & data;
                    if and == 0 {
                        self.status.insert(CpuFlags::ZERO);
                    } else {
                        self.status.remove(CpuFlags::ZERO);
                    }

                    self.status.set(CpuFlags::NEGATIVE, data & 0b10000000 > 0);
                    self.status.set(CpuFlags::OVERFLOW, data & 0b01000000 > 0);
                }
                // ASL
                0x0A => {
                    let mut data = self.register_a;
                    if data >> 7 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data << 1;
                    self.set_register_a(data);
                }
                // ASL
                0x06 | 0x16 | 0x0E | 0x1E => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    if data >> 7 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data << 1;
                    self.mem_write(addr, data);
                    self.update_zero_and_negative_flags(data);
                }
                // LSR
                0x4A => {
                    let mut data = self.register_a;
                    if data >> 7 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data >> 1;
                    self.set_register_a(data);
                }
                // LSR
                0x46 | 0x56 | 0x4E | 0x5E => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    if data & 1 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data >> 1;
                    self.mem_write(addr, data);
                    self.update_zero_and_negative_flags(data);
                }
                // ROL
                0x2A => {
                    let mut data = self.register_a;
                    let old_carry = self.status.contains(CpuFlags::CARRY);

                    if data >> 7 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data << 1;
                    if old_carry {
                        data = data | 1;
                    }
                    self.set_register_a(data);
                }
                // ROL
                0x26 | 0x36 | 0x2E | 0x3E => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    let old_carry = self.status.contains(CpuFlags::CARRY);

                    if data >> 7 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data << 1;
                    if old_carry {
                        data = data | 1;
                    }
                    self.mem_write(addr, data);
                    self.update_zero_and_negative_flags(data);
                }
                // ROR
                0x6A => {
                    let mut data = self.register_a;
                    let old_carry = self.status.contains(CpuFlags::CARRY);

                    if data & 1 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data >> 1;
                    if old_carry {
                        data = data | 0b10000000;
                    }
                    self.set_register_a(data);
                }
                // ROR
                0x66 | 0x76 | 0x6E | 0x7E => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let mut data = self.mem_read(addr);
                    let old_carry = self.status.contains(CpuFlags::CARRY);

                    if data & 1 == 1 {
                        self.status.insert(CpuFlags::CARRY);
                    } else {
                        self.status.remove(CpuFlags::CARRY);
                    }
                    data = data >> 1;
                    if old_carry {
                        data = data | 0b10000000;
                    }
                    self.mem_write(addr, data);
                    self.update_zero_and_negative_flags(data);
                }
                // INC
                0xE6 | 0xF6 | 0xEE | 0xFE => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.mem_write(addr, data.wrapping_add(1));
                    self.update_zero_and_negative_flags(data);
                }
                // DEC
                0xC6 | 0xD6 | 0xCE | 0xDE => {
                    let addr = self.get_operand_address(&opcode.mode);
                    let data = self.mem_read(addr);
                    self.mem_write(addr, data.wrapping_sub(1));
                    self.update_zero_and_negative_flags(data);
                }
                // BCC
                0x90 => {
                    self.branch(!self.status.contains(CpuFlags::CARRY));
                }
                // BCS
                0xB0 => {
                    self.branch(self.status.contains(CpuFlags::CARRY));
                }
                // BEQ
                0xF0 => {
                    self.branch(self.status.contains(CpuFlags::ZERO));
                }
                // BNE
                0xD0 => {
                    self.branch(!self.status.contains(CpuFlags::ZERO));
                }
                // BMI
                0x30 => {
                    self.branch(self.status.contains(CpuFlags::NEGATIVE));
                }
                // BPL
                0x10 => {
                    self.branch(!self.status.contains(CpuFlags::CARRY));
                }
                // BVC
                0x50 => {
                    self.branch(!self.status.contains(CpuFlags::OVERFLOW));
                }
                // BVS
                0x70 => {
                    self.branch(self.status.contains(CpuFlags::OVERFLOW));
                }
                /* JMP Absolute */
                0x4c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = mem_address;
                }
                /* JMP Indirect */
                0x6c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    // let indirect_ref = self.mem_read_u16(mem_address);
                    //6502 bug mode with with page boundary:
                    //  if address $3000 contains $40, $30FF contains $80, and $3100 contains $50,
                    // the result of JMP ($30FF) will be a transfer of control to $4080 rather than $5080 as you intended
                    // i.e. the 6502 took the low byte of the address from $30FF and the high byte from $3000

                    let indirect_ref = if mem_address & 0x00FF == 0x00FF {
                        let lo = self.mem_read(mem_address);
                        let hi = self.mem_read(mem_address & 0xFF00);
                        (hi as u16) << 8 | (lo as u16)
                    } else {
                        self.mem_read_u16(mem_address)
                    };

                    self.program_counter = indirect_ref;
                }
                 /* JSR */
                 0x20 => {
                    self.stack_push_u16(self.program_counter + 2 - 1);
                    let target_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = target_address
                }
                /* RTS */
                0x60 => {
                    self.program_counter = self.stack_pop_u16() + 1;
                }

                /* RTI */
                0x40 => {
                    let flags = self.stack_pop();
                    self.status.set(CpuFlags::CARRY, flags & CpuFlags::CARRY.bits() != 0);
                    self.status.set(CpuFlags::ZERO, flags & CpuFlags::ZERO.bits() != 0);
                    self.status.set(CpuFlags::INTERRUPT_DISABLE, flags & CpuFlags::INTERRUPT_DISABLE.bits() != 0);
                    self.status.set(CpuFlags::DECIMAL_MODE, flags & CpuFlags::DECIMAL_MODE.bits() != 0);
                    self.status.set(CpuFlags::BREAK, flags & CpuFlags::BREAK.bits() != 0);
                    self.status.set(CpuFlags::BREAK2, flags & CpuFlags::BREAK2.bits() != 0);
                    self.status.set(CpuFlags::OVERFLOW, flags & CpuFlags::OVERFLOW.bits() != 0);
                    self.status.set(CpuFlags::NEGATIVE, flags & CpuFlags::NEGATIVE.bits() != 0);
                    
                    self.status.remove(CpuFlags::BREAK);
                    self.status.insert(CpuFlags::BREAK2);

                    self.program_counter = self.stack_pop_u16();
                }
                // BRK, NOP
                0x00 | 0xEA => return,
                _ => todo!(),
            }

            if program_counter_state == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x05, 0x00]);
        assert_eq!(cpu.register_a, 0x05);
    }

    #[test]
    fn test_0xaa_tax_move_a_to_x() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x0A, 0xAA, 0x00]);
        assert_eq!(cpu.register_x, 10)
    }

    #[test]
    fn test_5_ops_working_together() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 0xc1)
    }

    #[test]
    fn test_inx_overflow() {
        let mut cpu = CPU::new();
        cpu.register_x = 0xff;
        cpu.load_and_run(vec![0xa9, 0xff, 0xaa, 0xe8, 0xe8, 0x00]);
        assert_eq!(cpu.register_x, 1)
    }

    #[test]
    fn test_ldx() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa2, 0x05, 0x00]);
        assert_eq!(cpu.register_x, 0x05);
    }

    #[test]
    fn test_ldy() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa0, 0x05, 0x00]);
        assert_eq!(cpu.register_y, 0x05);
    }

    #[test]
    fn test_sta_zero_page() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x05, 0x85, 0x00]);
        let addr = cpu.get_operand_address(&AddressingMode::ZeroPage);
        assert_eq!(cpu.mem_read(addr), 0x05);
    }

    #[test]
    fn test_stx_zero_page() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa2, 0x05, 0x86, 0x00]);
        let addr = cpu.get_operand_address(&AddressingMode::ZeroPage);
        assert_eq!(cpu.mem_read(addr), 0x05);
    }

    #[test]
    fn test_sty_zero_page() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa0, 0x05, 0x84, 0x00]);
        let addr = cpu.get_operand_address(&AddressingMode::ZeroPage);
        assert_eq!(cpu.mem_read(addr), 0x05);
    }
}
