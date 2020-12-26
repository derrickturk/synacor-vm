use std::{
    error::Error,
    io::{self, Read, Write},
    fs::File,
    path::PathBuf,
};

use synacor_vm::{
    binary,
    vm::{self, Vm},
};

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(name="FILE", parse(from_os_str))]
    image_file: PathBuf,

    #[structopt(short, long, parse(from_os_str))]
    initial_input: Option<PathBuf>,
}

fn run_tracing<R: Read, W: Write>(vm: &mut Vm, r: &mut R, w: &mut W
  ) -> Result<(), vm::Error> {
    loop {
        // println!("{}", vm.ip());
        match vm.step(r, w)? {
            vm::VmState::Halted => return Ok(()),
            vm::VmState::Running => { },
        };
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::from_args();

    let prog = {
        let mut prog = Vec::new();
        File::open(options.image_file)?.read_to_end(&mut prog)?;
        binary::read_binary(&prog)?
    };

    let mut vm = Vm::new();
    vm.load(&prog)?;

    if let Some(path) = options.initial_input {
        run_tracing(&mut vm,
          &mut File::open(path)?.chain(io::stdin()), &mut io::stdout())?;
    } else {
        run_tracing(&mut vm, &mut io::stdin(), &mut io::stdout())?;
    }

    Ok(())
}
