// cargo run --features reqwest --example by_id

use balatro_mod_index::{forge, lfs, mods::ModIndex};

use env_logger::Env;

const CONCURRENCY_FACTOR: usize = 50;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let index_repo = forge::Tree::default();

    log::info!("fetching index...");
    let mut index = ModIndex::from_reqwest(&reqwest, &index_repo).await?;

    // fetch all thumbnail download urls at once
    index
        .mut_fetch_blob_urls(&reqwest, CONCURRENCY_FACTOR, 0, index.mods.len(), false)
        .await?;

    let mut my_mods = index
        .mods
        .iter_mut()
        .filter(|(id, _)| {
            [
                "kasimeka@typist",
                "Breezebuilder@SystemClock", // has no update timestamp
                "MathIsFun0@Talisman",       // has no thumbnail
            ]
            .contains(&id.to_string().as_str())
        })
        .collect::<Vec<_>>();

    log::info!("fetching thumbnails");
    lfs::mut_fetch_blobs(
        &mut my_mods
            .iter_mut()
            .filter_map(|(_, m)| m.thumbnail.as_mut()) // drop mods without thumbnails
            .collect::<Vec<_>>(),
        &reqwest,
        CONCURRENCY_FACTOR,
    )
    .await;

    for (id, m) in my_mods {
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

    Ok(())
}
