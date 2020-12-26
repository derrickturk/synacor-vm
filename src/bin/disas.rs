#![feature(str_split_once)]

use std::{
    collections::HashMap,
    error::Error,
    io::{self, BufReader, BufRead, Read},
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

    let initial_labels = {
        if let Some(path) = options.map_file {
            let map_file = File::open(path)?;
            let map_file = BufReader::new(map_file);
            let mut labels = HashMap::new();
            for line in map_file.lines() {
                match line?.split_once('\t') {
                    Some((addr, lbl)) =>
                      labels.insert(addr.parse()?, lbl.to_string()),
                    _ => Err("invalid map file")?,
                };
            }
            Some(labels)
        } else {
            None
        }
    };

    let opts = DisAsmOpts {
        autolabel: options.autolabel,
        initial_labels,
    };

    let map = ImageMap::new(&prog, &opts);

    if let Some(path) = options.output_file {
        map.disasm(&mut File::create(path)?)?;
    } else {
        map.disasm(&mut io::stdout())?;
    }

    Ok(())
}
