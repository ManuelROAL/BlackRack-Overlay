use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const GAME_RELATIVE_PATH: &str = "steamapps/common/Le Mans Ultimate";
const TELEMETRY_PLUGIN: &str = "Plugins/LMU_SharedMemoryMapPlugin64.dll";

pub(crate) fn installations() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("LMU_INSTALL_DIR") {
        candidates.push(PathBuf::from(path));
    }

    let mut steam_roots = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles(x86)") {
        steam_roots.push(PathBuf::from(program_files).join("Steam"));
    }
    for drive in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:/", drive as char));
        steam_roots.push(root.join("Steam"));
        steam_roots.push(root.join("SteamLibrary"));
        steam_roots.push(root.join("Program Files (x86)/Steam"));
    }

    let mut library_roots = steam_roots.clone();
    for steam in &steam_roots {
        library_roots.extend(libraries_from_vdf(
            &steam.join("steamapps/libraryfolders.vdf"),
        ));
    }
    for library in library_roots {
        candidates.push(library.join(GAME_RELATIVE_PATH));
    }

    let mut seen = HashSet::new();
    candidates
        .retain(|path| path.is_dir() && seen.insert(path.to_string_lossy().to_ascii_lowercase()));
    candidates
}

pub(crate) fn telemetry_plugin() -> Option<PathBuf> {
    installations()
        .into_iter()
        .map(|installation| installation.join(TELEMETRY_PLUGIN))
        .find(|plugin| plugin.is_file())
}

fn libraries_from_vdf(path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let quoted = line.split('"').skip(1).step_by(2).collect::<Vec<_>>();
            let index = quoted
                .iter()
                .position(|value| value.trim().eq_ignore_ascii_case("path"))?;
            let value = quoted.get(index + 1)?.trim();
            Some(PathBuf::from(value.replace("\\\\", "\\")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::libraries_from_vdf;
    use std::fs;

    #[test]
    fn extracts_steam_library_paths() {
        let path = std::env::temp_dir().join(format!(
            "lmu-overlay-libraryfolders-{}.vdf",
            std::process::id()
        ));
        fs::write(
            &path,
            "\"libraryfolders\"\n{\n\"0\" { \"path\" \"C:\\\\Program Files (x86)\\\\Steam\" }\n\"1\" { \"path\" \"D:\\\\SteamLibrary\" }\n}",
        )
        .unwrap();
        let libraries = libraries_from_vdf(&path);
        assert_eq!(libraries.len(), 2);
        assert_eq!(libraries[1].to_string_lossy(), "D:\\SteamLibrary");
        let _ = fs::remove_file(path);
    }
}
