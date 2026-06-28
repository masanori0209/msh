use crate::error::Result;
use std::env;
use std::io::{self, Write};

pub fn run() -> Result<()> {
    let cwd = env::current_dir()?;
    println!("{}", cwd.display());
    io::stdout().flush()?;
    Ok(())
}
