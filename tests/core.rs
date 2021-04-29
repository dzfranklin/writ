#![feature(map_into_keys_values, path_try_exists)]

#[path = "core/add.rs"]
mod add;
#[path = "core/commit.rs"]
mod commit;
#[path = "core/repo_init.rs"]
mod repo_init;
#[path = "core/status.rs"]
mod status;
