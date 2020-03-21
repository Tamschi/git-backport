use {
    git2::{BranchType, Repository},
    git_backport::{backport, BackportArgs, Error},
    log::{debug, error},
    std::path::PathBuf,
    structopt::StructOpt,
};

#[derive(Debug, StructOpt)]
#[structopt(author)]
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
        repository
            .find_branch(&head.name().unwrap(), BranchType::Local)
            .unwrap()
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
        "{:#?}",
        branches.iter().map(|b| b.name()).collect::<Vec<_>>()
    );

    if let Err(error) = backport(BackportArgs {
        repository: &repository,
        backup: !options.no_backup,
        branches,
    }) {
        match error {
            Error::UNCLEAN => error!("{}", error),
        }
    }
}
