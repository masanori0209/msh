//! L2 WASM プラグイン PoC — 外部 `wasmtime` 委譲（新規 WASM ランタイム依存なし）。

use crate::error::{MshError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct WasmPlugin {
    pub name: String,
    pub dir: PathBuf,
    pub wasm_path: PathBuf,
    pub description: String,
}

/// `~/.config/msh/plugins/<name>/` 配下の `plugin.toml` + `.wasm` を列挙。
pub fn discover_plugins(home: &Path) -> Vec<WasmPlugin> {
    let root = home.join(".config").join("msh").join("plugins");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(plugin) = load_plugin_dir(&path) {
            plugins.push(plugin);
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

fn load_plugin_dir(dir: &Path) -> Option<WasmPlugin> {
    let manifest = dir.join("plugin.toml");
    if !manifest.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(&manifest).ok()?;
    let mut name = dir.file_name()?.to_string_lossy().into_owned();
    let mut wasm_file = "plugin.wasm".to_string();
    let mut description = String::new();

    for line in content.lines() {
        let line = line.split('#').next()?.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = trim_quotes(value.trim());
            match key.trim() {
                "name" => name = value.to_string(),
                "wasm" => wasm_file = value.to_string(),
                "description" => description = value.to_string(),
                _ => {}
            }
        }
    }

    let wasm_path = dir.join(wasm_file);
    if !wasm_path.is_file() {
        return None;
    }

    Some(WasmPlugin {
        name,
        dir: dir.to_path_buf(),
        wasm_path,
        description,
    })
}

fn trim_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

pub fn list_text(home: &Path) -> String {
    let plugins = discover_plugins(home);
    if plugins.is_empty() {
        return "No WASM plugins found in ~/.config/msh/plugins/".into();
    }
    let mut out = String::from("WASM plugins:\n");
    for p in plugins {
        out.push_str(&format!(
            "  {} — {} ({})\n",
            p.name,
            p.description,
            p.wasm_path.display()
        ));
    }
    out
}

/// `wasmtime run` でプラグインを起動（未インストール時は案内のみ）。
pub fn run_plugin(home: &Path, name: &str, invoke: &str) -> Result<String> {
    let plugin = discover_plugins(home)
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| MshError::ScriptError(format!("plugin: '{name}' not found")))?;

    let wasmtime = which_wasmtime().ok_or_else(|| {
        MshError::ScriptError(
            "plugin: wasmtime not found — install from https://wasmtime.dev/".into(),
        )
    })?;

    let output = Command::new(wasmtime)
        .arg("run")
        .arg("--dir")
        .arg(format!("{}::", plugin.dir.display()))
        .arg(&plugin.wasm_path)
        .arg("--invoke")
        .arg(invoke)
        .output()
        .map_err(|e| MshError::ScriptError(format!("plugin: failed to run wasmtime: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MshError::ScriptError(format!(
            "plugin: wasmtime exited {}: {}",
            output.status.code().unwrap_or(1),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn which_wasmtime() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("wasmtime");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_plugin_from_manifest() {
        let base = std::env::temp_dir().join(format!("msh-plug-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let plug_dir = base
            .join(".config")
            .join("msh")
            .join("plugins")
            .join("demo");
        fs::create_dir_all(&plug_dir).unwrap();
        fs::write(
            plug_dir.join("plugin.toml"),
            "name = \"demo\"\ndescription = \"test plugin\"\nwasm = \"demo.wasm\"\n",
        )
        .unwrap();
        fs::write(plug_dir.join("demo.wasm"), b"\0asm").unwrap();

        let plugins = discover_plugins(&base);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "demo");
        let _ = fs::remove_dir_all(&base);
    }
}
