use std::{
    collections::BTreeMap,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use int_enum::IntEnum;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::recipe::{DepdRecipeHash, HydratedRecipeVersion, SourceContents};

#[derive(Debug)]
pub enum StatusUpdate {
    CommandRun(Vec<String>),
    CommandOut(String),
    CommandError(String),
    CommandDone(i32),
    Done,
}

#[repr(u8)]
#[derive(Debug, PartialEq, IntEnum)]
pub enum WorkerStatus {
    ReadyForWork = 0,
    ReadyForSource = 1,
    ReadyForOverlay = 2,
    HaveSource = 3,
    NeedSource = 4,
    ReadyForDep = 5,
    HaveDep = 6,
    NeedDep = 7,
    HaveOverlay = 8,
    NeedOverlay = 9,
    ReadyForEnvs = 10,
    BuildComplete = 11,
}

pub async fn read_recipe(stream: &mut TcpStream) -> HydratedRecipeVersion {
    let recipe_len = stream.read_u64().await.unwrap();
    let mut recipe_buf = vec![0u8; recipe_len.try_into().unwrap()];
    stream.read_exact(recipe_buf.as_mut_slice()).await.unwrap();
    serde_yaml::from_slice(&recipe_buf).unwrap()
}

pub async fn read_sources(stream: &mut TcpStream) -> BTreeMap<String, (SourceContents, Vec<u8>)> {
    let mut source_data = BTreeMap::new();

    loop {
        stream
            .write_u8(WorkerStatus::ReadyForSource as u8)
            .await
            .unwrap();
        let source_name_len = stream.read_u16().await.unwrap();
        if source_name_len == 0 {
            break;
        };
        let mut source_name_buf = vec![0u8; source_name_len.into()];
        stream
            .read_exact(source_name_buf.as_mut_slice())
            .await
            .unwrap();
        let source_name = String::from_utf8(source_name_buf).unwrap();

        stream
            .write_u8(WorkerStatus::NeedSource as u8)
            .await
            .unwrap();

        let source_contents_len = stream.read_u32().await.unwrap();
        let mut source_contents_buf = vec![0u8; source_contents_len.try_into().unwrap()];
        stream
            .read_exact(source_contents_buf.as_mut_slice())
            .await
            .unwrap();
        let source_contents = serde_yaml::from_slice(&source_contents_buf).unwrap();

        let source_data_len = stream.read_u64().await.unwrap();
        let mut source_data_buf = vec![0u8; source_data_len.try_into().unwrap()];
        stream
            .read_exact(source_data_buf.as_mut_slice())
            .await
            .unwrap();

        source_data.insert(source_name, (source_contents, source_data_buf));
    }
    source_data
}

pub async fn write_source(
    stream: &mut TcpStream,
    name: &str,
    contents: &SourceContents,
    data: &[u8],
) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForSource as u8
    );
    let name = name.as_bytes().to_vec();
    stream
        .write_u16(name.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(&name).await.unwrap();
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::NeedSource as u8
    );
    let source_buf = serde_yaml::to_string(&contents)
        .unwrap()
        .as_bytes()
        .to_vec();
    stream
        .write_u32(source_buf.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(&source_buf).await.unwrap();
    stream
        .write_u64(data.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(data).await.unwrap();
}

pub async fn finish_sources(stream: &mut TcpStream) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForSource as u8
    );
    stream.write_u16(0).await.unwrap();
}

pub async fn read_deps(stream: &mut TcpStream) -> BTreeMap<String, Vec<u8>> {
    let mut dep_data = BTreeMap::new();

    loop {
        stream
            .write_u8(WorkerStatus::ReadyForDep as u8)
            .await
            .unwrap();
        let dep_name_len = stream.read_u16().await.unwrap();
        if dep_name_len == 0 {
            break;
        };
        let mut dep_name_buf = vec![0u8; dep_name_len.into()];
        stream
            .read_exact(dep_name_buf.as_mut_slice())
            .await
            .unwrap();
        let dep_name = String::from_utf8(dep_name_buf).unwrap();

        stream.write_u8(WorkerStatus::NeedDep as u8).await.unwrap();

        let dep_data_len = stream.read_u64().await.unwrap();
        let mut dep_data_buf = vec![0u8; dep_data_len.try_into().unwrap()];
        stream
            .read_exact(dep_data_buf.as_mut_slice())
            .await
            .unwrap();

        dep_data.insert(dep_name, dep_data_buf);
    }
    dep_data
}

