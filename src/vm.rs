use std::{
    error,
    fmt,
    io::{Read, Write},
};

#[derive(Debug, Copy, Clone)]
pub enum Error {
    BadBinary,
    ProgramTooLarge(usize),
    StackUnderflow,
    InvalidSrcOperand(u16),
    InvalidDstOperand(u16),
    InvalidIp(usize),
    IllegalInstruction(u16),
    InvalidIOWord(u16),
    IOError,
    InvalidAddress(u16),
}

pub const INDIRECT_BIT: u16 = 0b1000000000000000;
pub const VALID_REGISTER_MASK: u16 = 0b0111111111111000;
pub const REGISTER_MASK: u16 = 0b0000000000000111;
pub const VALID_IO_MASK: u16 = 0b1111111100000000;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadBinary => write!(f, "bad binary format"),
            Error::ProgramTooLarge(n) =>
              write!(f, "program too large ({} words)", n),
            Error::StackUnderflow => write!(f, "pop from empty stack"),
            Error::InvalidSrcOperand(w) =>
              write!(f, "invalid source operand word ({})", w),
            Error::InvalidDstOperand(w) =>
              write!(f, "invalid destination operand word ({})", w),
            Error::InvalidIp(ip) =>
              write!(f, "invalid instruction pointer ({})", ip),
            Error::IllegalInstruction(w) =>
              write!(f, "illegal instruction ({})", w),
            Error::InvalidIOWord(w) =>
              write!(f, "invalid I/O word ({})", w),
            Error::IOError => write!(f, "I/O error"),
            Error::InvalidAddress(w) =>
              write!(f, "invalid memory address ({})", w),
        }
    }
}

impl error::Error for Error { }

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone)]
pub enum VmState {
    Running,
    Halted,
}

