use {
    core::fmt::{self, Formatter},
    git2::{Branch, Repository},
    log::{debug, info, trace},
};

#[derive(Debug)]
pub enum Error {}
impl std::error::Error for Error {}
impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        use Error::*;
        match self {
            UNCLEAN => write!(
                fmt,
                "Repository isn't completely clean (including ignored)."
            ),
        }
    }
}

pub struct BackportArgs<'a> {
    pub repository: &'a Repository,
    pub backup: bool,
    pub branches: Vec<Branch<'a>>,
}
pub fn backport(
    BackportArgs {
        repository,
        backup,
        branches,
    }: BackportArgs,
) -> Result<(), Error> {
    info!("Collecting commits...");
    assert!(!branches.is_empty());
    let mut commits = vec![];
    'branch: for window in branches.windows(2) {
        let (current, parent) = if let [current, parent] = window {
            (current, parent)
        } else {
            unreachable!()
        };
        let mut current_commit = current.get().peel_to_commit().unwrap();
        let parent_branch_id = parent.get().peel_to_commit().unwrap().id();
        loop {
            if current_commit.id() == parent_branch_id {
                continue 'branch;
            }
            assert_eq!(
                current_commit.parent_count(),
                1,
                "Only commits with 1 parent are supported, but {} has {}",
                current_commit.id(),
                current_commit.parent_count(),
            );
            let parent_commit = current_commit.parent(0).unwrap();
            commits.push((current_commit, current));
            trace!(
                "Found commit: {:?} on {:?}",
                commits[commits.len() - 1].0.id(),
                commits[commits.len() - 1].1.name(),
            );
            current_commit = parent_commit;
        }
    }
    Ok(())
}
