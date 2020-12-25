use std::{
    error,
    fmt,
};

#[derive(Copy, Clone, Debug)]
pub enum Error {
    BadBinary,
    ProgramTooLarge(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadBinary => write!(f, "bad binary format"),
            Error::ProgramTooLarge(n) =>
              write!(f, "program too large ({} words)", n),
        }
    }
}

impl error::Error for Error { }

pub type Result<T> = std::result::Result<T, Error>;

pub struct Vm {
    memory: [u16; 32768],
    registers: [u16; 8],
    stack: Vec<u16>,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            memory: [0; 32768],
            registers: [0; 8],
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
}
