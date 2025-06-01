// cargo run --features reqwest --example no_request_wasted

use balatro_mod_index::{github::Tree, lfs, mods};

use env_logger::Env;

const CONCURRENCY_FACTOR: usize = 50;

const PAGE_SIZE: usize = 20;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let index_repo = Tree::default();

    log::info!("fetching index");
    let mut index = mods::from_reqwest(&reqwest, &index_repo).await?;
    let mods = &mut index.mods;
    mods.sort_by(|(_, a), (_, b)| a.meta.title.cmp(&b.meta.title));
    mods.sort_by(|(_, a), (_, b)| b.meta.last_updated.cmp(&a.meta.last_updated));

    // fetch all download urls
    lfs::mut_fetch_download_urls(
        &mut mods
            .iter_mut()
            .filter_map(|(_, m)| m.thumbnail.as_mut())
            .collect::<Vec<_>>(),
        &reqwest,
        &index_repo,
        false,
    )
    .await?;
    // but only `PAGE_SIZE` thumbnails
    lfs::mut_fetch_blobs(
        &mut mods
            .iter_mut()
            .take(PAGE_SIZE)
            .filter_map(|(_, m)| m.thumbnail.as_mut())
            .collect::<Vec<_>>(),
        &reqwest,
        &index_repo,
        CONCURRENCY_FACTOR,
        false,
    )
    .await?;

    for (mod_id, mod_data) in index.mods.iter().take(PAGE_SIZE) {
        log::info!(
            "{mod_id}: has {}",
            mod_data
                .thumbnail
                .as_ref()
                .map_or("no thumbnail".to_string(), |t| format!(
                    "thumbnail of size {}",
                    t.data.as_ref().map_or(0, bytes::Bytes::len)
                ))
        );
    }

    Ok(())
}
