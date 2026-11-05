use std::{
    fs::{self, File},
    io::{self, Seek},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{
    config::{Config, GameSource},
    embedded,
};

#[derive(Clone, Debug)]
pub struct GameInstall {
    pub exe: PathBuf,
    pub workdir: PathBuf,
}

impl GameInstall {
    pub fn ensure(config: &Config) -> Result<Self> {
        match config.game_source() {
            GameSource::Dir(dir) => Self::from_dir(&dir),
            GameSource::Zip(zip) => Self::from_zip(config, &zip),
            GameSource::Embedded => Self::from_embedded(config),
        }
    }

    pub fn from_embedded(config: &Config) -> Result<Self> {
        let target = config.game_extract_dir();
        fs::create_dir_all(&target)?;
        let marker = target.join(".metarace-extracted");
        let needs_extract = !marker.exists() || find_dedicated_server(&target).is_none();
        if needs_extract {
            extract_zip_bytes(embedded::GAME_ZIP, &target)?;
            let _ = fs::write(&marker, b"ok");
        }
        let exe = find_dedicated_server(&target).with_context(|| {
            format!(
                "DedicatedServer.exe was not found after extracting the embedded game payload to {}",
                target.display()
            )
        })?;
        let workdir = workdir_for(&exe)?;
        Ok(Self { exe, workdir })
    }

    pub fn from_dir(dir: &Path) -> Result<Self> {
        if !dir.exists() {
            anyhow::bail!("game directory does not exist: {}", dir.display());
        }
        let exe = find_dedicated_server(dir).with_context(|| {
            format!("DedicatedServer.exe was not found under {}", dir.display())
        })?;
        let workdir = workdir_for(&exe)?;
        Ok(Self { exe, workdir })
    }

    pub fn from_zip(config: &Config, zip_path: &Path) -> Result<Self> {
        if !zip_path.exists() {
            anyhow::bail!("game ZIP does not exist: {}", zip_path.display());
        }
        let target = config.data_dir.join("game");
        fs::create_dir_all(&target)?;
        let marker = target.join(".metarace-extracted");
        let needs_extract = !marker.exists()
            || find_dedicated_server(&target).is_none()
            || zip_is_newer(zip_path, &marker);
        if needs_extract {
            extract_zip(zip_path, &target)?;
            let _ = fs::write(&marker, b"ok");
        }
        let exe = find_dedicated_server(&target).with_context(|| {
            format!(
                "DedicatedServer.exe was not found after extracting {}",
                zip_path.display()
            )
        })?;
        let workdir = workdir_for(&exe)?;
        Ok(Self { exe, workdir })
    }
}

fn workdir_for(exe: &Path) -> Result<PathBuf> {
    exe.parent()
        .map(Path::to_path_buf)
        .context("DedicatedServer.exe has no parent directory")
}

fn zip_is_newer(zip: &Path, marker: &Path) -> bool {
    let zip_time = fs::metadata(zip).and_then(|metadata| metadata.modified());
    let marker_time = fs::metadata(marker).and_then(|metadata| metadata.modified());
    match (zip_time, marker_time) {
        (Ok(zip_time), Ok(marker_time)) => zip_time > marker_time,
        _ => true,
    }
}

fn extract_zip(zip_path: &Path, target: &Path) -> Result<()> {
    let file =
        File::open(zip_path).with_context(|| format!("open game ZIP {}", zip_path.display()))?;
    extract_zip_reader(file, target)
}

fn extract_zip_bytes(bytes: &[u8], target: &Path) -> Result<()> {
    extract_zip_reader(io::Cursor::new(bytes), target)
}

fn extract_zip_reader<R: io::Read + Seek>(reader: R, target: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(reader).context("read game ZIP")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            anyhow::bail!("game ZIP contains an unsafe path: {}", entry.name());
        };
        let out_path = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)
            .with_context(|| format!("create extracted file {}", out_path.display()))?;
        io::copy(&mut entry, &mut out)
            .with_context(|| format!("extract {}", out_path.display()))?;
    }
    Ok(())
}

pub fn find_dedicated_server(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("DedicatedServer.exe"))
            {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_and_finds_exe() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("game.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("4d73/112.0.1733.0/Game/DedicatedServer.exe", options)
                .unwrap();
            zip.write_all(b"fake exe").unwrap();
            zip.finish().unwrap();
        }
        let target = dir.path().join("out");
        extract_zip(&zip_path, &target).unwrap();
        let exe = find_dedicated_server(&target).unwrap();
        assert!(exe.ends_with("DedicatedServer.exe"));
    }
}
