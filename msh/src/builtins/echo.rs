use crate::error::Result;
use std::io::{self, Write};

pub fn run(args: &[String]) -> Result<()> {
    let (newline, rest) = if args.first().is_some_and(|a| a == "-n") {
        (false, &args[1..])
    } else {
        (true, args)
    };

    let output = rest.join(" ");
    print!("{output}");
    if newline {
        println!();
    }
    io::stdout().flush()?;
    Ok(())
}
