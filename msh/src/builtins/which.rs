use crate::builtins;
use crate::error::Result;
use crate::expand;

pub fn run(args: &[String]) -> Result<()> {
    let Some(name) = args.first() else {
        return Err(crate::error::MshError::ParseError(
            "which: command name required".into(),
        ));
    };

    if builtins::is_builtin(name) {
        println!("builtin {name}");
        return Ok(());
    }

    if let Some(path) = expand::resolve_command_path(name) {
        println!("{}", path.display());
        return Ok(());
    }

    Err(crate::error::MshError::CommandNotFound(name.clone()))
}
