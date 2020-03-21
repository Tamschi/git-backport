use {
    console::{measure_text_width, pad_str, truncate_str, Alignment, Key, Term},
    git2::{Branch, BranchType, Repository},
    git_backport::{backport, BackportArgs, Error},
    log::{debug, error},
    std::{io::Write, path::PathBuf},
    structopt::StructOpt,
};

#[derive(Debug, StructOpt)]
#[structopt(
    author,
    about = "\nInteractively backport commits to ancestor branches."
)]
struct Options {
    #[structopt(short, long, default_value = ".", parse(from_os_str))]
    repository: PathBuf,
    #[structopt(short = "B", long)]
    no_backup: bool,
    #[structopt(short, long, default_value = "HEAD")]
    head: String,
    #[structopt(required = true)]
    ancestors: Vec<String>,
}

fn main() {
    let options = Options::from_args();

    simple_logger::init().unwrap();

    let repository = Repository::open(options.repository).unwrap();
    let mut branches = vec![if options.head == "HEAD" {
        let head = repository.head().unwrap();
        assert!(head.is_branch());
        Branch::wrap(head)
    } else {
        repository
            .find_branch(&options.head, BranchType::Local)
            .unwrap()
    }];
    for ancestor in options.ancestors.into_iter() {
        let ancestor = repository
            .find_branch(&ancestor, BranchType::Local)
            .unwrap();
        branches.push(ancestor);
    }
    debug!(
        "Branches specified: {}",
        (&branches
            .iter()
            .map(|b| b.name().unwrap().unwrap())
            .collect::<Vec<_>>())
            .join(", ")
    );

    if let Err(error) = backport(BackportArgs {
        repository: &repository,
        backup: !options.no_backup,
        branches,
        edit: |branches, commits| {
            let mut out = Term::stdout();
            let mut cursor = 0;
            let (_, width) = out.size();
            let width = width as usize;
            dbg!(width);
            loop {
                for (i, (commit, branch)) in commits.iter().enumerate() {
                    let branch_index = branches
                        .iter()
                        .enumerate()
                        .find_map(|(i, branch_i)| {
                            if branch_i as *const Branch == *branch.borrow() as *const Branch {
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    out.write_all(pad_str("", branch_index, Alignment::Left, None).as_bytes())
                        .unwrap();
                    out.write_all(if cursor == i { b">" } else { b" " })
                        .unwrap();
                    out.write_all(truncate_str(&commit.id().to_string(), 8, "").as_bytes())
                        .unwrap();
                    out.write_all(b" ").unwrap();
                    let branch_name =
                        truncate_str(branch.borrow().name().unwrap().unwrap(), width / 2, "...");
                    let branch_name_width = measure_text_width(branch_name.as_ref());
                    out.write_all(branch_name.as_bytes()).unwrap();
                    out.write_all(b" ").unwrap();
                    out.write_line(
                        pad_str(
                            commit
                                .message()
                                .unwrap()
                                .split('\r')
                                .next()
                                .unwrap()
                                .split('\n')
                                .next()
                                .unwrap(),
                            width - (branch_index + 1 + 8 + 1 + branch_name_width + 1),
                            Alignment::Left,
                            Some("..."),
                        )
                        .as_ref(),
                    )
                    .unwrap();
                }
                {
                    let branch_index = branches
                        .iter()
                        .enumerate()
                        .find_map(|(i, branch_i)| {
                            if branch_i as *const Branch
                                == *commits[cursor].1.borrow() as *const Branch
                            {
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    use Key::*;
                    match out.read_key().unwrap() {
                        ArrowLeft => {
                            if branch_index > 0 {
                                *commits[cursor].1.borrow_mut() = &branches[branch_index - 1]
                            }
                        }
                        ArrowRight => {
                            if branch_index < branches.len() - 1 {
                                *commits[cursor].1.borrow_mut() = &branches[branch_index + 1]
                            }
                        }
                        ArrowUp => {
                            if cursor > 0 {
                                cursor -= 1
                            }
                        }
                        ArrowDown => {
                            if cursor < commits.len() - 1 {
                                cursor += 1
                            }
                        }
                        Enter => break,
                        Escape => panic!(),
                        _ => (),
                    }
                }
                out.move_cursor_up(commits.len()).unwrap()
            }
        },
    }) {
        match error {}
    }
}
