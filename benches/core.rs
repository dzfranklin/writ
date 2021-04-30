use std::{fs, path::Path, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use duct::cmd;
use lazy_static::lazy_static;
use tempfile::{tempdir, TempDir};
use walkdir::{DirEntry, WalkDir};
use writ::core::*;

// NOTE: This compares the performance of shelling out to git against calling
//   into our library.

fn unpack_sample_workdir() -> TempDir {
    let dir = tempdir().unwrap();

    cmd!(
        "tar",
        "xf",
        "/home/daniel/writ/test_data/git_repo.tar.gz",
        "--strip-components", // place children directly in cwd
        "1",
        "--exclude",
        ".git"
    )
    .dir(dir.path())
    .run()
    .unwrap();

    dir
}

lazy_static! {
    static ref SAMPLE: TempDir = unpack_sample_workdir();
}

fn is_workspace_file(root: &Path, entry: &DirEntry) -> bool {
    entry.file_type().is_file() && !entry.path().strip_prefix(root).unwrap().starts_with(".git")
}

fn setup_workspace(file_count: u64) -> TempDir {
    let ws = tempdir().unwrap();
    let sample = SAMPLE.path();

    let mut total_possible = 0;
    for entry in WalkDir::new(sample) {
        if is_workspace_file(sample, &entry.unwrap()) {
            total_possible += 1;
        }
    }
    let can_skip = total_possible - file_count;

    let mut added = 0;
    let mut skipped = 0;
    for entry in WalkDir::new(sample).sort_by_file_name() {
        if added >= file_count {
            return ws;
        }

        if (skipped as u32) < (can_skip as u32) * (added as u32 / file_count as u32) {
            skipped += 1;
            continue;
        }

        let entry = entry.unwrap();

        if !is_workspace_file(sample, &entry) {
            continue;
        }

        let sample_parent = entry.path().parent().unwrap().strip_prefix(sample).unwrap();
        let ws_parent = ws.path().join(sample_parent);

        fs::create_dir_all(&ws_parent).unwrap();
        fs::copy(entry.path(), ws_parent.join(entry.file_name())).unwrap();

        added += 1;
    }

    panic!("Could not fill workspace: not enough files")
}

fn for_first_n_files(ws: &Path, n: u64, mut f: impl FnMut(&Path)) {
    let mut processed = 0;
    for entry in WalkDir::new(ws).sort_by_file_name() {
        let entry = entry.unwrap();

        if !is_workspace_file(ws, &entry) {
            continue;
        }

        if processed > n {
            return;
        }

        let path = entry.path().strip_prefix(ws).unwrap();
        f(path);

        processed += 1;
    }
}

pub fn bench_add(c: &mut Criterion) {
    let mut group = c.benchmark_group("add");
    for size in (0..100).step_by(20) {
        group.throughput(Throughput::Elements(size));
        group.bench_with_input(BenchmarkId::new("writ", size), &size, |b, &size| {
            b.iter_with_large_setup(
                || {
                    let ws = setup_workspace(size);
                    let repo = Repo::init(ws.path()).unwrap();
                    (ws, repo)
                },
                |(_ws, mut repo)| repo.add(vec!["."]).unwrap(),
            )
        });
        group.bench_with_input(BenchmarkId::new("git", size), &size, |b, &size| {
            b.iter_with_large_setup(
                || {
                    let ws = setup_workspace(size);
                    cmd!("git", "init", ".").dir(ws.path()).read().unwrap();
                    ws
                },
                |ws| {
                    cmd!("git", "add", ".").dir(ws.path()).read().unwrap();
                },
            )
        });
    }
    group.finish();
}

pub fn bench_status(c: &mut Criterion) {
    const WS_FILES: u64 = 500;

    let mut group = c.benchmark_group("status");
    for count in (0..500).step_by(100) {
        group.throughput(Throughput::Elements(count));
        group.measurement_time(Duration::from_secs(240));
        group.bench_with_input(BenchmarkId::new("writ", count), &count, |b, &count| {
            b.iter_with_large_setup(
                || {
                    let ws = setup_workspace(WS_FILES);
                    let mut repo = Repo::init(ws.path()).unwrap();
                    for_first_n_files(ws.path(), count, |path| {
                        assert_eq!(repo.add(vec![path]).unwrap().len(), 1);
                    });
                    (ws, repo)
                },
                |(_ws, mut repo)| {
                    repo.status().unwrap();
                },
            )
        });
        group.bench_with_input(BenchmarkId::new("git", count), &count, |b, &count| {
            b.iter_with_large_setup(
                || {
                    let ws = setup_workspace(WS_FILES);
                    cmd!("git", "init", ".").dir(ws.path()).read().unwrap();
                    for_first_n_files(ws.path(), count, |path| {
                        cmd!("git", "add", path).dir(ws.path()).run().unwrap();
                    });
                    ws
                },
                |ws| {
                    cmd!("git", "status").dir(ws.path()).read().unwrap();
                },
            )
        });
    }
    group.finish();
}

// criterion_group!(core, bench_add, bench_status);
criterion_group!(core, bench_status);
criterion_main!(core);
