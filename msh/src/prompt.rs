use colored::*;
use std::env;
use std::io::Result;
use whoami::{hostname, username};

pub fn display_prompt() -> Result<()> {
    let current_path = env::current_dir()?;
    print!(
        "{}{}{}:{}{}",
        username().green().truecolor(255, 255, 0).bold(),
        "@".truecolor(255, 255, 0).bold(),
        hostname().truecolor(255, 255, 0).bold(),
        current_path.display(),
        ">".truecolor(222, 255, 0).bold()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_current_directory() {
        assert!(display_prompt().is_ok());
    }
}
