use bootstrapper::{
    env_substitute,
    network::{
        client::ClientRequest,
        worker::{
            finish_deps, finish_overlays, finish_sources, write_dep, write_envs, write_overlay,
            write_source, WorkerStatus,
        },
    },
    recipe::{
        get_depd_hash_from_recipe, get_eq_depd_hash_from_depd_hash, get_eq_depd_hash_from_recipe, get_equiv_hash_from_depd_hash, DepdRecipeHash, EquivHash, HydratedRecipeVersion, EQUIV_DB, RECIPES
    },
    source::{fetch_source, source_path},
    CLIENT_PORT, WORKER_PORT,
};
use std::{collections::BTreeMap, panic, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::sleep,
};
use tracing::{debug, info};

// fn ready_to_build(
//     deptree: &BTreeMap<(String, String), BTreeSet<(String, String)>>,
// ) -> BTreeSet<&(String, String)> {
//     deptree
//         .iter()
//         .filter_map(|(k, v)| if v.is_empty() { Some(k) } else { None })
//         .collect()
// }
// fn finish_dep(
//     deptree: &mut BTreeMap<(String, String), BTreeSet<(String, String)>>,
//     dep: &(String, String),
// ) {
//     deptree.remove(dep);
//     deptree.values_mut().for_each(|v| {
//         v.remove(dep);
//     });
// }

fn dep_path(hash: &EquivHash) -> PathBuf {
    PathBuf::from("build-cache")
        .join("build")
        .join(&hash.0[0..2])
        .join(&hash.0[2..4])
        .join(hash.0.clone())
}

// fn test_dep(recipe_hash: &EqDepdRecipeHash) -> bool {
//     let equiv_hash = get_equiv_hash_from_eq_depd(recipe_hash);
//     if let Some(hash) = equiv_hash {
//         std::fs::exists(dep_path(&hash)).unwrap()
//     } else {
//         false
//     }
// }

fn load_dep(equiv_hash: &EquivHash) -> Vec<u8> {
    std::fs::read(dep_path(equiv_hash)).unwrap()
}

fn store_dep(recipe_hash: &DepdRecipeHash, contents: &[u8]) {
    let equiv_hash = sha256::digest(contents);
    let dep_path = dep_path(&EquivHash(equiv_hash.clone()));
    std::fs::create_dir_all(dep_path.parent().unwrap()).unwrap();
    std::fs::write(dep_path, contents).unwrap();
    let eq_depd = get_eq_depd_hash_from_depd_hash(recipe_hash).unwrap();
    EQUIV_DB.insert(eq_depd.0.clone(), equiv_hash.as_str()).unwrap();
    info!(
        "Storing equiv cache {:?}->{:?}",
        eq_depd,
        EquivHash(equiv_hash)
    );
}

