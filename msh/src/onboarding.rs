use std::fs;
use std::path::PathBuf;

pub fn maybe_show(config_dir: &PathBuf) {
    let marker = config_dir.join(".onboarded");
    if marker.is_file() {
        return;
    }

    println!("Welcome to msh!");
    println!("  Tab          command/path completion");
    println!("  Ctrl+R       search history");
    println!("  help         builtin commands");
    println!("  empty Enter  quick tips");
    println!();
    println!("Config: ~/.config/msh/config.toml");
    println!("Set onboarding_done = true to hide this message.");
    println!();

    let _ = fs::create_dir_all(config_dir);
    let _ = fs::write(marker, "1");
}

pub fn quick_tip() {
    println!("tip: try `help`, `pushd`, Tab completion, or `msh --compat bash`");
}
