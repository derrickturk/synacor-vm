use std::{
    error::Error,
    io::{self, Read},
    fs::File,
    path::PathBuf,
};

use synacor_vm::{
    binary,
    asm::{ImageMap, DisAsmOpts},
};

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    autolabel: bool,

    #[structopt(short, long, parse(from_os_str))]
    output_file: Option<PathBuf>,

    #[structopt(short, long, parse(from_os_str))]
    map_file: Option<PathBuf>,

    #[structopt(name="FILE", parse(from_os_str))]
    input_file: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = Options::from_args();

    let prog = {
        let mut prog = Vec::new();
        if let Some(path) = options.input_file {
            File::open(path)?.read_to_end(&mut prog)?;
        } else {
            io::stdin().read_to_end(&mut prog)?;
        }
        binary::read_binary(&prog)?
    };

    let opts = DisAsmOpts {
        autolabel: options.autolabel,
    };
    let map = ImageMap::new(&prog, &opts);

    if let Some(path) = options.output_file {
        map.disasm(&mut File::create(path)?)?;
    } else {
        map.disasm(&mut io::stdout())?;
    }

    Ok(())
}
