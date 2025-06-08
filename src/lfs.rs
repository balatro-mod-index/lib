#![allow(clippy::missing_errors_doc)]

#[derive(PartialEq, Eq, Hash, Default, Debug, Clone, serde::Serialize)]
pub struct Pointer {
    pub oid: String,
    pub size: usize,
}
#[derive(Debug, Clone)]
pub struct Blob<'tree> {
    pub pointer: Pointer,
    pub url: Option<String>,
    pub data: Result<bytes::Bytes, String>,
    pub tree: &'tree crate::forge::Tree<'tree>,
}

#[derive(serde::Serialize)]
pub struct BatchRequest<'pointers> {
    pub operation: String,
    pub objects: &'pointers [&'pointers Pointer],
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

#[cfg(feature = "reqwest")]
pub async fn mut_fetch_download_urls(
    blobs: &mut [&mut Blob<'_>],
    client: &reqwest::Client,
    concurrency_factor: usize,
    refresh_available: bool,
) -> Result<(), String> {
    use futures::{StreamExt, stream};
    use std::cmp::min;

    let tree = blobs.first().ok_or("no blobs to fetch")?.tree;
    let pointers = if refresh_available {
        blobs.iter().map(|b| &b.pointer).collect::<Vec<_>>()
    } else {
        blobs
            .iter()
            .filter(|b| b.url.is_none())
            .map(|b| &b.pointer)
            .collect::<Vec<_>>()
    };

    if pointers.is_empty() {
        log::debug!("no lfs info to fetch");
        return Ok(());
    }

    log::debug!("fetching lfs info for {} blobs", pointers.len());

    let mut tasks = Vec::new();

    let mut offset = 0;
    while offset < pointers.len() {
        let count = min(
            GH_API_UNAUTHENTICATED_BATCH_OBJECT_LIMIT,
            pointers.len() - offset,
        );

        let next = offset + count;

        let batch = &pointers[offset..next];

        let future = async move {
            log::debug!("getting lfs object info at offset {offset}");
            let resp = client
                .post(format!(
                    "https://{}/{}/{}.git/info/lfs/objects/batch",
                    tree.hostname, tree.namespace, tree.name
                ))
                .json(&BatchRequest {
                    operation: String::from("download"),
                    objects: batch,
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

            Ok(data
                .objects
                .into_iter()
                .map(|obj| obj.actions.download.href)
                .collect::<Vec<_>>())
        };
        tasks.push(future);

        offset = next;
    }

    let download_urls = stream::iter(tasks)
        .buffer_unordered(concurrency_factor)
        .collect::<Vec<Result<Vec<String>, String>>>()
        .await
        .into_iter()
        .map(Result::ok)
        .try_fold(Vec::new(), |mut acc, result| {
            acc.extend(result?);
            Some(acc)
        })
        .ok_or("couldn't fetch download urls")?;

    for (blob, url) in blobs.iter_mut().zip(download_urls) {
        blob.url = Some(url);
    }

    Ok(())
}

#[cfg(feature = "reqwest")]
pub async fn mut_fetch_blobs(
    blobs: &mut [&mut Blob<'_>],
    client: &reqwest::Client,
    concurrency_factor: usize,
) -> Result<(), String> {
    use futures::{StreamExt, stream};

    stream::iter(blobs.iter_mut().filter_map(|b| {
        b.url.as_ref().map(|url| async {
            b.data = fetch_one(client, url, &b.pointer.oid).await;
        })
    }))
    .buffer_unordered(concurrency_factor)
    .collect::<Vec<_>>()
    .await;

    Ok(())
}

#[cfg(feature = "reqwest")]
// cache a max of 100 responses for 30 minutes
#[cached::proc_macro::cached(
    result = true,
    ty = "cached::TimedSizedCache<String, bytes::Bytes>",
    create = "{ cached::TimedSizedCache::with_size_and_lifespan(100, 1800) }",
    convert = r#"{ format!("{}", oid) }"#
)]
pub async fn fetch_one(
    client: &reqwest::Client,
    url: &String,
    #[allow(unused_variables)] oid: &str, // our cache key
) -> Result<bytes::Bytes, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("couldn't GET {url}: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("couldn't get response body for {url}: {e}"))
}
