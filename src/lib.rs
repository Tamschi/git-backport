use {
    core::fmt::{self, Formatter},
    git2::{Branch, Repository},
    log::debug,
};

#[derive(Debug)]
pub enum Error {
    UNCLEAN,
}
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
    let mut unclean = false;
    for status in repository.statuses(None).unwrap().iter() {
        unclean = true;
        debug!("{:?} {:?}", status.status(), status.path());
    }
    if unclean {
        return Err(Error::UNCLEAN);
    }
    Ok(())
}
