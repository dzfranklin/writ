use std::{
    borrow::Cow,
    env::{self, VarError},
    path::PathBuf,
};

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
    Status,
    Plumb(Plumb),
}

#[derive(StructOpt, Debug, Clone)]
pub enum Plumb {
    ShowHead,
}

fn main() -> eyre::Result<()> {
    let filter = match env::var("RUST_LOG") {
        Ok(var) => Cow::Owned(var),
        Err(VarError::NotPresent) => Cow::Borrowed("warn"),
        Err(err) => return Err(err.into()),
    };
    let filter = tracing_subscriber::EnvFilter::try_new(filter)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
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
        Opt::Status => eprintln!("{:#?}", Repo::for_current_dir()?.status()?),
        Opt::Plumb(plumb) => plumb_main(plumb)?,
    }

    Ok(())
}

fn plumb_main(opt: Plumb) -> eyre::Result<()> {
    match opt {
        Plumb::ShowHead => Repo::for_current_dir()?.show_head(),
    }
}
