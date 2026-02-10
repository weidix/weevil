use std::fs;
use std::fs::File;
use std::path::Path;

use fs2::FileExt;

use crate::errors::{AppError, LinkKind};
use crate::mode_params::MultiFolderStrategy;

pub(crate) fn move_locked_file(from: &Path, to: &Path) -> Result<(), AppError> {
    if from == to {
        return Ok(());
    }
    let file = lock_exclusive(from)?;
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_error(&err) => {
            fs::copy(from, to).map_err(|copy_err| AppError::FileCopy {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                source: copy_err,
            })?;
            drop(file);
            fs::remove_file(from).map_err(|remove_err| AppError::FileRemove {
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

pub(crate) fn create_link(
    strategy: MultiFolderStrategy,
    from: &Path,
    to: &Path,
) -> Result<(), AppError> {
    if from == to {
        return Ok(());
    }
    match strategy {
        MultiFolderStrategy::HardLink => {
            fs::hard_link(from, to).map_err(|err| AppError::FileLink {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                kind: LinkKind::Hard,
                source: err,
            })
        }
        MultiFolderStrategy::SoftLink => {
            create_soft_link(from, to).map_err(|err| AppError::FileLink {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                kind: LinkKind::Soft,
                source: err,
            })
        }
        MultiFolderStrategy::First => Ok(()),
    }
}

fn create_soft_link(from: &Path, to: &Path) -> Result<(), std::io::Error> {
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
}

fn is_cross_device_error(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(18)
}

fn lock_exclusive(path: &Path) -> Result<File, AppError> {
    let file = File::open(path).map_err(|err| AppError::FileLock {
        path: path.to_path_buf(),
        source: err,
    })?;
    file.lock_exclusive().map_err(|err| AppError::FileLock {
        path: path.to_path_buf(),
        source: err,
    })?;
    Ok(file)
}