async fn handle_worker_conn(
    mut stream: TcpStream,
    worker_semaphore: Arc<tokio::sync::Semaphore>,
    work_queue: async_channel::Receiver<(HydratedRecipeVersion, OwnedSemaphorePermit)>,
    in_progress: Arc<lockfree::set::Set<DepdRecipeHash>>,
) {
    let worker_semaphore_permit = SemaphorePermitWrapper::new(worker_semaphore);
    while let Ok((to_build, permit)) = work_queue.recv().await {
        let hash = get_depd_hash_from_recipe(&to_build);
        if get_equiv_hash_from_depd_hash(&hash).is_none()
            && in_progress.insert(hash.clone()).is_ok()
        {
            // If err, someone else is already handling this recipe, continue to the next one.
            let archive_buf = build_recipe(&mut stream, to_build).await;
            store_dep(&hash, &archive_buf);
            std::mem::drop(permit);
        }
    }
    info!("Done, Releasing worker semaphore");
    std::mem::drop(worker_semaphore_permit);
}
async fn handle_client_conn(
    mut stream: TcpStream,
    request_queue: tokio::sync::mpsc::UnboundedSender<HydratedRecipeVersion>,
) {
    // let mut waiting_for_deps = BTreeMap::new();
    // let mut ready_to_run = BTreeSet::new();
    loop {
        match ClientRequest::try_from(stream.read_u8().await.unwrap()).unwrap() {
            ClientRequest::AddRecipe => {
                let recipe_len = stream.read_u64().await.unwrap();
                let mut recipe_buf = vec![0; recipe_len.try_into().unwrap()];
                stream.read_exact(&mut recipe_buf).await.unwrap();
                let recipe: HydratedRecipeVersion = serde_yaml::from_slice(&recipe_buf).unwrap();

                // if get_eq_depd_hash_from_recipe(&recipe).is_some() {
                //     ready_to_run.insert(recipe.clone());
                // } else {
                //     let needed_deps: BTreeSet<DepdRecipeHash> = recipe.deps.iter().cloned().filter(|x| test_eq_hash_from_depd_hash(&x.hash)).map(|x| x.hash).collect();
                //     waiting_for_deps.insert(recipe.clone(),needed_deps);
                // }
                RECIPES.insert(get_depd_hash_from_recipe(&recipe), recipe.clone());
                request_queue.send(recipe).unwrap();
            }
            ClientRequest::GetStatus => {
                // println!("{:?}",waiting_for_deps);
                // println!("{:?}",ready_to_run);
                // if let Some(v) = ready_to_run.pop_first() {
                //     let waiter = complete_notifier.notified();
                //     work_queue.send(v.clone()).await.unwrap();
                //     waiter.await;
                //     stream.write_u8(1).await.unwrap();
                //     waiting_for_deps.iter_mut().for_each(|(_,deps)| {
                //         deps.remove(&get_depd_hash_from_recipe(&v));
                //     });
                //     waiting_for_deps.retain(|hrv,deps|{
                //         if deps.is_empty() {
                //             ready_to_run.insert(hrv.clone());
                //             false
                //         } else {
                //             true
                //         }
                //     });
                // } else {
                //     stream.write_u8(0).await.unwrap();
                // }
            }
            ClientRequest::GetHash => {
                todo!();
                // let mut hash = [0;64];
                // stream.read_exact(&mut hash).await.unwrap();
                // if let Some(equiv) = get_equiv_hash(std::str::from_utf8(&hash).unwrap()) {
                //     stream.write_u8(1).await.unwrap();
                //     hash.copy_from_slice(equiv.as_bytes());
                //     stream.write_all(&hash).await.unwrap();
                // } else {
                //     stream.write_u8(0).await.unwrap();
                // }
            }
            ClientRequest::Goodbye => {
                break;
            }
        }
    }
}

async fn circulate_tasks(
    mut request_queue: tokio::sync::mpsc::UnboundedReceiver<HydratedRecipeVersion>,
    worker_semaphore: Arc<tokio::sync::Semaphore>,
    work_queue: async_channel::Sender<(HydratedRecipeVersion, OwnedSemaphorePermit)>,
) {
    let work_to_do = lockfree::set::Set::new();
    let work_notifier = tokio::sync::Notify::new();
    tokio::join!(
        async {
            while let Some(v) = request_queue.recv().await {
                debug!("got request {:?}", v);
                let _ = work_to_do.insert(v);
                work_notifier.notify_waiters();
            }
        },
        async {
            loop {
                info!("Trying to get worker semaphore...");
                let work_permit = worker_semaphore.clone().acquire_owned().await.unwrap();
                let mut sent_work = false;
                let work_wakeup = work_notifier.notified();
                info!("Looking for work...");
                for recipe in work_to_do.iter() {
                    if get_eq_depd_hash_from_recipe(&recipe).is_some() {
                        info!("Dispatched {}:{} to workers", recipe.name, recipe.version);
                        work_queue
                            .send((recipe.clone(), work_permit))
                            .await
                            .unwrap();
                        sent_work = true;
                        work_to_do.remove(&recipe);
                        break;
                    }
                }
                if !sent_work {
                    info!("No work to do! Waiting until notified by work inserter");
                    work_wakeup.await;
                }
            }
        }
    );
}

