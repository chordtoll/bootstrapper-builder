use std::collections::{BTreeMap, BTreeSet};

use bootstrapper::{
    client::{hydrate_recipe, load_recipes},
    network::client::ClientRequest,
    recipe::{DepSpec, NamedRecipeVersion},
    CLIENT_PORT,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let recipes = load_recipes();

    let mut stream = TcpStream::connect(("127.0.0.1", CLIENT_PORT))
        .await
        .unwrap();

    let mut hydrated_recipes = BTreeMap::new();

    let mut to_hydrate = BTreeSet::new();

    for (name, recipe) in &recipes {
        for version in recipe.keys() {
            to_hydrate.insert((name.to_owned(), version.to_owned()));
        }
    }

    while !to_hydrate.is_empty() {
        let mut hydrated = false;
        to_hydrate.retain(|(name, version)| {
            if let Some(n) = recipes.get(name) {
                if let Some(recipe) = n.get(version) {
                    let nrv = NamedRecipeVersion::from((
                        name.to_owned(),
                        version.to_owned(),
                        recipe.to_owned(),
                    ));
                    for dep in recipe.deps.clone().unwrap_or_default() {
                        let spec = DepSpec::from(dep);
                        if !hydrated_recipes.contains_key(&(spec.name, spec.version)) {
                            return true;
                        }
                    }
                    info!("Hydrating {}:{}", name, version);
                    hydrated_recipes.insert(
                        (name.to_owned(), version.to_owned()),
                        hydrate_recipe(nrv, &hydrated_recipes).unwrap(),
                    );
                    hydrated = true;
                    return false;
                }
            }
            todo!();
        });
        if !hydrated {
            todo!();
        }
    }

    for (name, recipe) in recipes {
        for (version, _) in recipe {
            info!("Sending to server {}:{}", name, version);
            let hydrated = hydrated_recipes.get(&(name.clone(), version)).unwrap();
            stream
                .write_u8(ClientRequest::AddRecipe.into())
                .await
                .unwrap();
            let recipe_buf = serde_yaml::to_string(&hydrated)
                .unwrap()
                .as_bytes()
                .to_owned();
            stream
                .write_u64(recipe_buf.len().try_into().unwrap())
                .await
                .unwrap();
            stream.write_all(&recipe_buf).await.unwrap();
        }
    }

    loop {
        info!("Checking status...");
        stream
            .write_u8(ClientRequest::GetStatus.into())
            .await
            .unwrap();
        if stream.read_u8().await.unwrap() == 0 {
            break;
        }
    }
    // stream.write_u8(ClientRequest::GetHash.into()).await.unwrap();
    // let recipe_hash = get_recipe_hash(hydrated);
    // stream.write_all(recipe_hash.as_bytes()).await.unwrap();
    // if stream.read_u8().await.unwrap() == 1 {
    //     let mut hash = [0;64];
    //     stream.read_exact(&mut hash).await.unwrap();
    //     println!("{:?}",hash);
    // } else {
    //     println!("No hash");
    // }
    stream
        .write_u8(ClientRequest::Goodbye.into())
        .await
        .unwrap();
    return;
}
