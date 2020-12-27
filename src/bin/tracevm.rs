use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    fs::File,
    io::{self, BufReader, BufRead, Cursor, Read, Write},
    path::PathBuf,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
};

use synacor_vm::{
    binary,
    vm::{self, Vm, VmState, Instruction},
    asm::{
        AsmError,
        DisAsm,
        DisAsmOpts,
        DisAsmError,
        ImageMap,
        Labels,
        read_labels,
    },
};

use structopt::StructOpt;

#[cfg(windows)]
use winapi::um::{
    handleapi::INVALID_HANDLE_VALUE,
    winbase::{STD_OUTPUT_HANDLE},
    wincon::{ENABLE_VIRTUAL_TERMINAL_PROCESSING},
    processenv::GetStdHandle,
    consoleapi::{GetConsoleMode, SetConsoleMode},
};

const BEGIN_RED: &'static str = "\u{1b}[1;31m";
const BEGIN_GREEN: &'static str = "\u{1b}[1;32m";
const BEGIN_BLUE: &'static str = "\u{1b}[1;34m";
const BEGIN_YELLOW: &'static str = "\u{1b}[1;33m";
const CLEAR_COLOR: &'static str = "\u{1b}[0m";

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    autolabel: bool,

    #[structopt(name="FILE", parse(from_os_str))]
    image_file: PathBuf,

    #[structopt(short, long, parse(from_os_str))]
    initial_input: Option<PathBuf>,

    #[structopt(short, long, parse(from_os_str))]
    map_file: Option<PathBuf>,
}

#[derive(Debug)]
pub enum TracerError {
    VmError(vm::Error),
    IOError(io::Error),
    AsmError(AsmError),
    DisAsmError(DisAsmError),
    UnknownCommand(String),
    UnknownLabel(String),
    UnknownRegister(String),
}

impl From<vm::Error> for TracerError {
    fn from(other: vm::Error) -> Self {
        TracerError::VmError(other)
    }
}

impl From<io::Error> for TracerError {
    fn from(other: io::Error) -> Self {
        TracerError::IOError(other)
    }
}

impl From<AsmError> for TracerError {
    fn from(other: AsmError) -> Self {
        TracerError::AsmError(other)
    }
}

impl From<DisAsmError> for TracerError {
    fn from(other: DisAsmError) -> Self {
        TracerError::DisAsmError(other)
    }
}

impl fmt::Display for TracerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TracerError::VmError(e) => write!(f, "VM error: {}", e),
            TracerError::IOError(e) => write!(f, "I/O error: {}", e),
            TracerError::AsmError(e) => write!(f, "assembly error: {}", e),
            TracerError::DisAsmError(e) =>
              write!(f, "disassembly error: {}", e),
            TracerError::UnknownCommand(line) =>
              write!(f, "unknown command: \"{}\"", line),
            TracerError::UnknownLabel(lbl) =>
              write!(f, "unknown label: \"{}\"", lbl),
            TracerError::UnknownRegister(reg) =>
              write!(f, "unknown register: \"{}\"", reg),
        }
    }
}

impl Error for TracerError { }

pub struct Tracer {
    vm: Vm,
    labels: Labels,
    in_cursor: Cursor<Vec<u8>>,
    out_buf: Vec<u8>,
    breakpoints: HashSet<usize>,
    map: ImageMap,
    autolabel: bool,
    interrupt: Arc<AtomicBool>,
}

