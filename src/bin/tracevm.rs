use std::{
    collections::HashMap,
    error::Error,
    io::{self, BufReader, BufRead, Cursor, Read, Write},
    fs::File,
    path::PathBuf,
};

use synacor_vm::{
    binary,
    vm::{self, Vm, Instruction},
    asm::{Labels, read_labels},
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
    #[structopt(name="FILE", parse(from_os_str))]
    image_file: PathBuf,

    #[structopt(short, long, parse(from_os_str))]
    initial_input: Option<PathBuf>,

    #[structopt(short, long, parse(from_os_str))]
    map_file: Option<PathBuf>,
}

struct Tracer {
    vm: Vm,
    labels: Labels,
    in_cursor: Cursor<Vec<u8>>,
    out_buf: Vec<u8>,
}

impl Tracer {
    fn new(vm: Vm, labels: Option<Labels>, initial_input: Option<Vec<u8>>
      ) -> Self {
        Self {
            vm,
            labels: labels.unwrap_or_else(|| HashMap::new()),
            in_cursor: Cursor::new(initial_input.unwrap_or_else(|| Vec::new())),
            out_buf: Vec::new(),
        }
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
    }

    fn ensure_input(&mut self) -> Result<(), Box<dyn Error>> {
        if self.in_cursor.position() == self.in_cursor.get_ref().len() as u64 {
            self.in_cursor.set_position(0);
            self.in_cursor.get_mut().clear();
            self.status_line();
            print!("{}input> {}", BEGIN_YELLOW, CLEAR_COLOR);
            io::stdout().flush()?;
            let stdin = io::stdin();
            stdin.lock().read_until(b'\n', self.in_cursor.get_mut())?;
        }
        Ok(())
    }

    fn pump_output(&mut self) -> Result<(), Box<dyn Error>> {
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

    fn run_tracing(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            let (_, instr) = self.vm.decode_next()?;
            match instr {
                Instruction::In(_) => self.ensure_input()?,

                Instruction::Halt => {
                    self.status_line();
                    println!("{}HALT{}", BEGIN_RED, CLEAR_COLOR);
                },

                _ => { },
            };

            match self.vm.step(&mut self.in_cursor, &mut self.out_buf)? {
                vm::VmState::Halted => return Ok(()),
                vm::VmState::Running => { },
            };

            self.pump_output()?;
        }
    }
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

fn main() -> Result<(), Box<dyn Error>> {
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

    let mut tracer = Tracer::new(vm, initial_labels, initial_input);
    tracer.run_tracing()?;

    Ok(())
}
