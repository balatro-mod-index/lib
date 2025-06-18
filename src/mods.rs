use std::fmt;

use crate::forge::Tree;
#[cfg(feature = "lfs")]
use crate::lfs;
#[cfg(feature = "zip")]
use zip::ZipArchive;

#[derive(Eq, Hash, PartialEq, Debug, Default)]
pub struct ModId(pub String);
impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Clone for ModId {
    fn clone(&self) -> Self {
        ModId(self.0.clone())
    }
}
impl PartialEq<str> for ModId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

#[cfg(feature = "lfs")]
#[derive(Clone, Default, Debug)]
pub struct Mod<'tree> {
    pub meta: ModMeta,
    pub description: Option<String>,
    pub thumbnail: Option<lfs::Blob<'tree>>,
}
#[cfg(not(feature = "lfs"))]
#[derive(Clone, Default, Debug)]
pub struct Mod {
    pub meta: ModMeta,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ModIndex<'tree> {
    #[cfg(feature = "lfs")]
    pub mods: Vec<(ModId, Mod<'tree>)>,
    #[cfg(not(feature = "lfs"))]
    pub mods: Vec<(ModId, Mod)>,

    pub repo: &'tree Tree<'tree>,
}

#[cfg(feature = "zip")]
#[allow(clippy::missing_errors_doc)]
impl ModIndex<'_> {
    #[cfg(feature = "reqwest")]
    #[allow(clippy::missing_errors_doc)]
    pub async fn from_reqwest<'a>(
        reqwest: &reqwest::Client,
        tree: &'a Tree<'a>,
    ) -> Result<ModIndex<'a>, String> {
        use crate::forge::Forge::{GitHub, GitLab};

        let url = match tree.forge {
            GitHub => format!(
                "https://{}/{}/{}/archive/refs/heads/{}.zip",
                tree.hostname, tree.namespace, tree.name, tree.rev
            ),
            GitLab => format!(
                "https://{}/api/v4/projects/{}%2F{}/repository/archive.zip?include_lfs_blobs=false&sha={}",
                tree.hostname, tree.namespace, tree.name, tree.rev
            ),
        };
        let response = reqwest.get(&url).send().await.map_err(|e| e.to_string())?;

        let zip = response.bytes().await.map_err(|e| e.to_string())?;
        log::debug!("downloaded zip file {url}");

        let mut archive = ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| e.to_string())?;
        log::debug!("scanning {} files", archive.len());
        ModIndex::from_zip(&mut archive, tree).map_err(|e| e.to_string())
    }

    pub fn from_zip<'a, R: std::io::Read + std::io::Seek>(
        zip: &mut ZipArchive<R>,
        tree: &'a Tree<'_>,
    ) -> Result<ModIndex<'a>, String> {
        use std::{collections::HashMap, io::Read};

        #[allow(elided_lifetimes_in_paths)] // to work with or without `lfs` feature
        let mut mods = HashMap::<ModId, Mod>::new();

        for file_number in 0..zip.len() {
            let mut item = zip.by_index(file_number).map_err(|e| e.to_string())?;
            let prefix = format!("{}-{}", tree.name, tree.rev);

            let parts = item.name().split('/');
            if !item.is_file()
                || !matches!(
                    parts.take(2).collect::<Vec<_>>().as_slice(),
                    [first, "mods"] if first.starts_with(&prefix)
                )
            {
                continue;
            }

            let mut buffer: Vec<u8> = Vec::new();
            let _bytes_read = item.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

            let parts = item.name().split('/').skip(2).collect::<Vec<_>>();
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
                    the_mod.thumbnail = Some(lfs::Blob {
                        pointer: lfs::parse_pointer(&String::from_utf8_lossy(&buffer))
                            .map_err(|e| format!("couldn't parse lfs pointer: {e}"))?,
                        url: None,
                        data: Err("no download attempts yet".into()),
                        tree,
                    });
                }
                _ => {}
            }
        }

        Ok(ModIndex {
            mods: mods.into_iter().collect::<Vec<_>>(),
            repo: tree,
        })
    }
}

#[cfg(all(feature = "reqwest", feature = "lfs"))]
impl ModIndex<'_> {
    #[allow(clippy::missing_errors_doc)]
    pub async fn mut_fetch_blob_urls(
        &mut self,
        client: &reqwest::Client,
        concurrency_factor: usize,
        offset: usize,
        count: usize,
        refresh_urls: bool,
    ) -> Result<usize, String> {
        use std::cmp::min;

        if offset >= self.mods.len() {
            return Err(format!(
                "offset {offset} is out of bounds for index with {} mods",
                self.mods.len()
            ));
        }

        let next = min(offset + count, self.mods.len());

        let blobs = &mut self.mods[offset..next]
            .iter_mut()
            .filter_map(|(_, m)| m.thumbnail.as_mut())
            .collect::<Vec<_>>();

        lfs::mut_fetch_download_urls(blobs, client, concurrency_factor, refresh_urls).await?;

        Ok(next)
    }

    #[allow(clippy::missing_errors_doc)]
    pub async fn mut_fetch_blobs(
        &mut self,
        client: &reqwest::Client,
        concurrency_factor: usize,
        offset: usize,
        count: usize,
        refresh_urls: bool,
    ) -> Result<usize, String> {
        use std::cmp::min;

        if offset >= self.mods.len() {
            return Err(format!(
                "offset {offset} is out of bounds for index with {} mods",
                self.mods.len()
            ));
        }

        let next = min(offset + count, self.mods.len());

        let blobs = &mut self.mods[offset..next]
            .iter_mut()
            .filter_map(|(_, m)| m.thumbnail.as_mut())
            .collect::<Vec<_>>();

        lfs::mut_fetch_download_urls(blobs, client, concurrency_factor, refresh_urls).await?;
        lfs::mut_fetch_blobs(blobs, client, concurrency_factor).await;

        Ok(next)
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ModMeta {
    #[serde(default)]
    pub requires_steamodded: bool,
    #[serde(default)]
    pub requires_talisman: bool,
    pub categories: Vec<String>,
    pub author: String,
    pub repo: String,
    pub title: String,
    #[serde(rename = "downloadURL")]
    pub download_url: String,
    #[serde(rename = "folderName")]
    pub folder_name: Option<String>,
    pub version: String,
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
