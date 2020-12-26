use std::{
    collections::{HashMap, HashSet},
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
    pub labels: HashMap<usize, String>,
    pub origins: HashSet<usize>,
}

impl ImageMap {
    pub fn new(memory: &[u16], opts: &DisAsmOpts) -> ImageMap {
        let mut stmts = Vec::new();
        let mut labels = opts.initial_labels.as_ref()
            .map(|m| m.clone())
            .unwrap_or_else(|| HashMap::new());
        let mut origins = HashSet::new();
        let mut next_label = 0;
        let mut ip = 0;

        while ip < memory.len() {
            if let Ok((new_ip, instr)) = Instruction::decode(memory, ip) {
                stmts.push((ip, AsmItem::Instruction(instr)));

                if opts.autolabel {
                    Self::add_labels(ip, &instr,
                      &mut labels, &mut origins, &mut next_label);
                }

                ip = new_ip;
            } else {
                stmts.push((ip, AsmItem::Value(memory[ip])));
                ip += 1;
            }
        }

        ImageMap { stmts, labels, origins }
    }

    pub fn disasm<W: Write>(&self, w: &mut W, opts: &DisAsmOpts
      ) -> Result<(), DisAsmError> {
        for (ip, stmt) in &self.stmts {
            if opts.line_addrs {
                write!(w, "{}\t", ip)?;
            }
            stmt.disasm(*ip, self, w)?;
        }
        Ok(())
    }

    fn add_labels(ip: usize, instr: &Instruction,
      labels: &mut HashMap<usize, String>, origins: &mut HashSet<usize>,
      next_label: &mut usize) {
        match instr {
            Instruction::Jmp(SrcOperand::Immediate(dst)) => {
                labels.entry(*dst as usize).or_insert_with(|| {
                    let lbl = format!("lbl{}", next_label);
                    *next_label += 1;
                    lbl
                });
                origins.insert(ip + 1);
            },

            Instruction::Jt(_, SrcOperand::Immediate(dst)) => {
                labels.entry(*dst as usize).or_insert_with(|| {
                    let lbl = format!("lbl{}", next_label);
                    *next_label += 1;
                    lbl
                });
                origins.insert(ip + 2);
            },

            Instruction::Jf(_, SrcOperand::Immediate(dst)) => {
                labels.entry(*dst as usize).or_insert_with(|| {
                    let lbl = format!("lbl{}", next_label);
                    *next_label += 1;
                    lbl
                });
                origins.insert(ip + 2);
            },

            Instruction::Call(SrcOperand::Immediate(dst)) => {
                labels.entry(*dst as usize).or_insert_with(|| {
                    let lbl = format!("fn{}", next_label);
                    *next_label += 1;
                    lbl
                });
                origins.insert(ip + 1);
            },

            _ => { },
        }
    }
}

#[derive(Clone, Debug)]
pub struct DisAsmOpts {
    pub autolabel: bool,
    pub line_addrs: bool,
    pub initial_labels: Option<HashMap<usize, String>>,
}

impl Default for DisAsmOpts {
    fn default() -> Self {
        Self {
            autolabel: true,
            line_addrs: false,
            initial_labels: None,
        }
    }
}

pub trait DisAsm {
    fn disasm<W: Write>(&self, ip: usize, map: &ImageMap, w: &mut W
      ) -> Result<(), DisAsmError>;
}

impl DisAsm for Instruction {
    fn disasm<W: Write>(&self, ip: usize, map: &ImageMap, w: &mut W
      ) -> Result<(), DisAsmError> {
        match self {
            Instruction::Halt => write!(w, "halt\n")?,

            Instruction::Set(dst, src) => {
                write!(w, "set ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                src.disasm(ip + 2, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Push(src) => {
                write!(w, "push ")?;
                src.disasm(ip + 1, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Pop(dst) => {
                write!(w, "pop ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Eq(dst, lhs, rhs) => {
                write!(w, "eq ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Gt(dst, lhs, rhs) => {
                write!(w, "gt ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Jmp(dst) => {
                write!(w, "jmp ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Jt(cond, dst) => {
                write!(w, "jt ")?;
                cond.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                dst.disasm(ip + 2, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Jf(cond, dst) => {
                write!(w, "jf ")?;
                cond.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                dst.disasm(ip + 2, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Add(dst, lhs, rhs) => {
                write!(w, "add ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Mult(dst, lhs, rhs) => {
                write!(w, "mult ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Mod(dst, lhs, rhs) => {
                write!(w, "mod ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::And(dst, lhs, rhs) => {
                write!(w, "and ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Or(dst, lhs, rhs) => {
                write!(w, "or ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                lhs.disasm(ip + 2, map, w)?;
                write!(w, ", ")?;
                rhs.disasm(ip + 3, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Not(dst, src) => {
                write!(w, "not ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                src.disasm(ip + 2, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Rmem(dst, src) => {
                write!(w, "rmem ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                src.disasm(ip + 2, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Wmem(dst, src) => {
                write!(w, "wmem ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, ", ")?;
                src.disasm(ip + 2, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Call(dst) => {
                write!(w, "call ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Ret => write!(w, "ret\n")?,

            Instruction::Out(src) => {
                write!(w, "out ")?;
                src.disasm(ip + 1, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::In(dst) => {
                write!(w, "in ")?;
                dst.disasm(ip + 1, map, w)?;
                write!(w, "\n")?;
            },

            Instruction::Noop => write!(w, "noop\n")?,
        };
        Ok(())
    }
}

impl DisAsm for SrcOperand {
    fn disasm<W: Write>(&self, ip: usize, map: &ImageMap, w: &mut W
      ) -> Result<(), DisAsmError> {
        match self {
            SrcOperand::Immediate(word) => word.disasm(ip, map, w),
            SrcOperand::Register(n) => {
                write!(w, "r{}", n)?;
                Ok(())
            }
        }
    }
}

impl DisAsm for DstOperand {
    fn disasm<W: Write>(&self, _ip: usize, _map: &ImageMap, w: &mut W
      ) -> Result<(), DisAsmError> {
        match self {
            DstOperand::Register(n) => write!(w, "r{}", n)?,
        };
        Ok(())
    }
}

impl DisAsm for AsmItem {
    fn disasm<W: Write>(&self, ip: usize, map: &ImageMap, w: &mut W
      ) -> Result<(), DisAsmError> {
        if let Some(label) = map.labels.get(&ip) {
            write!(w, "{}: ", label)?;
        }
        match *self {
            AsmItem::Instruction(instr) => instr.disasm(ip, map, w),
            AsmItem::Value(word) => {
                word.disasm(ip, map, w)?;
                write!(w, "\n")?;
                Ok(())
            },
        }
    }
}

impl DisAsm for u16 {
    fn disasm<W: Write>(&self, ip: usize, map: &ImageMap, w: &mut W
      ) -> Result<(), DisAsmError> {
        if map.origins.contains(&ip) {
            if let Some(lbl) = map.labels.get(&(*self as usize)) {
                write!(w, "{}", lbl)?;
                return Ok(());
            }
        }

        let word_u8 = *self as u8;
        if *self & vm::VALID_IO_MASK == 0
          && word_u8.is_ascii() && !word_u8.is_ascii_control() {
            write!(w, "'{}'", word_u8 as char)?;
        } else {
            write!(w, "{}", *self)?;
        }
        Ok(())
    }
}
