use std::{
    io::{self, Read},
    path::PathBuf,
};

use structopt::StructOpt;
use writ::*;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .init();

    match Opt::from_args() {
        Opt::Init { dir } => init(dir),
        Opt::Commit {
            name,
            email,
            message,
        } => commit(name, email, message),
    }
}

#[derive(StructOpt, Debug, Clone)]
pub enum Opt {
    Init {
        #[structopt(default_value = ".")]
        dir: PathBuf,
    },
    Commit {
        #[structopt(long)]
        name: String,
        #[structopt(long)]
        email: String,
        #[structopt(long, short)]
        message: String,
    },
}
