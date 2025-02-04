use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::PathBuf,
};

use lazy_static::lazy_static;

use serde::{Deserialize, Serialize};

lazy_static! {
    pub static ref SOURCES: BTreeMap<String, SourceContents> = load_sources();
    pub static ref RECIPES: lockfree::map::Map<DepdRecipeHash, HydratedRecipeVersion> =
        lockfree::map::Map::new();
    pub static ref EQUIV_DB: sled::Db = sled::open("equiv.sled").unwrap();
}

pub fn load_sources() -> BTreeMap<String, SourceContents> {
    serde_yaml::from_reader::<File, BTreeMap<String, SourceContents>>(
        File::open("recipes/sources.yaml").unwrap(),
    )
    .unwrap()
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BareRecipeHash(pub String);
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct DepdRecipeHash(pub String);
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EqDepdRecipeHash(pub String);
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EquivHash(pub String);

pub fn generate_recipe_hashes(recipe: &HydratedRecipeVersion) -> (BareRecipeHash,DepdRecipeHash) {
    let depd = DepdRecipeHash(sha256::digest(bincode::serialize(&recipe).unwrap()));
    let mut recipe = recipe.clone();
    recipe.deps = Vec::new();
    let bare = BareRecipeHash(sha256::digest(bincode::serialize(&recipe).unwrap()));
    (bare,depd)
}

pub fn get_bare_recipe_hash_from_recipe(recipe: &HydratedRecipeVersion) -> BareRecipeHash {
    recipe.bare_hash.clone()
}

pub fn get_depd_hash_from_recipe(recipe: &HydratedRecipeVersion) -> DepdRecipeHash {
    recipe.depd_hash.clone()
}

pub fn test_eq_hash_from_depd_hash(recipe_hash: &DepdRecipeHash) -> bool {
    get_equiv_hash_from_depd_hash(recipe_hash).is_some()
}

pub fn get_equiv_hash_from_depd_hash(recipe_hash: &DepdRecipeHash) -> Option<EquivHash> {
    get_equiv_hash_from_recipe(&RECIPES.get(recipe_hash)?.1)
}

pub fn get_eq_depd_hash_from_depd_hash(recipe_hash: &DepdRecipeHash) -> Option<EqDepdRecipeHash> {
    get_eq_depd_hash_from_recipe(&RECIPES.get(recipe_hash)?.1)
}

pub fn get_eq_depd_hash_from_recipe(recipe: &HydratedRecipeVersion) -> Option<EqDepdRecipeHash> {
    lazy_static! {
        static ref EQ_DEPD_CACHE: lockfree::map::Map<DepdRecipeHash,EqDepdRecipeHash> = lockfree::map::Map::new();
    }
    if let Some(v) = EQ_DEPD_CACHE.get(&recipe.depd_hash) {
        return Some(v.1.clone());
    }
    let mut recipe = recipe.clone();
    for i in recipe.deps.iter_mut() {
        *i = HydratedDepSpec {
            hash: DepdRecipeHash(get_equiv_hash_from_recipe(&RECIPES.get(&i.hash)?.1)?.0),
            from: i.from.clone(),
            to: i.to.clone(),
        } // Not the right type here, but we're just stuffing it here until we calculate the hash below.
    }
    let hash = EqDepdRecipeHash(sha256::digest(
        bincode::serialize(&recipe).unwrap(),
    ));
    EQ_DEPD_CACHE.insert(recipe.depd_hash, hash.clone());
    Some(hash)
}

pub fn get_equiv_hash_from_recipe(recipe: &HydratedRecipeVersion) -> Option<EquivHash> {
    get_equiv_hash_from_eq_depd(&get_eq_depd_hash_from_recipe(recipe)?)
}

pub fn get_equiv_hash_from_eq_depd(recipe_hash: &EqDepdRecipeHash) -> Option<EquivHash> {
    lazy_static!{
        static ref EQUIV_CACHE: lockfree::map::Map<EqDepdRecipeHash, EquivHash> =
        lockfree::map::Map::new();
    }
    if let Some(hash) = EQUIV_CACHE.get(recipe_hash) {
        return Some(hash.1.clone());
    }
    let res = EQUIV_DB
        .get(recipe_hash.0.clone())
        .unwrap()
        .map(|x| EquivHash(String::from_utf8(x.to_vec()).unwrap()));
    if let Some(hash) = &res {
        EQUIV_CACHE.insert(recipe_hash.clone(), hash.to_owned());
    }
    //println!("{:?} -> {:?}",recipe_hash,res);
    res
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Source {
    pub extract: Option<String>,
    pub noextract: Option<String>,
    pub copy: Option<Vec<String>>,
    pub chmod: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SourceContents {
    pub url: String,
    pub sha: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(untagged)]
pub enum RecipeBuildSteps {
    Single {
        single: Vec<RecipeBuildStep>,
    },
    Piecewise {
        unpack: Option<Vec<RecipeBuildStep>>,
        unpack_dirname: String,
        patch_dir: String,
        package_dir: Option<String>,
        prepare: Option<Vec<RecipeBuildStep>>,
        configure: Option<Vec<RecipeBuildStep>>,
        compile: Option<Vec<RecipeBuildStep>>,
        install: Option<Vec<RecipeBuildStep>>,
        postprocess: Option<Vec<RecipeBuildStep>>,
        #[serde(default = "_default_true")]
        checksum: bool,
    },
}

const fn _default_true() -> bool {
    true
}

const fn _default_false() -> bool {
    false
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(untagged)]
pub enum RecipeBuildStep {
    Simple(String),
    Complex {
        cmd: String,
        #[serde(default = "_default_true")]
        serial: bool,
        #[serde(default = "_default_false")]
        bash: bool,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DepSpec {
    pub name: String,
    pub version: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl From<String> for DepSpec {
    fn from(s: String) -> Self {
        let mut dep_iter = s.split(':');
        let name = dep_iter.next().unwrap().to_string();
        let version = dep_iter.next().unwrap().to_string();
        let from = dep_iter.next().map(str::to_string);
        let to = dep_iter.next().map(str::to_string);
        Self {
            name,
            version,
            from,
            to,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct HydratedDepSpec {
    pub hash: DepdRecipeHash,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Owner {
    Single(String),
    Multiple(Vec<String>),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct License {
    spdx: String,
    owner: Owner,
    license_file: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Licenses {
    recipe: Option<License>,
    package: Option<License>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RecipeVersion {
    pub licenses: Option<Licenses>,
    pub source: Option<BTreeMap<String, Source>>,
    pub deps: Option<Vec<String>>,
    pub mkdirs: Option<Vec<String>>,
    pub build: RecipeBuildSteps,
    pub artefacts: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NamedRecipeVersion {
    pub name: String,
    pub version: String,
    pub source: Option<BTreeMap<String, Source>>,
    pub deps: Option<Vec<DepSpec>>,
    pub mkdirs: Option<Vec<String>>,
    pub build: RecipeBuildSteps,
    pub artefacts: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct HydratedRecipeVersion {
    pub name: String,
    pub version: String,
    pub source: Vec<(Source, SourceContents)>,
    pub deps: Vec<HydratedDepSpec>,
    pub mkdirs: Vec<String>,
    pub build: RecipeBuildSteps,
    pub artefacts: Vec<String>,
    pub envs: Vec<(String, String)>,
    pub overlays: Vec<(PathBuf, Vec<u8>)>,
    pub bare_hash: BareRecipeHash,
    pub depd_hash: DepdRecipeHash,
}

impl From<(String, String, RecipeVersion)> for NamedRecipeVersion {
    fn from((name, version, recipe): (String, String, RecipeVersion)) -> Self {
        Self {
            name,
            version,
            source: recipe.source,
            deps: recipe
                .deps
                .map(|x| x.into_iter().map(|x| x.into()).collect()),
            mkdirs: recipe.mkdirs,
            build: recipe.build,
            artefacts: recipe.artefacts,
        }
    }
}

impl NamedRecipeVersion {
    pub fn load_by_name(name: &str) -> Self {
        let (target, version) = name.split_once(':').unwrap();
        Self::load_by_target_version(target, version)
    }
    pub fn load_by_target_version(target: &str, version: &str) -> Self {
        let rv: RecipeVersion = serde_yaml::from_reader(
            std::fs::File::open(
                PathBuf::from("recipes")
                    .join(target)
                    .join(format!("{}.yaml", version)),
            )
            .unwrap(),
        )
        .unwrap();
        NamedRecipeVersion {
            name: target.to_owned(),
            version: version.to_owned(),
            source: rv.source,
            deps: rv.deps.map(|x| x.into_iter().map(|x| x.into()).collect()),
            mkdirs: rv.mkdirs,
            build: rv.build,
            artefacts: rv.artefacts,
        }
    }
}
