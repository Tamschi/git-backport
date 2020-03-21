use {
    core::{
        cell::RefCell,
        fmt::{self, Formatter},
    },
    git2::{Branch, Commit, Repository},
    log::{debug, info, trace},
};

#[derive(Debug)]
pub enum Error {}
impl std::error::Error for Error {}
impl core::fmt::Display for Error {
    fn fmt(&self, _: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        Ok(())
    }
}

pub struct BackportArgs<
    'a,
    E: for<'b> FnOnce(&'b [Branch<'b>], &[(Commit, RefCell<&'b Branch<'b>>)]),
> {
    pub repository: &'a Repository,
    pub backup: bool,
    pub branches: Vec<Branch<'a>>,
    pub edit: E,
}
pub fn backport<E: for<'a> FnOnce(&'a [Branch<'a>], &[(Commit, RefCell<&'a Branch<'a>>)])>(
    BackportArgs {
        repository,
        backup,
        branches,
        edit,
    }: BackportArgs<E>,
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
            commits.push((current_commit, RefCell::new(current)));
            trace!(
                "Found commit: {} on {}",
                commits[commits.len() - 1].0.id(),
                commits[commits.len() - 1]
                    .1
                    .borrow()
                    .name()
                    .unwrap()
                    .unwrap(),
            );
            current_commit = parent_commit;
        }
    }
    edit(&branches, &commits);
    Ok(())
}