impl Tracer {
    pub fn new(vm: Vm, labels: Option<Labels>, initial_input: Option<Vec<u8>>,
      autolabel: bool) -> Self {
        let map = ImageMap::new(vm.memory(), &DisAsmOpts {
            autolabel,
            line_addrs: false,
            initial_labels: labels.clone(),
        });

        Self {
            vm,
            labels: labels.unwrap_or_else(|| HashMap::new()),
            in_cursor: Cursor::new(initial_input.unwrap_or_else(|| Vec::new())),
            out_buf: Vec::new(),
            breakpoints: HashSet::new(),
            map,
            autolabel,
            interrupt: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(&mut self) -> Result<(), TracerError> {
        loop {
            self.status_line();
            let cmd = self.get_command();
            match cmd {
                Ok(cmd) => {
                    match self.do_cmd(cmd)? {
                        TracerState::WaitCommand => { },

                        TracerState::Quit => {
                            println!("{}bye!{}", BEGIN_YELLOW, CLEAR_COLOR);
                            return Ok(());
                        },
                    };
                },

                Err(e) => {
                    println!("{}{}{}", BEGIN_RED, e, CLEAR_COLOR);
                },
            }
        }
    }

    pub fn register_sigint(&self) -> Result<(), TracerError> {
        signal_hook::flag::register(signal_hook::consts::signal::SIGINT,
          Arc::clone(&self.interrupt))?;
        Ok(())
    }

    fn status_line(&self) {
        if let Some(lbl) = self.labels.get(&self.vm.ip()) {
            println!("ip {}{} = {}{} / regs {}{:?}{} / stack# {}{}{}",
              BEGIN_BLUE, lbl, self.vm.ip(), CLEAR_COLOR,
              BEGIN_BLUE, self.vm.registers(), CLEAR_COLOR,
              BEGIN_BLUE, self.vm.stack().len(), CLEAR_COLOR);
        } else {
            println!("ip {}{}{} / regs {}{:?}{} / stack# {}{}{}",
              BEGIN_BLUE, self.vm.ip(), CLEAR_COLOR,
              BEGIN_BLUE, self.vm.registers(), CLEAR_COLOR,
              BEGIN_BLUE, self.vm.stack().len(), CLEAR_COLOR);
        }

        match self.vm.decode_next() {
            Ok((_, instr)) => {
                match instr.disasm(self.vm.ip(), &self.map, &mut io::stdout()) {
                    Ok(_) => { },
                    Err(e) => println!("{}disassembly error: {}{}",
                      BEGIN_RED, e, CLEAR_COLOR),
                }
            },

            Err(e) => println!("{}disassembly error: {}{}",
              BEGIN_RED, e, CLEAR_COLOR),
        }
    }

    fn remap(&mut self) {
        self.map = ImageMap::new(self.vm.memory(), &DisAsmOpts {
            autolabel: self.autolabel,
            line_addrs: false,
            initial_labels: Some(self.labels.clone()),
        });
    }

    fn ensure_input(&mut self, single_step: bool) -> Result<bool, TracerError> {
        if self.in_cursor.position() == self.in_cursor.get_ref().len() as u64 {
            if single_step {
                self.status_line();
            }
            self.in_cursor.set_position(0);
            self.in_cursor.get_mut().clear();
            print!("{}input> {}", BEGIN_YELLOW, CLEAR_COLOR);
            io::stdout().flush()?;
            let stdin = io::stdin();
            stdin.lock().read_until(b'\n', self.in_cursor.get_mut())?;

            if self.interrupt.swap(false, Ordering::Relaxed) {
                return Ok(false);
            }
        }

        // we can also just... get... nothing...
        Ok(self.in_cursor.position() < self.in_cursor.get_ref().len() as u64)
    }

    fn pump_output(&mut self) -> Result<(), TracerError> {
        match self.out_buf.last() {
            Some(b'\n') =>  {
                print!("{}output> {}", BEGIN_GREEN, CLEAR_COLOR);
                io::stdout().write_all(&mut self.out_buf)?;
                self.out_buf.clear();
            },
            _ => { },
        };
        Ok(())
    }

    fn step(&mut self, single_step: bool) -> Result<VmState, TracerError> {
        let (_, instr) = self.vm.decode_next()?;
        match instr {
            Instruction::In(_) => {
                if !self.ensure_input(single_step)? {
                    // an interrupt happened, don't step
                    return Ok(VmState::Running)
                }
            },

            Instruction::Halt => {
                if !single_step {
                    self.status_line();
                }
                println!("{}HALT{}", BEGIN_RED, CLEAR_COLOR);
            },

            _ => { },
        };

        let state = self.vm.step(&mut self.in_cursor, &mut self.out_buf)?;
        self.pump_output()?;
        Ok(state)
    }

    fn do_cmd(&mut self, command: TracerCommand
      ) -> Result<TracerState, TracerError> {
        let state = match command {
            TracerCommand::Step => {
                self.step(true)?;
                TracerState::WaitCommand
            },

            TracerCommand::Continue(til) => {
                let til = til.unwrap_or(usize::MAX);
                while self.vm.ip() < til {
                    if self.interrupt.swap(false, Ordering::Relaxed) {
                        return Ok(TracerState::WaitCommand);
                    }

                    match self.step(false)? {
                        VmState::Halted => break,
                        _ => { },
                    };

                    if self.breakpoints.contains(&self.vm.ip()) {
                        return Ok(TracerState::WaitCommand);
                    }
                }
                TracerState::WaitCommand
            },

            TracerCommand::SetLabel(ptr, label) => {
                self.labels.insert(ptr, label);
                TracerState::WaitCommand
            },

            TracerCommand::ClearLabel(label) => {
                let ptr = self.labels.iter()
                    .find(|(_, v)| v.as_str() == label)
                    .map(|(ptr, _)| *ptr);
                match ptr {
                    Some(ptr) => { self.labels.remove(&ptr); },
                    None => { },
                };
                TracerState::WaitCommand
            },

            TracerCommand::SetBreakpoint(ip) => {
                self.breakpoints.insert(ip);
                TracerState::WaitCommand
            },

            TracerCommand::ClearBreakpoint(ip) => {
                self.breakpoints.remove(&ip);
                TracerState::WaitCommand
            },

            TracerCommand::Push(val) => {
                self.vm.push_stack(val);
                TracerState::WaitCommand
            },

            TracerCommand::Pop => {
                match self.vm.pop_stack() {
                    Some(val) => println!("{}", val),
                    None => println!("{}empty stack{}", BEGIN_RED, CLEAR_COLOR),
                };
                TracerState::WaitCommand
            },

            TracerCommand::Poke(ptr, val) => {
                match self.vm.memory_mut().get_mut(ptr) {
                    Some(target) => *target = val,
                    None => println!("{}invalid address{}",
                      BEGIN_RED, CLEAR_COLOR),
                };
                TracerState::WaitCommand
            },

            TracerCommand::SetReg(reg, val) => {
                self.vm.registers_mut()[reg] = val;
                TracerState::WaitCommand
            },

            TracerCommand::Status => {
                self.status_line();
                TracerState::WaitCommand
            },

            TracerCommand::Remap => {
                self.remap();
                TracerState::WaitCommand
            },

            TracerCommand::Help => {
                println!("{}syntrace - tracer commands:", BEGIN_YELLOW);
                println!("  (s)tep");
                println!("  (l)abel <ptr> <lbl>");
                println!("  (u)nlabel <ptr>");
                println!("  clea(r) <breakpoint>");
                println!("  (b)reak <ptr>");
                println!("  (c)ontinue <ptr>");
                println!("  push <val>");
                println!("  pop");
                println!("  poke <ptr> <val>");
                println!("  se(t) [r0-r7] <val>");
                println!("  st(a)tus");
                println!("  re(m)ap");
                println!("  (h)elp");
                println!("  (q)uit{}", CLEAR_COLOR);
                println!();
                TracerState::WaitCommand
            },

            TracerCommand::Quit => TracerState::Quit,
        };
        Ok(state)
    }

    fn get_command(&self) -> Result<TracerCommand, TracerError> {
        let mut line = String::new();
        print!("ictrace> ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut line)?;

        let cmd = line.trim();
        if cmd.is_empty() {
            return Ok(TracerCommand::Step);
        }

        let mut cmd_words = cmd.split_whitespace();
        let cmd_word = cmd_words.next()
          .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
        let res = match cmd_word {
            "s" | "step" => {
                TracerCommand::Step
            },

            "c" | "continue" => {
                let ptr = cmd_words.next()
                    .map(|ptr| self.ptr_or_label(ptr))
                    .transpose()?;
                TracerCommand::Continue(ptr)
            },

            "l" | "label" | "lbl" => {
                let ptr = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                let ptr = self.ptr_or_label(ptr)?;
                let lbl = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                TracerCommand::SetLabel(ptr, lbl.to_string())
            },

            "u" | "unlabel" => {
                let label = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                TracerCommand::ClearLabel(label.to_string())
            },

            "b" | "break" => {
                let ptr = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                let ptr = self.ptr_or_label(ptr)?;
                TracerCommand::SetBreakpoint(ptr)
            },

            "r" | "clear" => {
                let ptr = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                let ptr = self.ptr_or_label(ptr)?;
                TracerCommand::ClearBreakpoint(ptr)
            },

            "push" => {
                let val = cmd_words.next()
                  .and_then(|v| v.parse::<u16>().ok())
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                TracerCommand::Push(val)
            },

            "pop" => TracerCommand::Pop,

            "poke" => {
                let ptr = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                let ptr = self.ptr_or_label(ptr)?;
                let val = cmd_words.next()
                  .and_then(|v| v.parse::<u16>().ok())
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                TracerCommand::Poke(ptr, val)
            },

            "t" | "set" => {
                let reg = cmd_words.next()
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                let reg = match reg {
                    "r0" => 0,
                    "r1" => 1,
                    "r2" => 2,
                    "r3" => 3,
                    "r4" => 4,
                    "r5" => 5,
                    "r6" => 6,
                    "r7" => 7,
                    _ => return Err(
                      TracerError::UnknownRegister(reg.to_string())),
                };
                let val = cmd_words.next()
                  .and_then(|v| v.parse::<u16>().ok())
                  .ok_or_else(|| TracerError::UnknownCommand(cmd.to_string()))?;
                TracerCommand::SetReg(reg, val)
            },

            "a" | "status" => TracerCommand::Status,

            "m" | "remap" => TracerCommand::Remap,

            "h" | "help" => TracerCommand::Help,

            "q" | "quit" => TracerCommand::Quit,

            _ => return Err(TracerError::UnknownCommand(cmd.to_string())),
        };

        Ok(res)
    }

    #[inline]
    fn ptr_or_label(&self, input: &str) -> Result<usize, TracerError> {
        if let Ok(ptr) = input.parse::<usize>() {
            return Ok(ptr);
        }
        if let Some(ptr) = self.labels.iter()
              .find(|(_, v)| v.as_str() == input).map(|(k, _)| *k) {
            Ok(ptr)
        } else {
            Err(TracerError::UnknownLabel(input.to_string()))
        }
    }
}

#[derive(Clone, Debug)]
pub enum TracerCommand {
    Step,
    Continue(Option<usize>),
    SetLabel(usize, String),
    ClearLabel(String),
    SetBreakpoint(usize),
    ClearBreakpoint(usize),
    Push(u16),
    Pop,
    Poke(usize, u16),
    SetReg(usize, u16),
    Status,
    Remap,
    Help,
    Quit,
}

#[derive(Copy, Clone, Debug)]
pub enum TracerState {
    WaitCommand,
    Quit,
}

#[cfg(windows)]
fn set_ansi_console() {
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if out == INVALID_HANDLE_VALUE {
            return;
        }

        let mut mode = 0;
        if GetConsoleMode(out, &mut mode) == 0 {
            return;
        }

        SetConsoleMode(out, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
    }
}

fn main() -> Result<(), TracerError> {
    if cfg!(windows) {
        set_ansi_console();
    }

    let options = Options::from_args();

    let prog = {
        let mut prog = Vec::new();
        File::open(options.image_file)?.read_to_end(&mut prog)?;
        binary::read_binary(&prog)?
    };

    let mut vm = Vm::new();
    vm.load(&prog)?;

    println!(
      "WELCOME TO {}H E L L{}, please leave your {}little{} {}dog{} outside",
      BEGIN_RED, CLEAR_COLOR, BEGIN_YELLOW, CLEAR_COLOR,
      BEGIN_BLUE, CLEAR_COLOR);

    let initial_labels = {
        if let Some(path) = options.map_file {
            let map_file = File::open(path)?;
            let mut map_file = BufReader::new(map_file);
            Some(read_labels(&mut map_file)?)
        } else {
            None
        }
    };

    let initial_input = if let Some(path) = options.initial_input {
        let mut input = Vec::new();
        File::open(path)?.read_to_end(&mut input)?;
        Some(input)
    } else {
        None
    };

    let mut tracer = Tracer::new(vm, initial_labels, initial_input,
      options.autolabel);
    tracer.register_sigint()?;
    tracer.run()?;

    Ok(())
}
