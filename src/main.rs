use std::{
    borrow::Cow,
    env::{self, VarError},
};

use structopt::StructOpt;
use writ::ui::{run_command, Opt};

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
    run_command(opt)
}
