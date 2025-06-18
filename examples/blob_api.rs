// cargo run --features reqwest --example blob_api

use balatro_mod_index::{
    forge::{self},
    mods::ModIndex,
};

use env_logger::Env;

const CONCURRENCY_FACTOR: usize = 50;

const PAGE_SIZE: usize = 5;
const PAGES_TO_FETCH: usize = 4;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let index_repo = forge::Tree::default();

    log::info!("fetching index...");
    let mut index = ModIndex::from_reqwest(&reqwest, &index_repo).await?;
    let mods = &mut index.mods;
    mods.sort_by(|(_, a), (_, b)| a.meta.title.cmp(&b.meta.title));
    mods.sort_by(|(_, a), (_, b)| b.meta.last_updated.cmp(&a.meta.last_updated));

    // fetch all thumbnail download urls at once
    index
        .mut_fetch_blob_urls(&reqwest, CONCURRENCY_FACTOR, 0, index.mods.len(), false)
        .await?;

    let mut offset = 0;
    for n in 1..=PAGES_TO_FETCH {
        log::info!("\n---------------------- page {n}");

        // fetch thumbnails without refetching their urls
        let next = index
            .mut_fetch_blobs(&reqwest, CONCURRENCY_FACTOR, offset, PAGE_SIZE, false)
            .await?;

        for (id, m) in index.mods.iter().skip(offset).take(PAGE_SIZE) {
            log::info!(
                "{id}: last updated at {} has {}",
                m.meta.last_updated.unwrap_or_default(),
                m.thumbnail
                    .as_ref()
                    .map_or("no thumbnail".to_string(), |t| format!(
                        "thumbnail of size {}",
                        t.data.as_ref().map_or(0, bytes::Bytes::len)
                    ))
            );
        }

        offset = next;
    }

    Ok(())
}
