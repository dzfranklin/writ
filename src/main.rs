use std::{env, path::PathBuf};

use structopt::StructOpt;
use tracing::debug;
use writ::*;

#[derive(StructOpt, Debug, Clone)]
pub enum Opt {
    Init {
        #[structopt(default_value = ".")]
        dir: PathBuf,
    },
    Add {
        files: Vec<PathBuf>,
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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .init();

    let opt = Opt::from_args();

    debug!("Parsed opt {:#?}", opt);

    match opt {
        Opt::Init { dir } => init(dir),
        Opt::Add { files } => add(env::current_dir()?, files),
        Opt::Commit {
            name,
            email,
            message,
        } => commit(env::current_dir()?, name, email, message),
    }
}
