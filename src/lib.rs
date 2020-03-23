use {
    core::{
        cell::RefCell,
        fmt::{self, Formatter},
    },
    git2::{Branch, Commit, Oid, Repository},
    log::{info, trace},
    std::collections::HashSet,
};

#[derive(Debug)]
pub enum Error {}
impl std::error::Error for Error {}
impl core::fmt::Display for Error {
    fn fmt(&self, _: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        Ok(())
    }
}

pub struct BackportCommit<'a> {
    pub commit: Commit<'a>,
    pub branch_index: RefCell<usize>,
}

pub struct BackportArgs<'a, E: FnOnce(&[Branch], &[BackportCommit])> {
    pub repository: &'a Repository,
    pub backup: bool,
    pub branches: Vec<Branch<'a>>,
    pub edit: E,
}
pub fn backport<E: FnOnce(&[Branch], &[BackportCommit])>(
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
    'branch: for (current_index, window) in branches.windows(2).enumerate() {
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
            trace!(
                "Found commit: {} on {}",
                current_commit.id(),
                current.name().unwrap().unwrap(),
            );
            let parent_commit = if current_commit.parent_count() == 1 {
                current_commit.parent(0).unwrap()
            } else {
                trace!(
                    "Found {} parents. Scanning...",
                    current_commit.parent_count()
                );
                let mut visited = HashSet::new();
                let matching_parents = current_commit
                    .parents()
                    .rev() // The commit we're looking for tends to be on the merged-in branch, at least with my workflow. -TS
                    .filter(|p| {
                        fn is_or_has_ancestor(
                            c: &Commit,
                            id: Oid,
                            visited: &mut HashSet<Oid>,
                        ) -> bool {
                            visited.insert(c.id())
                                && (c.id() == id
                                    || c.parents()
                                        .rev()
                                        .any(|p| is_or_has_ancestor(&p, id, visited)))
                        };
                        is_or_has_ancestor(p, parent_branch_id, &mut visited)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    matching_parents.len(),
                    1,
                    "Ambiguous parents found. The next ancestor must be reachable via only one parent in each commit."
                );
                matching_parents.into_iter().next().unwrap()
            };
            commits.push(BackportCommit {
                commit: current_commit,
                branch_index: RefCell::new(current_index),
            });
            current_commit = parent_commit;
        }
    }
    edit(&branches, &commits);
    Ok(())
}
