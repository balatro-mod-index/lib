#![allow(clippy::missing_errors_doc)]

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct Pointer {
    pub oid: String,
    pub size: usize,
}

#[derive(serde::Serialize)]
pub struct BatchRequest<'a> {
    pub operation: String,
    pub objects: &'a Vec<Pointer>,
}

#[derive(Debug, serde::Deserialize)]
pub struct BatchResponse {
    pub objects: Vec<BatchResponseObject>,
}
#[derive(Debug, serde::Deserialize)]
pub struct BatchResponseObject {
    pub oid: String,
    pub size: usize,
    pub actions: BatchResponseActions,
}
#[derive(Debug, serde::Deserialize)]
pub struct BatchResponseActions {
    pub download: BatchResponseActionsDownload,
}
#[derive(Debug, serde::Deserialize)]
pub struct BatchResponseActionsDownload {
    pub href: String,
}

pub const VERSION: &str = "https://git-lfs.github.com/spec/v1";
pub const GH_API_HASH_ALGO: &str = "sha256";
pub const GH_API_UNAUTHENTICATED_BATCH_OBJECT_LIMIT: usize = 100;
#[allow(clippy::missing_errors_doc)]
pub fn parse_pointer(input: &str) -> Result<Pointer, String> {
    let mut pointer = Pointer::default();
    for line in input.lines() {
        let (key, value) = line
            .trim()
            .split_once(' ')
            .ok_or("line is not a key-value pair")?;
        match key {
            "version" => {
                if value != VERSION {
                    return Err(format!("unexpected lfs api version: \"{value}\""));
                }
            }
            "oid" => {
                let mut parts = value.split(':');
                if parts.next().ok_or("hashing algorithm missing")? != GH_API_HASH_ALGO {
                    return Err("lfs hash algorithm is not sha256".into());
                }
                pointer.oid = parts.next().ok_or("oid missing")?.to_string();
            }
            "size" => pointer.size = value.parse().map_err(|_| "couldn't parse size")?,
            _ => {}
        }
    }

    Ok(pointer)
}
pub trait BatchPointers {
    #[allow(clippy::missing_errors_doc)]
    fn batch_pointers(&self, offset: usize, count: usize)
    -> Result<(Vec<&Pointer>, usize), String>;
}
impl BatchPointers for &Vec<&Pointer> {
    fn batch_pointers(
        &self,
        offset: usize,
        count: usize,
    ) -> Result<(Vec<&Pointer>, usize), String> {
        use std::cmp::min;

        if offset >= self.len() {
            return Err("cursor out of bounds".into());
        }

        let count = min(count, self.len() - offset);
        Ok((
            self.iter().skip(offset).take(count).copied().collect(),
            offset + count,
        ))
    }
}

#[cfg(feature = "reqwest")]
#[allow(clippy::missing_errors_doc)]
pub async fn batch_query_objects(
    pointers: &Vec<&Pointer>,
    client: &reqwest::Client,
    tree: &super::github::Tree<'_>,
) -> Result<Vec<(String, String)>, String> {
    let mut download_urls = Vec::new();
    let mut offset = 0;
    loop {
        log::debug!("getting lfs object info at offset {offset}");
        let page = pointers.batch_pointers(offset, GH_API_UNAUTHENTICATED_BATCH_OBJECT_LIMIT);
        let pointers = match page {
            Ok((pointers, next_offset)) => {
                log::trace!(
                    "got {} pointers, next offset is {next_offset}",
                    pointers.len()
                );
                offset = next_offset;
                pointers
            }
            Err(e) => {
                log::debug!("done getting lfs object info: {e}");
                break;
            }
        };
        let objects = pointers.into_iter().cloned().collect::<Vec<_>>();
        let resp = client
            .post(format!(
                "https://{}/{}/{}.git/info/lfs/objects/batch",
                tree.hostname, tree.namespace, tree.name
            ))
            .json(&BatchRequest {
                operation: String::from("download"),
                objects: &objects,
            })
            .header("Accept", "application/vnd.git-lfs+json")
            .header("Content-Type", "application/vnd.git-lfs+json")
            .send()
            .await
            .map_err(|_| "couldn't send request".to_string())?;
        let data = resp
            .bytes()
            .await
            .map_err(|e| format!("couldn't read raw response: {e}"))?
            .to_vec();

        let data: BatchResponse = serde_json::from_slice(&data)
            .map_err(|_| match String::from_utf8(data) {
                Ok(s) => format!("response was not json, but a string: {s}"),
                Err(e) => format!("response was not json, and not a valid utf-8 string: {e}"),
            })
            .map_err(|e| format!("couldn't parse response: {e}"))?;

        download_urls.extend(
            data.objects
                .into_iter()
                .map(|obj| (obj.oid, obj.actions.download.href)),
        );
    }

    Ok(download_urls)
}

#[cfg(feature = "reqwest")]
#[cached::proc_macro::cached(
    result = true,
    ty = "cached::SizedCache<String, bytes::Bytes>",
    create = "{ cached::SizedCache::with_size(100) }",
    convert = r#"{ format!("{}", oid) }"#
)]
pub async fn fetch_one(
    client: &reqwest::Client,
    url: &String,
    #[allow(unused_variables)] oid: &str, // our cache key
) -> Result<bytes::Bytes, String> {
    log::debug!("fetching blob at {url}");
    client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("couldn't fetch blob at {url}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("couldn't get response body {url}: {e}"))
}

#[cfg(feature = "reqwest")]
use std::collections::HashMap;

#[cfg(feature = "reqwest")]
#[allow(clippy::missing_errors_doc)]
pub async fn memoized_concurrent_download<'a>(
    urls: &'a Vec<(&'a String, &'a String)>,
    client: &'a reqwest::Client,
    concurrency_factor: usize,
) -> Result<HashMap<&'a String, bytes::Bytes>, String> {
    concurrent_download_impl(fetch_one, urls, client, concurrency_factor).await
}
#[cfg(feature = "reqwest")]
#[allow(clippy::missing_errors_doc)]
pub async fn concurrent_download<'a>(
    urls: &'a Vec<(&'a String, &'a String)>,
    client: &'a reqwest::Client,
    concurrency_factor: usize,
) -> Result<HashMap<&'a String, bytes::Bytes>, String> {
    concurrent_download_impl(fetch_one_no_cache, urls, client, concurrency_factor).await
}

#[cfg(feature = "reqwest")]
#[allow(clippy::missing_errors_doc)]
async fn concurrent_download_impl<'a, F, Fut>(
    fetch_fn: F,
    urls: &'a Vec<(&'a String, &'a String)>,
    client: &'a reqwest::Client,
    concurrency_factor: usize,
) -> Result<HashMap<&'a String, bytes::Bytes>, String>
where
    F: Fn(&'a reqwest::Client, &'a String, &'a str) -> Fut, // Now returns Fut directly
    Fut: Future<Output = Result<bytes::Bytes, String>> + Send + 'a, // Fut is the concrete future type
{
    use futures::{StreamExt, stream};

    let buffer_unordered = stream::iter(urls)
        .map(|(oid, url)| {
            let fetch_fn = &fetch_fn;
            async move { (*oid, fetch_fn(client, url, oid).await) }
        })
        .buffer_unordered(concurrency_factor);
    Ok(buffer_unordered
        .filter_map(|(oid, req)| async move { req.ok().map(|bytes| (oid, bytes)) })
        .collect::<HashMap<_, _>>()
        .await)
}
