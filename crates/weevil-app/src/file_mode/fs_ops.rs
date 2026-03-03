use std::fs::File;
use std::path::Path;

use fs2::FileExt;
use tokio::fs;

use crate::errors::{AppError, LinkKind};
use crate::mode_params::MultiFolderStrategy;

pub(crate) async fn move_locked_file(from: &Path, to: &Path) -> Result<(), AppError> {
    if from == to {
        return Ok(());
    }
    let file = lock_exclusive(from).await?;
    match fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_error(&err) => {
            fs::copy(from, to)
                .await
                .map_err(|copy_err| AppError::FileCopy {
                    from: from.to_path_buf(),
                    to: to.to_path_buf(),
                    source: copy_err,
                })?;
            drop(file);
            fs::remove_file(from)
                .await
                .map_err(|remove_err| AppError::FileRemove {
                    path: from.to_path_buf(),
                    source: remove_err,
                })?;
            Ok(())
        }
        Err(err) => Err(AppError::FileMove {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            source: err,
        }),
    }
}

pub(crate) async fn create_link(
    strategy: MultiFolderStrategy,
    from: &Path,
    to: &Path,
) -> Result<(), AppError> {
    if from == to {
        return Ok(());
    }
    match strategy {
        MultiFolderStrategy::HardLink => {
            fs::hard_link(from, to)
                .await
                .map_err(|err| AppError::FileLink {
                    from: from.to_path_buf(),
                    to: to.to_path_buf(),
                    kind: LinkKind::Hard,
                    source: err,
                })
        }
        MultiFolderStrategy::SoftLink => {
            create_soft_link(from, to)
                .await
                .map_err(|err| AppError::FileLink {
                    from: from.to_path_buf(),
                    to: to.to_path_buf(),
                    kind: LinkKind::Soft,
                    source: err,
                })
        }
        MultiFolderStrategy::First => Ok(()),
    }
}

async fn create_soft_link(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    let from = from.to_path_buf();
    let to = to.to_path_buf();
    tokio::task::spawn_blocking(move || {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(from, to)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(from, to)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "soft links are not supported on this platform",
            ))
        }
    })
    .await
    .map_err(|err| std::io::Error::other(format!("{err}")))?
}

fn is_cross_device_error(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(18)
}

async fn lock_exclusive(path: &Path) -> Result<File, AppError> {
    let path = path.to_path_buf();
    let path_for_join = path.clone();
    let path_for_io = path.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<File> {
        let file = File::open(&path)?;
        file.lock_exclusive()?;
        Ok(file)
    })
    .await
    .map_err(|err| AppError::FetchRuntime {
        reason: format!("failed to lock file {path_for_join:?}: {err}"),
    })?
    .map_err(|err| AppError::FileLock {
        path: path_for_io,
        source: err,
    })
}
