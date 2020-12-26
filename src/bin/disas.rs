use std::{
    env,
    error::Error,
    io::{self, Read},
    fs::File,
};

use synacor_vm::{
    binary,
    asm::{ImageMap, DisAsmOpts},
};

fn main() -> Result<(), Box<dyn Error>> {
    let bin = env::args().nth(1).ok_or("missing binary file path")?;
    let prog = {
        let mut bin = File::open(bin)?;
        let mut prog = Vec::new();
        bin.read_to_end(&mut prog)?;
        binary::read_binary(&prog)?
    };

    let map = ImageMap::new(&prog, &DisAsmOpts::default());
    map.disasm(&mut io::stdout())?;

    Ok(())
}
