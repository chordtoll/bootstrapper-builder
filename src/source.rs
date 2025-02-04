use std::path::PathBuf;

use tracing::info;

use crate::recipe::SourceContents;

pub fn source_path(hash: &str) -> PathBuf {
    PathBuf::from("build-cache")
        .join("source")
        .join(&hash[0..2])
        .join(&hash[2..4])
        .join(hash)
}

pub async fn fetch_source(source: &SourceContents) -> Vec<u8> {
    info!("Downloading {}", source.url);
    let source_data = reqwest::get(&source.url)
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(source.sha, sha256::digest(&*source_data));
    let store_path = source_path(&source.sha);
    std::fs::create_dir_all(store_path.parent().unwrap()).unwrap();
    std::fs::write(store_path, &source_data).unwrap();
    source_data.to_vec()
}