pub async fn write_dep(stream: &mut TcpStream, equiv_hash: &DepdRecipeHash, data: &[u8]) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForDep as u8
    );
    let equiv_hash = equiv_hash.0.as_bytes().to_vec();
    stream
        .write_u16(equiv_hash.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(&equiv_hash).await.unwrap();
    assert_eq!(stream.read_u8().await.unwrap(), WorkerStatus::NeedDep as u8);
    stream
        .write_u64(data.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(data).await.unwrap();
}

pub async fn read_overlays(stream: &mut TcpStream) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut source_data = BTreeMap::new();

    loop {
        stream
            .write_u8(WorkerStatus::ReadyForOverlay as u8)
            .await
            .unwrap();
        let source_name_len = stream.read_u16().await.unwrap();
        if source_name_len == 0 {
            break;
        };
        let mut source_name_buf = vec![0u8; source_name_len.into()];
        stream
            .read_exact(source_name_buf.as_mut_slice())
            .await
            .unwrap();
        let source_name = PathBuf::from(OsStr::from_bytes(&source_name_buf));

        stream
            .write_u8(WorkerStatus::NeedOverlay as u8)
            .await
            .unwrap();

        let source_data_len = stream.read_u64().await.unwrap();
        let mut source_data_buf = vec![0u8; source_data_len.try_into().unwrap()];
        stream
            .read_exact(source_data_buf.as_mut_slice())
            .await
            .unwrap();

        source_data.insert(source_name, source_data_buf);
    }
    source_data
}

pub async fn write_overlay(stream: &mut TcpStream, path: &Path, data: &[u8]) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForOverlay as u8
    );
    let path = path.as_os_str().as_bytes().to_vec();
    stream
        .write_u16(path.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(&path).await.unwrap();
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::NeedOverlay as u8
    );
    stream
        .write_u64(data.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(data).await.unwrap();
}

pub async fn finish_overlays(stream: &mut TcpStream) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForOverlay as u8
    );
    stream.write_u16(0).await.unwrap();
}

pub async fn finish_deps(stream: &mut TcpStream) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForDep as u8
    );
    stream.write_u16(0).await.unwrap();
}

pub async fn read_envs(stream: &mut TcpStream) -> BTreeMap<String, String> {
    let mut env_data = BTreeMap::new();

    stream
        .write_u8(WorkerStatus::ReadyForEnvs as u8)
        .await
        .unwrap();
    let env_count = stream.read_u16().await.unwrap();
    for _ in 0..env_count {
        let env_k_len = stream.read_u16().await.unwrap();
        let mut env_k_buf = vec![0u8; env_k_len.into()];
        stream.read_exact(env_k_buf.as_mut_slice()).await.unwrap();
        let env_k = String::from_utf8(env_k_buf).unwrap();
        let env_v_len = stream.read_u16().await.unwrap();
        let mut env_v_buf = vec![0u8; env_v_len.into()];
        stream.read_exact(env_v_buf.as_mut_slice()).await.unwrap();
        let env_v = String::from_utf8(env_v_buf).unwrap();
        env_data.insert(env_k, env_v);
    }

    env_data
}

pub async fn write_envs(stream: &mut TcpStream, envs: BTreeMap<String, String>) {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForEnvs as u8
    );
    stream
        .write_u16(envs.len().try_into().unwrap())
        .await
        .unwrap();
    for (k, v) in envs {
        let k = k.as_bytes().to_vec();
        stream.write_u16(k.len().try_into().unwrap()).await.unwrap();
        stream.write_all(&k).await.unwrap();
        let v = v.as_bytes().to_vec();
        stream.write_u16(v.len().try_into().unwrap()).await.unwrap();
        stream.write_all(&v).await.unwrap();
    }
}

pub async fn write_archive(stream: &mut TcpStream, hash: &str, archive: &[u8]) {
    stream
        .write_u8(WorkerStatus::BuildComplete as u8)
        .await
        .unwrap();
    assert_eq!(hash.len(), 64);
    stream.write_all(hash.as_bytes()).await.unwrap();
    stream
        .write_u64(archive.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(archive).await.unwrap();
}
