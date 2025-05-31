// cargo run --features reqwest --example no_request_wasted

use std::{collections::HashMap, time};

use balatro_mod_index::{github::Tree, lfs, mods};

use env_logger::Env;

const CONCURRENCY_FACTOR: usize = 50;

const PAGE_SIZE: usize = 5;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let index_repo = Tree {
        hostname: "github.com",
        namespace: "balatro-mod-index",
        name: "repo",
        rev: "main",
    }; // the exact same as `Tree::default()`

    log::info!("fetching index");
    let mut index = mods::from_reqwest(&reqwest, &index_repo).await?;
    let mods = &mut index.mods;
    mods.sort_by(|(_, a), (_, b)| a.meta.title.cmp(&b.meta.title));
    mods.sort_by(|(_, a), (_, b)| b.meta.last_updated.cmp(&a.meta.last_updated));
    let thumbnail_pointers = index
        .mods
        .iter()
        .filter_map(|(_, m)| m.thumbnail.as_ref().map(|p| &p.pointer))
        .collect::<Vec<_>>();
    let lfs_urls = lfs::batch_query_objects(&thumbnail_pointers, &reqwest, &index_repo).await?;

    log::info!("---------------------pagination demo");
    {
        let lfs_urls = lfs_urls.iter().cloned().collect::<HashMap<_, _>>();
        let mut next = 0;
        // fetch 4 pages of `PAGE_COUNT` mods each
        for _ in 1..=4 {
            let thumbnail_pointers = index
                .mods
                .iter()
                .skip(next)
                .take(PAGE_SIZE)
                .filter_map(|(_, m)| m.thumbnail.as_ref().map(|p| &p.pointer));
            let urls = thumbnail_pointers
                .map(|t| {
                    let oid = &t.oid;
                    (oid, lfs_urls.get(oid).unwrap())
                })
                .collect::<Vec<_>>();
            let thumbnails = lfs::memoized_concurrent_download(
                &urls,
                &reqwest,
                CONCURRENCY_FACTOR, // EXAMPLE_COUNT is lower than this so it does nothing
            )
            .await?;
            log::info!("\nfetched {} thumbnails", thumbnails.len());

            for (id, m) in index.mods.iter().skip(next).take(PAGE_SIZE) {
                log::info!(
                    "mod `{}`, last updated on {} has {}",
                    id,
                    m.meta.last_updated.unwrap_or_default(),
                    m.thumbnail.as_ref().map(|p| &p.pointer.oid).map_or(
                        "no thumbnail".to_string(),
                        |oid| format!(
                            "thumbnail of size {}",
                            thumbnails.get(oid).map_or_else(
                                || {
                                    log::warn!("thumbnail {oid} for mod {id} is empty");
                                    0
                                },
                                bytes::Bytes::len
                            )
                        )
                    )
                );
            }

            next += PAGE_SIZE;
        }
    }

    log::info!("---------------------download caching demo");
    {
        let last_url = lfs_urls.last().unwrap();
        let download_vec = &vec![(&last_url.0, &last_url.1)];
        for _ in 0..=10 {
            let now = time::SystemTime::now();
            let thumbnails =
                lfs::memoized_concurrent_download(download_vec, &reqwest, CONCURRENCY_FACTOR)
                    .await?;
            log::info!(
                "fetched thumbnail of size {} in {}ms",
                thumbnails.get(&last_url.0).map(bytes::Bytes::len).unwrap(),
                now.elapsed().unwrap().as_millis()
            );
        }
    }
    Ok(())
}
