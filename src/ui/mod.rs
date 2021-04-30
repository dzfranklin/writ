use std::{
    fmt,
    path::{Path, PathBuf},
};

use console::style;
use eyre::eyre;
use structopt::StructOpt;
use tracing::debug;

use bstr::ByteSlice;

use crate::core;

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
    Plumb(PlumbOpt),
}

#[derive(StructOpt, Debug, Clone)]
pub enum PlumbOpt {
    ShowHead,
}

pub struct Ui {
    repo: core::Repo,
}

#[macro_export]
macro_rules! println_style {
    ($fmt:literal.$($sty:tt)+) => {{
        let msg = style(format!($fmt)).$($sty)+;
        println!("{}", msg);
    }};
}

impl Ui {
    pub fn new(repo: core::Repo) -> Self {
        Self { repo }
    }
    pub fn for_current_dir() -> eyre::Result<Self> {
        let repo = core::Repo::for_current_dir()?;
        Ok(Self::new(repo))
    }

    pub fn init(workspace: impl Into<PathBuf>) -> eyre::Result<Self> {
        let workspace = workspace.into();

        let workspace_name = workspace.clone();
        let workspace_name = workspace_name.to_string_lossy();

        let repo = core::Repo::init(workspace)?;

        println!("Initialized repository in {}", workspace_name);

        Ok(Self::new(repo))
    }

    pub fn add<I, P>(&mut self, files: I) -> eyre::Result<()>
    where
        I: IntoIterator<Item = P> + fmt::Debug,
        P: AsRef<Path>,
    {
        let added = self.repo.add(files)?;

        if added.is_empty() {
            return Err(eyre!("No files match paths specified"));
        }

        let added = added
            .iter()
            .map(|p| p.as_bstr().to_str_lossy())
            .collect::<Vec<_>>()
            .join(", ");
        println_style!("Added file(s): {added}".green());
        Ok(())
    }

    pub fn commit(
        &mut self,
        name: impl Into<String> + fmt::Debug,
        email: impl Into<String> + fmt::Debug,
        msg: impl Into<String> + fmt::Debug,
    ) -> eyre::Result<()> {
        self.repo.commit(name, email, msg)?;
        println_style!("Committed".green().bold());
        Ok(())
    }

    pub fn status(&mut self) -> eyre::Result<()> {
        let status = self.repo.status()?;

        let mut to_commit = Vec::new();
        let mut not_staged = Vec::new();
        let mut untracked = Vec::new();

        for (path, status) in status {
            if status.workspace == core::Status::Untracked {
                untracked.push(path);
            } else {
                let index = status.index;
                let ws = status.workspace;

                if index != core::Status::Unmodified {
                    to_commit.push((path.clone(), status.index));
                }
                if ws != core::Status::Unmodified {
                    not_staged.push((path, status.workspace));
                }
            }
        }

        if !to_commit.is_empty() {
            println!("Changes to be committed:");
            for (path, status) in to_commit {
                let status = style(status.name());
                println_style!("    {status}: {path}".green());
            }
            println!();
        }

        if !not_staged.is_empty() {
            println!("Changes not staged for commit:");
            for (path, status) in not_staged {
                let status = status.name();
                println_style!("    {status}: {path}".green());
            }
            println!();
        }

        if !untracked.is_empty() {
            println!("Untracked files:");
            for path in untracked {
                println_style!("    {path}".red());
            }
            println!();
        }

        Ok(())
    }

    pub fn plumb_show_head(&mut self) -> eyre::Result<()> {
        let head = self
            .repo
            .refs
            .head()?
            .ok_or_else(|| eyre::eyre!("No HEAD"))?;
        let commit = self.repo.db.load::<core::db::Commit>(head)?;
        eprintln!("HEAD: {}\n", head);
        self.plumb_print_tree(commit.tree, 0)?;
        Ok(())
    }

    fn plumb_print_tree(
        &mut self,
        tree: core::Oid<core::db::Tree>,
        level: usize,
    ) -> eyre::Result<()> {
        let tree = self.repo.db.load::<core::db::Tree>(tree)?;
        let level_prefix = " ".repeat(level * 4);
        for node in tree.direct_children() {
            match node {
                core::db::tree::Node::File(core::db::tree::FileNode { name, mode, oid }) => {
                    println!("{}{} {} ({:?})", level_prefix, oid, name, mode)
                }
                core::db::tree::Node::Tree { name, oid } => {
                    println!("{}{} {}/", level_prefix, oid, name);
                    self.plumb_print_tree(*oid, level + 1)?;
                }
            }
        }
        Ok(())
    }
}

pub fn run_command(opt: Opt) -> eyre::Result<()> {
    debug!("Got opt {:#?}", opt);

    match opt {
        Opt::Init { dir } => {
            Ui::init(dir)?;
        }
        Opt::Add { files } => Ui::for_current_dir()?.add(files)?,
        Opt::Commit {
            name,
            email,
            message,
        } => Ui::for_current_dir()?.commit(name, email, message)?,
        Opt::Status => Ui::for_current_dir()?.status()?,
        Opt::Plumb(plumb) => run_plumb_command(plumb)?,
    }

    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn run_plumb_command(opt: PlumbOpt) -> eyre::Result<()> {
    match opt {
        PlumbOpt::ShowHead => Ui::for_current_dir()?.plumb_show_head(),
    }
}