struct SemaphorePermitWrapper {
    semaphore: Arc<Semaphore>,
}

impl SemaphorePermitWrapper {
    pub fn new(semaphore: Arc<Semaphore>) -> Self {
        info!("Incrementing semaphore");
        semaphore.add_permits(1);
        Self { semaphore }
    }
}

impl Drop for SemaphorePermitWrapper {
    fn drop(&mut self) {
        info!("Decrementing semaphore");
        self.semaphore.forget_permits(1);
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let worker_listener = TcpListener::bind(("0.0.0.0", WORKER_PORT)).await.unwrap();
    let client_listener = TcpListener::bind(("0.0.0.0", CLIENT_PORT)).await.unwrap();

    let (request_queue_tx, request_queue_rx) = tokio::sync::mpsc::unbounded_channel();
    let (work_queue_tx, work_queue_rx) = async_channel::unbounded();
    let worker_semaphore = Arc::new(tokio::sync::Semaphore::new(0));

    let in_progress = Arc::new(lockfree::set::Set::new());
    let in_progress_worker = in_progress.clone();

    let worker_semaphore_clone = worker_semaphore.clone();
    let worker_jh = tokio::spawn(async move {
        info!("Waiting for worker connections...");
        while let Ok((stream, _)) = worker_listener.accept().await {
            info!("Worker joined...");
            tokio::spawn(handle_worker_conn(
                stream,
                worker_semaphore_clone.clone(),
                work_queue_rx.clone(),
                in_progress_worker.clone(),
            ));
        }
    });

    let client_jh = tokio::spawn(async move {
        info!("Waiting for client connections...");
        while let Ok((stream, _)) = client_listener.accept().await {
            tokio::spawn(handle_client_conn(stream, request_queue_tx.clone()));
        }
    });

    let circulator_jh = tokio::spawn(async move {
        circulate_tasks(request_queue_rx, worker_semaphore, work_queue_tx).await
    });

    tokio::try_join!(worker_jh, client_jh, circulator_jh).unwrap();
}

async fn build_recipe(stream: &mut TcpStream, to_build: HydratedRecipeVersion) -> Vec<u8> {
    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::ReadyForWork as u8
    );

    stream.write_u8(0).await.unwrap();

    let recipe_ser = serde_yaml::to_string(&to_build)
        .unwrap()
        .as_bytes()
        .to_vec();
    stream
        .write_u64(recipe_ser.len().try_into().unwrap())
        .await
        .unwrap();
    stream.write_all(&recipe_ser).await.unwrap();

    for (_, source_contents) in to_build.source {
        let spath = source_path(&source_contents.sha);
        let source_data = if spath.exists() {
            std::fs::read(spath).unwrap()
        } else {
            fetch_source(&source_contents).await
        };
        write_source(stream, &source_contents.sha, &source_contents, &source_data).await;
    }
    finish_sources(stream).await;

    for dep in to_build.deps {
        let equiv = get_equiv_hash_from_depd_hash(&dep.hash).unwrap();
        write_dep(stream, &dep.hash, &load_dep(&equiv)).await;
    }
    finish_deps(stream).await;

    for (path, data) in to_build.overlays {
        write_overlay(stream, &path, &data).await;
    }
    finish_overlays(stream).await;

    let mut envs = BTreeMap::new();
    for (k, v) in to_build.envs {
        envs.insert(k.to_owned(), env_substitute(&v, &envs));
    }

    write_envs(stream, envs).await;

    assert_eq!(
        stream.read_u8().await.unwrap(),
        WorkerStatus::BuildComplete as u8
    );
    let mut hash = vec![0u8; 64];
    stream.read_exact(hash.as_mut_slice()).await.unwrap();
    let archive_len = stream.read_u64().await.unwrap();
    let mut archive_buf = vec![0u8; archive_len.try_into().unwrap()];
    stream.read_exact(archive_buf.as_mut_slice()).await.unwrap();

    archive_buf
}
