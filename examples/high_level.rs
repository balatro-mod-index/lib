// cargo run --example high_level --features reqwest

use bmm_index::{github::GitHubTree, lfs, r#mod::from_reqwest};

use env_logger::Env;

const CONCURRENT_REQUESTS: usize = 50;

const EXAMPLE_COUNT: usize = 20;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let tree = GitHubTree::default();

    let mut index = from_reqwest(&reqwest, &tree).await?;
    let mods = &mut index.mods;
    mods.sort_by(|(_, a), (_, b)| a.meta.title.cmp(&b.meta.title));
    mods.sort_by(|(_, a), (_, b)| b.meta.last_updated.cmp(&a.meta.last_updated));

    let mods_with_thumbs = mods.iter().filter(|(_, m)| m.thumbnail.is_some());
    let thumb_pointers = mods_with_thumbs
        .map(|(_, m)| m.thumbnail.as_ref().unwrap())
        .take(EXAMPLE_COUNT)
        .collect::<Vec<_>>();

    let thumbs =
        lfs::batch_download(&thumb_pointers, &reqwest, &tree, CONCURRENT_REQUESTS, false).await?;

    for (id, m) in mods.iter().take(EXAMPLE_COUNT) {
        if let Some(p) = &m.thumbnail {
            println!(
                "mod `{}`, last updated on {} has thumbnail of size {}",
                id,
                m.meta.last_updated.unwrap_or_default(),
                thumbs.get(&p.oid).map_or(0, bytes::Bytes::len)
            );
        }
    }

    Ok(())
}
