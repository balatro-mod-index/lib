use std::fmt;

use crate::github::GitHubTree;
#[cfg(feature = "lfs")]
use crate::lfs;
#[cfg(feature = "zip")]
use zip::ZipArchive;

#[derive(Eq, Hash, PartialEq)]
pub struct ModId(pub String);
impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Clone for ModId {
    fn clone(&self) -> Self {
        ModId(self.0.clone())
    }
}

#[derive(Default, Debug)]
pub struct Mod {
    pub meta: ModMeta,
    pub description: Option<String>,
    #[cfg(feature = "lfs")]
    pub thumbnail: Option<lfs::Pointer>,
}

pub struct ModIndex<'a> {
    pub mods: Vec<(ModId, Mod)>,
    pub repo: &'a GitHubTree<'a>,
}

#[cfg(feature = "lfs")]
#[allow(clippy::missing_errors_doc)]
impl ModIndex<'_> {
    pub fn batch_lfs_on<F>(
        &self,
        f: F,
        offset: usize,
        count: usize,
    ) -> Result<(Vec<&lfs::Pointer>, usize), String>
    where
        F: Fn(&Mod) -> Option<&lfs::Pointer>,
    {
        use std::cmp::min;

        if offset >= self.mods.len() {
            return Err("cursor out of bounds".into());
        }

        let count = min(count, self.mods.len() - offset);
        let mut pointers = Vec::with_capacity(count);
        for (_, mod_data) in self.mods.iter().skip(offset).take(count) {
            if let Some(p) = f(mod_data) {
                pointers.push(p);
            }
        }

        Ok((pointers, offset + count + 1))
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ModMeta {
    pub requires_steamodded: bool,
    pub requires_talisman: bool,
    pub categories: Vec<String>,
    pub author: String,
    pub repo: String,
    pub title: String,
    #[serde(rename = "downloadURL")]
    pub download_url: Option<String>,
    #[serde(rename = "folderName")]
    pub folder_name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub automatic_version_check: bool,
    pub last_updated: Option<u64>,
}
impl ModMeta {
    #[allow(clippy::missing_errors_doc)]
    pub fn from_slice(bytes: &[u8]) -> Result<ModMeta, String> {
        serde_json::from_slice::<ModMeta>(bytes).map_err(|e| e.to_string())
    }
}
impl std::str::FromStr for ModMeta {
    type Err = String;
    fn from_str(text: &str) -> Result<ModMeta, Self::Err> {
        serde_json::from_str::<ModMeta>(text).map_err(|e| e.to_string())
    }
}

#[cfg(feature = "zip")]
#[allow(clippy::missing_errors_doc)]
impl ModIndex<'_> {
    pub fn from_zip<'a, R: std::io::Read + std::io::Seek>(
        zip: &mut ZipArchive<R>,
        source_spec: &'a GitHubTree,
    ) -> Result<ModIndex<'a>, String> {
        use std::{collections::HashMap, io::Read};

        let mut mods = HashMap::<ModId, Mod>::new();
        for file_number in 0..zip.len() {
            let mut item = zip.by_index(file_number).map_err(|e| e.to_string())?;

            let prefix = format!("{}-{}/mods/", source_spec.name, source_spec.rev);
            if !item.is_file() || !item.name().starts_with(&prefix) {
                continue;
            }

            let mut buffer: Vec<u8> = Vec::new();
            let _bytes_read = item.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

            let path = item.name().trim_start_matches(&prefix);
            let parts = path.split('/').collect::<Vec<_>>();
            let mod_id = ModId((*parts.first().ok_or("path is not pathing")?).to_string());
            let the_mod = mods.entry(mod_id).or_default();

            match *parts.get(1).ok_or("sub-path is not pathing")? {
                "meta.json" => {
                    the_mod.meta = ModMeta::from_slice(&buffer)
                        .map_err(|e| format!("couldn't parse mod meta for {}: {e}", parts[0]))?;
                }
                "description.md" => {
                    the_mod.description = Some(String::from_utf8_lossy(&buffer).to_string());
                }
                #[cfg(feature = "lfs")]
                "thumbnail.jpg" | "thumbnail.png" => {
                    the_mod.thumbnail = Some(
                        lfs::parse_pointer(&String::from_utf8_lossy(&buffer))
                            .map_err(|e| format!("couldn't parse lfs pointer: {e}"))?,
                    );
                }
                _ => {}
            }
        }

        Ok(ModIndex {
            mods: mods.into_iter().collect::<Vec<_>>(),
            repo: source_spec,
        })
    }
}

#[cfg(all(feature = "reqwest", feature = "zip"))]
#[allow(clippy::missing_errors_doc)]
pub async fn from_reqwest<'a>(
    reqwest: &reqwest::Client,
    source_spec: &'a GitHubTree<'a>,
) -> Result<ModIndex<'a>, String> {
    let url = format!(
        "https://{}/{}/{}/archive/refs/heads/{}.zip",
        source_spec.hostname, source_spec.namespace, source_spec.name, source_spec.rev
    );
    let response = reqwest.get(&url).send().await.map_err(|e| e.to_string())?;

    let zip = response.bytes().await.map_err(|e| e.to_string())?;
    log::info!("downloaded zip file {url}");

    let mut archive = ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| e.to_string())?;
    log::debug!("scanning {} files", archive.len());
    ModIndex::from_zip(&mut archive, source_spec).map_err(|e| e.to_string())
}
