#![allow(unreachable_code)]

use {
    core::{
        cell::RefCell,
        fmt::{self, Formatter},
    },
    git2::{Branch, Commit, MergeOptions, Oid, Repository},
    log::{info, trace},
    std::collections::{HashMap, HashSet},
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
    pub branches: &'a [Branch<'a>],
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

    info!("Detecting forks...");
    let forks = {
        let mut visited = HashSet::new();
        let mut forks = HashMap::new();

        for current_parent in commits
            .iter()
            .map(Some)
            .chain([None].iter().copied())
            .collect::<Vec<_>>()
            .windows(2)
            .rev()
        {
            let (current, parents) = match current_parent {
                [Some(current), parent] => (
                    current,
                    current.commit.parents().filter(move |p| {
                        if let Some(parent) = parent {
                            p.id() != parent.commit.id()
                        } else {
                            true
                        }
                    }),
                ),
                _ => unreachable!(),
            };
            visited.insert(current.commit.id());
            trace!(
                " Checking parents of {} on branch {1}...",
                current.commit.id(),
                *current.branch_index.borrow()
            );
            for parent in parents {
                visit(
                    parent,
                    &mut visited,
                    *current.branch_index.borrow(),
                    &mut forks,
                );
                fn visit(
                    commit: Commit,
                    visited: &mut HashSet<Oid>,
                    branch_index: usize,
                    forks: &mut HashMap<Oid, usize>,
                ) {
                    if visited.insert(commit.id()) {
                        trace!("  Found side chain commit {}.", commit.id());
                        for parent in commit.parents() {
                            visit(parent, visited, branch_index, forks)
                        }
                    } else {
                        trace!("  Found fork commit {}.", commit.id());
                        // Fork found.
                        // Only the ones that are actually on the edited chain are interesting here, but the overhead shouldn't be too bad.
                        // Larger branch_index equals a more senior branch, which is necessary here to make sure changes stay where they should.
                        //TODO: This doesn't properly handle side chain forks yet, though.
                        if let Some(old_value) = forks.insert(commit.id(), branch_index) {
                            if old_value > branch_index {
                                *forks.get_mut(&commit.id()).unwrap() = old_value
                            }
                        }
                    }
                }
            }
        }
        forks
    };

    if backup {
        todo!("Backup!");
    }

    let mut heads = vec![None; branches.len()];
    let mut map = HashMap::new();
    let mut inverse_map = HashMap::new();
    let mut branch_map_overlays = vec![HashMap::new(); branches.len()];
    let mut dirty = vec![false; branches.len()];

    info!("Transforming history...");

    #[allow(clippy::for_loop_over_option)]
    for BackportCommit {
        commit: oldest,
        branch_index,
    } in commits.last()
    {
        // Always unchanged.
        map.insert(oldest.id(), oldest.clone());
        inverse_map.insert(oldest.id(), oldest.clone());
        heads[*branch_index.borrow()] = Some(oldest.clone());
        for dirty in dirty[0..*branch_index.borrow()].iter_mut() {
            *dirty = true;
        }
    }

    fn catch_up_branch<'a>(
        branch_index: usize,
        branches: &[Branch],
        heads: &mut [Option<Commit<'a>>],
        inverse_map: &mut HashMap<Oid, Commit<'a>>,
        branch_map_overlays: &mut [HashMap<Oid, Commit<'a>>],
        dirty: &mut [bool],
        repository: &'a Repository,
    ) -> Oid {
        if branch_index == branches.len() - 1 || !dirty[branch_index] {
            return inverse_map[&heads[branch_index].as_ref().unwrap().id()].id();
        }
        let original_commit_id = catch_up_branch(
            branch_index + 1,
            branches,
            heads,
            inverse_map,
            branch_map_overlays,
            dirty,
            repository,
        );
        trace!("Catching up branch {}...", branch_index);
        heads[branch_index] = Some(match heads[branch_index].as_ref() {
            None => heads[branch_index + 1].as_ref().unwrap().clone(),
            Some(head) => {
                let mut merge_index = repository
                    .merge_commits(
                        head,
                        heads[branch_index + 1].as_ref().unwrap(),
                        Some(
                            MergeOptions::new()
                                .find_renames(true)
                                .fail_on_conflict(true)
                                .minimal(true),
                        ),
                    )
                    .expect(
                        "This should never fail, since the changes were compatible to begin with.",
                    );
                let merge_oid = merge_index.write_tree().unwrap();
                let merge_tree = repository.find_tree(merge_oid).unwrap();
                let signature = repository
                    .signature()
                    .expect("Could not create default signature");
                let merge_commit_id = repository
                    .commit(
                        None,
                        &signature,
                        &signature,
                        &format!(
                            "Merge {} into {}",
                            branches[branch_index + 1].name().unwrap().unwrap(),
                            branches[branch_index].name().unwrap().unwrap(),
                        ),
                        &merge_tree,
                        &[head, heads[branch_index + 1].as_ref().unwrap()],
                    )
                    .unwrap();
                repository.find_commit(merge_commit_id).unwrap()
            }
        });
        assert!(branch_map_overlays[branch_index]
            .insert(
                original_commit_id,
                heads[branch_index].as_ref().unwrap().clone()
            )
            .is_none());
        assert!(inverse_map
            .insert(
                heads[branch_index].as_ref().unwrap().id(),
                repository.find_commit(original_commit_id).unwrap()
            )
            .is_none());
        dirty[branch_index] = false;
        original_commit_id
    }

    for commit_parent in commits.windows(2).rev() {
        let (commit, parent) = match commit_parent {
            [commit, parent] => (commit, parent),
            _ => unreachable!(),
        };
        catch_up_branch(
            *commit.branch_index.borrow(),
            branches,
            heads.as_mut_slice(),
            &mut inverse_map,
            branch_map_overlays.as_mut_slice(),
            dirty.as_mut_slice(),
            repository,
        );

        todo!("Cherry-pick while mapping parent commits (especially side chains)");
        
        for dirty in dirty[0..*commit.branch_index.borrow()].iter_mut() {
            *dirty = true;
        }
    }

    catch_up_branch(
        0,
        branches,
        heads.as_mut_slice(),
        &mut inverse_map,
        branch_map_overlays.as_mut_slice(),
        dirty.as_mut_slice(),
        repository,
    );

    info!("Setting branches...");
    for (branch, head) in branches.iter().zip(heads.into_iter()) {
        repository
            .branch(branch.name().unwrap().unwrap(), &head.unwrap(), true)
            .unwrap();
    }

    Ok(())
}
