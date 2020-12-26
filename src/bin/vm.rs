use std::{
    error::Error,
    io::{self, Read},
    fs::File,
    path::PathBuf,
};

use synacor_vm::{
    binary,
    vm::Vm,
};

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(name="FILE", parse(from_os_str))]
    image_file: PathBuf,

    #[structopt(short, long, parse(from_os_str))]
    initial_input: Option<PathBuf>,
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
        vm.run(&mut File::open(path)?.chain(io::stdin()), &mut io::stdout())?;
    } else {
        vm.run(&mut io::stdin(), &mut io::stdout())?;
    }

    Ok(())
}
