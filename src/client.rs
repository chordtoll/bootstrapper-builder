use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::PathBuf,
};

use maplit::btreemap;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::recipe::{
    generate_recipe_hashes, get_depd_hash_from_recipe, BareRecipeHash, DepdRecipeHash, HydratedDepSpec, HydratedRecipeVersion, NamedRecipeVersion, RecipeVersion, SOURCES
};

pub fn load_recipes() -> BTreeMap<String, BTreeMap<String, RecipeVersion>> {
    info!("Loading recipes...");
    let mut recipes = BTreeMap::new();
    for entry in glob::glob("recipes/*/**/*.yaml").unwrap() {
        let entry = entry.unwrap();
        let name = entry
            .parent()
            .unwrap()
            .strip_prefix("recipes")
            .unwrap()
            .as_os_str()
            .to_str()
            .unwrap();
        let version = entry
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_end_matches(".yaml");
        let recipe: RecipeVersion =
            serde_yaml::from_reader(File::open(entry.clone()).unwrap()).unwrap();

        if recipe.licenses.is_none() {
            warn!("No license for {}:{}", name, version);
        }

        match recipes.entry(name.to_owned()) {
            std::collections::btree_map::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(btreemap! {version.to_owned()=>recipe});
            }
            std::collections::btree_map::Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().insert(version.to_owned(), recipe);
            }
        }
    }
    recipes
}

pub fn hydrate_recipe(
    recipe: NamedRecipeVersion,
    recipes: &BTreeMap<(String, String), HydratedRecipeVersion>,
) -> Option<HydratedRecipeVersion> {
    let mut sources = Vec::new();
    if let Some(recipe_sources) = recipe.source {
        for (name, source) in recipe_sources {
            sources.push((source, SOURCES.get(&name).unwrap().to_owned()));
        }
    }

    let mut overlays = Vec::new();
    let overlay_path = PathBuf::from(format!("recipes/{}/{}", recipe.name, recipe.version));
    if overlay_path.exists() {
        for entry in WalkDir::new(&overlay_path) {
            let entry = entry.unwrap();
            if entry.metadata().unwrap().is_file() {
                overlays.push((
                    entry.path().strip_prefix(&overlay_path).unwrap().to_owned(),
                    std::fs::read(entry.path()).unwrap(),)
                );
            }
        }
    }

    let mut envs = Vec::new();
    if let Ok(v) = std::fs::read(
        PathBuf::from(format!("recipes/{}.yaml", recipe.name))
            .parent()
            .unwrap()
            .join("env"),
    ) {
        for line in String::from_utf8(v).unwrap().split('\n') {
            let (k, v) = line.split_once('=').unwrap();
            envs.push((k.to_owned(), v.trim_matches('"').to_owned()));
        }
    };

    let deps = if let Some(deps) = recipe.deps {
        deps.into_iter()
            .map(|x| HydratedDepSpec {
                hash: get_depd_hash_from_recipe(recipes.get(&(x.name, x.version)).unwrap()),
                from: x.from,
                to: x.to,
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut recipe = HydratedRecipeVersion {
        name: recipe.name,
        version: recipe.version,
        source: sources,
        deps,
        mkdirs: recipe.mkdirs.unwrap_or_default(),
        build: recipe.build,
        artefacts: recipe.artefacts,
        envs,
        overlays,
        bare_hash: BareRecipeHash(String::new()),
        depd_hash: DepdRecipeHash(String::new()),
    };

    let (bare_hash,depd_hash) = generate_recipe_hashes(&recipe);
    recipe.bare_hash = bare_hash;
    recipe.depd_hash = depd_hash;
    Some(recipe)
}
