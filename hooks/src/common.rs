use std::{fs, path::Path};
pub(crate) enum StorageType {
    Internal,
    External,
}
// before you shred me with "you dont handle errors properly??
// what happens if it errors out and crashes minecraft???"
// And the answer to that is that this is experimental for a reason.
pub(crate) fn parse_options(path: &Path) -> StorageType {
    let file = fs::read_to_string(path).expect("expected options.txt to exist");
    let parsed_yaml: serde_yaml::Value = serde_yaml::from_str(&file).expect("expected yaml");
    let storage_type = parsed_yaml
        .get("dvce_filestoragelocation")
        .expect("expected storagetype");
    let storage_type_int = storage_type
        .as_u64()
        .expect("dvce_filestoragelocation isnt a u64");
    match storage_type_int {
        1 => StorageType::External,
        2 => StorageType::Internal,
        _ => {
            log::warn!("Uhoh so we cant tell where storage is at so we will guess internal");
            StorageType::Internal
        }
    }
}