#[derive(Debug, Clone)]
pub struct Vm {
    memory: [u16; 32768],
    registers: [u16; 8],
    ip: usize,
    stack: Vec<u16>,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            memory: [0; 32768],
            registers: [0; 8],
            ip: 0,
            stack: Vec::new(),
        }
    }

    pub fn load(&mut self, program: &[u16]) -> Result<()> {
        if program.len() > self.memory.len() {
            return Err(Error::ProgramTooLarge(program.len()));
        }

        self.memory = [0; 32768];
        self.memory[0..program.len()].copy_from_slice(program);
        Ok(())
    }

    #[inline]
    pub fn run<R: Read, W: Write>(&mut self, read: &mut R, write: &mut W
      ) -> Result<()> {
        loop {
            match self.step(read, write)? {
                VmState::Halted => return Ok(()),
                _ => { }
            };
        }
    }

    pub fn step<R: Read, W: Write>(&mut self, read: &mut R, write: &mut W
      ) -> Result<VmState> {
        let (mut new_ip, instr) = self.decode_next()?;
        match instr {
            Instruction::Halt => return Ok(VmState::Halted),

            Instruction::Set(dst, src) =>
              self.write_dst(&dst, self.read_src(&src)),

            Instruction::Push(src) => self.stack.push(self.read_src(&src)),

            Instruction::Pop(dst) => match self.stack.pop() {
                Some(val) => self.write_dst(&dst, val),
                None => return Err(Error::StackUnderflow),
            },

            Instruction::Eq(dst, lhs, rhs) =>
              self.write_dst(&dst,
                (self.read_src(&lhs) == self.read_src(&rhs)) as u16),

            Instruction::Gt(dst, lhs, rhs) =>
              self.write_dst(&dst,
                (self.read_src(&lhs) > self.read_src(&rhs)) as u16),

            Instruction::Jmp(ip) => new_ip = self.read_src(&ip) as usize,

            Instruction::Jt(cond, ip) => if self.read_src(&cond) != 0 {
                new_ip = self.read_src(&ip) as usize;
            },

            Instruction::Jf(cond, ip) => if self.read_src(&cond) == 0 {
                new_ip = self.read_src(&ip) as usize;
            },

            Instruction::Add(dst, lhs, rhs) =>
              self.write_dst(&dst,
                self.read_src(&lhs).wrapping_add(self.read_src(&rhs))
                & !INDIRECT_BIT),

            Instruction::Mult(dst, lhs, rhs) =>
              self.write_dst(&dst,
                self.read_src(&lhs).wrapping_mul(self.read_src(&rhs))
                & !INDIRECT_BIT),

            Instruction::Mod(dst, lhs, rhs) =>
              self.write_dst(&dst, self.read_src(&lhs) % self.read_src(&rhs)),

            Instruction::And(dst, lhs, rhs) =>
              self.write_dst(&dst, self.read_src(&lhs) & self.read_src(&rhs)),

            Instruction::Or(dst, lhs, rhs) =>
              self.write_dst(&dst, self.read_src(&lhs) | self.read_src(&rhs)),

            Instruction::Not(dst, src) =>
              self.write_dst(&dst, !self.read_src(&src) & !INDIRECT_BIT),

            Instruction::Rmem(dst, src_addr) =>
              self.write_dst(&dst,
                self.read_indirect(self.read_src(&src_addr))?),

            Instruction::Wmem(dst_addr, src_addr) =>
              self.write_indirect(
                self.read_src(&dst_addr), self.read_src(&src_addr))?,

            Instruction::Call(ip) => {
                self.stack.push(new_ip as u16);
                new_ip = self.read_src(&ip) as usize;
            },

            Instruction::Ret => {
                match self.stack.pop() {
                    Some(ip) => new_ip = ip as usize,
                    None => return Ok(VmState::Halted),
                };
            },

            Instruction::Out(src) => {
                let byte = self.read_src(&src);
                if byte & VALID_IO_MASK != 0 {
                    return Err(Error::InvalidIOWord(byte));
                }
                let byte = byte as u8;
                write.write_all(&[byte]).map_err(|_| Error::IOError)?;
            },

            Instruction::In(dst) => {
                let mut buf = [0u8];
                loop {
                    read.read_exact(&mut buf[..]).map_err(|_| Error::IOError)?;
                    if buf[0] != b'\r' {
                        break;
                    }
                }
                self.write_dst(&dst, buf[0] as u16);
            },

            Instruction::Noop => { },
        };

        self.ip = new_ip;
        Ok(VmState::Running)
    }

    #[inline]
    pub fn memory(&self) -> &[u16; 32768] {
        &self.memory
    }

    #[inline]
    pub fn registers(&self) -> &[u16; 8] {
        &self.registers
    }

    #[inline]
    pub fn ip(&self) -> usize {
        self.ip
    }

    #[inline]
    pub fn stack(&self) -> &[u16] {
        &self.stack
    }

    #[inline]
    pub fn decode(&self, addr: usize) -> Result<(usize, Instruction)> {
        Instruction::decode(&self.memory[..], addr)
    }

    #[inline]
    pub fn decode_next(&self) -> Result<(usize, Instruction)> {
        self.decode(self.ip)
    }

    #[inline]
    pub fn memory_mut(&mut self) -> &mut [u16; 32768] {
        &mut self.memory
    }

    #[inline]
    pub fn registers_mut(&mut self) -> &mut [u16; 8] {
        &mut self.registers
    }

    #[inline]
    pub fn jump_to(&mut self, ip: usize) {
        self.ip = ip;
    }

    #[inline]
    pub fn push_stack(&mut self, val: u16) {
        self.stack.push(val)
    }

    #[inline]
    pub fn pop_stack(&mut self) -> Option<u16> {
        self.stack.pop()
    }

    #[inline]
    fn read_src(&self, operand: &SrcOperand) -> u16 {
        match *operand {
            SrcOperand::Immediate(val) => val,
            SrcOperand::Register(reg) => self.registers[reg],
        }
    }

    #[inline]
    fn write_dst(&mut self, operand: &DstOperand, word: u16) {
        match *operand {
            DstOperand::Register(reg) => self.registers[reg] = word,
        };
    }

    #[inline]
    fn read_indirect(&self, ptr: u16) -> Result<u16> {
        Ok(*self.memory.get(ptr as usize).ok_or(Error::InvalidAddress(ptr))?)
    }

    #[inline]
    fn write_indirect(&mut self, ptr: u16, word: u16) -> Result<()> {
        *self.memory.get_mut(ptr as usize)
          .ok_or(Error::InvalidAddress(ptr))? = word;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SrcOperand {
    Immediate(u16),
    Register(usize),
}

impl SrcOperand {
    pub fn decode(word: u16) -> Result<Self> {
        if word & INDIRECT_BIT != 0 {
            if word & VALID_REGISTER_MASK != 0 {
                Err(Error::InvalidSrcOperand(word))
            } else {
                Ok(SrcOperand::Register((word & REGISTER_MASK) as usize))
            }
        } else {
            Ok(SrcOperand::Immediate(word))
        }
    }

    #[inline]
    pub fn decode_at(memory: &[u16], ip: usize) -> Result<Self> {
        Self::decode(*memory.get(ip).ok_or(Error::InvalidIp(ip))?)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum DstOperand {
    Register(usize)
}

impl DstOperand {
    pub fn decode(word: u16) -> Result<Self> {
        if word & INDIRECT_BIT != 0 {
            if word & VALID_REGISTER_MASK != 0 {
                Err(Error::InvalidDstOperand(word))
            } else {
                Ok(DstOperand::Register((word & REGISTER_MASK) as usize))
            }
        } else {
            Err(Error::InvalidDstOperand(word))
        }
    }

    #[inline]
    pub fn decode_at(memory: &[u16], ip: usize) -> Result<Self> {
        Self::decode(*memory.get(ip).ok_or(Error::InvalidIp(ip))?)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Instruction {
    Halt,
    Set(DstOperand, SrcOperand),
    Push(SrcOperand),
    Pop(DstOperand),
    Eq(DstOperand, SrcOperand, SrcOperand),
    Gt(DstOperand, SrcOperand, SrcOperand),
    Jmp(SrcOperand),
    Jt(SrcOperand, SrcOperand),
    Jf(SrcOperand, SrcOperand),
    Add(DstOperand, SrcOperand, SrcOperand),
    Mult(DstOperand, SrcOperand, SrcOperand),
    Mod(DstOperand, SrcOperand, SrcOperand),
    And(DstOperand, SrcOperand, SrcOperand),
    Or(DstOperand, SrcOperand, SrcOperand),
    Not(DstOperand, SrcOperand),
    Rmem(DstOperand, SrcOperand),
    Wmem(SrcOperand, SrcOperand),
    Call(SrcOperand),
    Ret,
    Out(SrcOperand),
    In(DstOperand),
    Noop,
}

impl Instruction {
    pub fn decode(memory: &[u16], ip: usize) -> Result<(usize, Instruction)> {
        match *memory.get(ip).ok_or(Error::InvalidIp(ip))? {
            0 => Ok((ip + 1, Instruction::Halt)),
            1 => Ok((ip + 3, Instruction::Set(
                   DstOperand::decode_at(memory, ip + 1)?,
                   SrcOperand::decode_at(memory, ip + 2)?
                 ))),
            2 => Ok((ip + 2, Instruction::Push(
                   SrcOperand::decode_at(memory, ip + 1)?
                 ))),
            3 => Ok((ip + 2, Instruction::Pop(
                   DstOperand::decode_at(memory, ip + 1)?
                 ))),
            4 => Ok((ip + 4, Instruction::Eq(
                   DstOperand::decode_at(memory, ip + 1)?,
                   SrcOperand::decode_at(memory, ip + 2)?,
                   SrcOperand::decode_at(memory, ip + 3)?
                 ))),
            5 => Ok((ip + 4, Instruction::Gt(
                   DstOperand::decode_at(memory, ip + 1)?,
                   SrcOperand::decode_at(memory, ip + 2)?,
                   SrcOperand::decode_at(memory, ip + 3)?
                 ))),
            6 => Ok((ip + 2, Instruction::Jmp(
                   SrcOperand::decode_at(memory, ip + 1)?
                 ))),
            7 => Ok((ip + 3, Instruction::Jt(
                   SrcOperand::decode_at(memory, ip + 1)?,
                   SrcOperand::decode_at(memory, ip + 2)?
                 ))),
            8 => Ok((ip + 3, Instruction::Jf(
                   SrcOperand::decode_at(memory, ip + 1)?,
                   SrcOperand::decode_at(memory, ip + 2)?
                 ))),
            9 => Ok((ip + 4, Instruction::Add(
                   DstOperand::decode_at(memory, ip + 1)?,
                   SrcOperand::decode_at(memory, ip + 2)?,
                   SrcOperand::decode_at(memory, ip + 3)?
                 ))),
            10 => Ok((ip + 4, Instruction::Mult(
                    DstOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?,
                    SrcOperand::decode_at(memory, ip + 3)?
                  ))),
            11 => Ok((ip + 4, Instruction::Mod(
                    DstOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?,
                    SrcOperand::decode_at(memory, ip + 3)?
                  ))),
            12 => Ok((ip + 4, Instruction::And(
                    DstOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?,
                    SrcOperand::decode_at(memory, ip + 3)?
                  ))),
            13 => Ok((ip + 4, Instruction::Or(
                    DstOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?,
                    SrcOperand::decode_at(memory, ip + 3)?
                  ))),
            14 => Ok((ip + 3, Instruction::Not(
                    DstOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?
                  ))),
            15 => Ok((ip + 3, Instruction::Rmem(
                    DstOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?
                  ))),
            16 => Ok((ip + 3, Instruction::Wmem(
                    SrcOperand::decode_at(memory, ip + 1)?,
                    SrcOperand::decode_at(memory, ip + 2)?
                  ))),
            17 => Ok((ip + 2, Instruction::Call(
                    SrcOperand::decode_at(memory, ip + 1)?
                  ))),
            18 => Ok((ip + 1, Instruction::Ret)),
            19 => Ok((ip + 2, Instruction::Out(
                    SrcOperand::decode_at(memory, ip + 1)?
                  ))),
            20 => Ok((ip + 2, Instruction::In(
                    DstOperand::decode_at(memory, ip + 1)?
                  ))),
            21 => Ok((ip + 1, Instruction::Noop)),
            word => Err(Error::IllegalInstruction(word)),
        }
    }
}
