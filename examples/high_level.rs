// cargo run --example high_level --features reqwest

use bmm_index::{github::Tree, lfs, mods};

use env_logger::Env;

const CONCURRENCY_FACTOR: usize = 50;

const EXAMPLE_COUNT: usize = 5;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let index_repo = Tree {
        hostname: "github.com",
        namespace: "kasimeka",
        name: "bmm-index-ng",
        rev: "main",
    }; // the exact same as `Tree::default()`

    let mut index = mods::from_reqwest(&reqwest, &index_repo).await?;
    let mods = &mut index.mods;
    mods.sort_by(|(_, a), (_, b)| a.meta.title.cmp(&b.meta.title));
    mods.sort_by(|(_, a), (_, b)| b.meta.last_updated.cmp(&a.meta.last_updated));

    let mut next = 0;
    // fetch 4 pages of 5 mods each
    for _ in 1..=4 {
        let (thumb_pointers, n) = index
            .batch_lfs_on(|m| m.thumbnail.as_ref(), next, EXAMPLE_COUNT)
            .map_err(|e| e.to_string())?;

        let thumbs = lfs::batch_download(
            &thumb_pointers,
            &reqwest,
            &index_repo,
            CONCURRENCY_FACTOR, // EXAMPLE_COUNT is lower than this so it does nothing
            false,
        )
        .await?;
        log::info!("\nfetched {} thumbnails", thumbs.len());

        for (id, m) in index.mods.iter().skip(next).take(EXAMPLE_COUNT) {
            log::info!(
                "mod `{}`, last updated on {} has {}",
                id,
                m.meta.last_updated.unwrap_or_default(),
                m.thumbnail
                    .as_ref()
                    .map_or("no thumbnail".to_string(), |p| format!(
                        "thumbnail of size {}",
                        thumbs.get(&p.oid).map_or_else(
                            || {
                                log::warn!("thumbnail {} for mod {} is empty", p.oid, id);
                                0
                            },
                            bytes::Bytes::len
                        )
                    ))
            );
        }

        next = n;
    }

    Ok(())
}
