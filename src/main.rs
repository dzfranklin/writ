use std::path::PathBuf;

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

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .init();

    color_eyre::install()?;

    let opt = Opt::from_args();

    debug!("Parsed opt {:#?}", opt);

    match opt {
        Opt::Init { dir } => {
            Repo::init(dir)?;
        }
        Opt::Add { files } => Repo::for_current_dir()?.add(files)?,
        Opt::Commit {
            name,
            email,
            message,
        } => Repo::for_current_dir()?.commit(name, email, message)?,
    }

    Ok(())
}
