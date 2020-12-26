use std::{
    error::Error,
    io::{self, Read, Write},
    fs::File,
    path::PathBuf,
};

use synacor_vm::{
    binary,
    vm::{self, Vm, Instruction},
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
}

fn run_tracing<R: Read>(vm: &mut Vm, r: &mut R) -> Result<(), Box<dyn Error>> {
    let mut out_buf: Vec<u8> = Vec::new();

    loop {
        let (_, instr) = vm.decode_next()?;
        match instr {
            Instruction::In(_) => {
                println!("ip {}{}{} / regs {}{:?}{} / stack# {}{}{}",
                  BEGIN_BLUE, vm.ip(), CLEAR_COLOR,
                  BEGIN_BLUE, vm.registers(), CLEAR_COLOR,
                  BEGIN_BLUE, vm.stack().len(), CLEAR_COLOR);
                print!("{}input> {}", BEGIN_YELLOW, CLEAR_COLOR);
                io::stdout().flush()?;
            }

            _ => { },
        };

        match vm.step(r, &mut out_buf)? {
            vm::VmState::Halted => return Ok(()),
            vm::VmState::Running => { },
        };

        match out_buf.last() {
            Some(b'\n') =>  {
                print!("{}output> {}", BEGIN_GREEN, CLEAR_COLOR);
                io::stdout().write_all(&out_buf)?;
                out_buf.clear();
            },

            _ => { },
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

    if let Some(path) = options.initial_input {
        run_tracing(&mut vm,
          &mut File::open(path)?.chain(io::stdin()))?;
    } else {
        run_tracing(&mut vm, &mut io::stdin())?;
    }

    Ok(())
}
