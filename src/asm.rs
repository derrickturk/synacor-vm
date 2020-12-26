use std::{
    error,
    fmt,
    io::{self, Write},
};

use super::vm::{
    self,
    Instruction,
    SrcOperand,
    DstOperand,
};

#[derive(Debug)]
pub enum DisAsmError {
    VmError(vm::Error),
    IOError(io::Error),
}

impl From<vm::Error> for DisAsmError {
    fn from(other: vm::Error) -> Self {
        DisAsmError::VmError(other)
    }
}

impl From<io::Error> for DisAsmError {
    fn from(other: io::Error) -> Self {
        DisAsmError::IOError(other)
    }
}

impl fmt::Display for DisAsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisAsmError::VmError(e) => write!(f, "VM error: {}", e),
            DisAsmError::IOError(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl error::Error for DisAsmError { }

#[derive(Copy, Clone, Debug)]
pub enum AsmItem {
    Instruction(Instruction),
    Value(u16),
}

#[derive(Clone, Debug)]
pub struct ImageMap {
    pub stmts: Vec<(usize, AsmItem)>,
}

/* TODO: we should probably be smarter about flagging addrs
 *   as "probably data" vs. "probably code", depending on whether
 *   they ever appear as jump targets or read/write source/sinks
 */
impl ImageMap {
    pub fn disasm(memory: &[u16]) -> ImageMap {
        let mut stmts = Vec::new();
        let mut ip = 0;

        while ip < memory.len() {
            if let Ok((new_ip, instr)) = Instruction::decode(memory, ip) {
                stmts.push((ip, AsmItem::Instruction(instr)));
                ip = new_ip;
            } else {
                stmts.push((ip, AsmItem::Value(memory[ip])));
                ip += 1;
            }
        }

        ImageMap { stmts }
    }
}
