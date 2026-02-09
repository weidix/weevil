use std::collections::{HashMap, HashSet};
use std::path::Path;

use weevil_lua::{HttpClient, HttpRequestOptions, TrustedUrl};

use crate::errors::AppError;
use crate::nfo::{Movie, Thumb};

pub(crate) fn localize_movie_images(
    movie: &mut Movie,
    target_dir: &Path,
    file_base: &str,
    trusted_urls: &[TrustedUrl],
) -> Result<Vec<String>, AppError> {
    let client = HttpClient::new(trusted_urls.to_vec()).map_err(AppError::LuaPlugin)?;
    let options = HttpRequestOptions::default();
    localize_movie_images_with_fetcher(movie, target_dir, file_base, |url| {
        client
            .get_bytes_blocking(url, &options)
            .map_err(AppError::LuaPlugin)
    })
}

fn localize_movie_images_with_fetcher<F>(
    movie: &mut Movie,
    target_dir: &Path,
    file_base: &str,
    mut fetcher: F,
) -> Result<Vec<String>, AppError>
where
    F: FnMut(&str) -> Result<Vec<u8>, AppError>,
{
    let mut state = LocalizeState::new(target_dir);

    if let Some(thumb) = movie.thumb.as_mut() {
        localize_thumb(
            thumb,
            &format!("{file_base}-poster"),
            &mut state,
            &mut fetcher,
        )?;
    }

    if let Some(fanart) = movie.fanart.as_mut() {
        for (index, thumb) in fanart.thumb.iter_mut().enumerate() {
            let name = if index == 0 {
                format!("{file_base}-fanart")
            } else {
                format!("{file_base}-fanart-{}", index + 1)
            };
            localize_thumb(thumb, &name, &mut state, &mut fetcher)?;
        }
    }

    Ok(state.local_files)
}

fn localize_thumb<F>(
    thumb: &mut Thumb,
    base_name: &str,
    state: &mut LocalizeState<'_>,
    fetcher: &mut F,
) -> Result<(), AppError>
where
    F: FnMut(&str) -> Result<Vec<u8>, AppError>,
{
    localize_field(&mut thumb.value, base_name, state, fetcher)?;
    localize_field(
        &mut thumb.preview,
        &format!("{base_name}-preview"),
        state,
        fetcher,
    )?;
    Ok(())
}

fn localize_field<F>(
    field: &mut Option<String>,
    base_name: &str,
    state: &mut LocalizeState<'_>,
    fetcher: &mut F,
) -> Result<(), AppError>
where
    F: FnMut(&str) -> Result<Vec<u8>, AppError>,
{
    let Some(value) = field.as_deref() else {
        return Ok(());
    };
    if is_http_url(value) {
        return Err(AppError::ImageHttpNotAllowed {
            url: value.trim().to_string(),
        });
    }
    if !is_https_url(value) {
        return Ok(());
    }
    let local_name = state.resolve(value, base_name, fetcher)?;
    *field = Some(local_name);
    Ok(())
}

fn is_http_url(value: &str) -> bool {
    let value = value.trim();
    value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
}

fn is_https_url(value: &str) -> bool {
    let value = value.trim();
    value
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
}

fn infer_extension(url: &str) -> String {
    let no_fragment = url.split('#').next().unwrap_or(url);
    let no_query = no_fragment.split('?').next().unwrap_or(no_fragment);
    let segment = no_query.rsplit('/').next().unwrap_or("");
    let ext = std::path::Path::new(segment)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return "jpg".to_string();
    }
    ext
}

struct LocalizeState<'a> {
    target_dir: &'a Path,
    url_to_local: HashMap<String, String>,
    used_names: HashSet<String>,
    local_files: Vec<String>,
}

impl<'a> LocalizeState<'a> {
    fn new(target_dir: &'a Path) -> Self {
        Self {
            target_dir,
            url_to_local: HashMap::new(),
            used_names: HashSet::new(),
            local_files: Vec::new(),
        }
    }

    fn resolve<F>(
        &mut self,
        remote_url: &str,
        base_name: &str,
        fetcher: &mut F,
    ) -> Result<String, AppError>
    where
        F: FnMut(&str) -> Result<Vec<u8>, AppError>,
    {
        if let Some(local_name) = self.url_to_local.get(remote_url) {
            return Ok(local_name.clone());
        }

        let extension = infer_extension(remote_url);
        let local_name = self.allocate_name(base_name, &extension);
        let local_path = self.target_dir.join(&local_name);
        if !local_path.exists() {
            let content = fetcher(remote_url)?;
            std::fs::write(&local_path, content).map_err(|source| AppError::OutputWrite {
                path: local_path.clone(),
                source,
            })?;
        }

        self.url_to_local
            .insert(remote_url.to_string(), local_name.clone());
        if self.used_names.insert(local_name.clone()) {
            self.local_files.push(local_name.clone());
        }
        Ok(local_name)
    }

    fn allocate_name(&self, base_name: &str, extension: &str) -> String {
        let mut index = 0usize;
        loop {
            let suffix = if index == 0 {
                String::new()
            } else {
                format!("-{index}")
            };
            let candidate = format!("{base_name}{suffix}.{extension}");
            if !self.used_names.contains(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::tempdir;

    use super::*;

    fn remote(url: &str) -> Option<String> {
        Some(url.to_string())
    }

    #[test]
    fn localize_downloads_each_remote_image_once() {
        let dir = tempdir().expect("temp dir");
        let mut movie = Movie {
            thumb: Some(Thumb {
                value: remote("https://img.example/poster.jpg"),
                preview: remote("https://img.example/poster.jpg"),
                ..Thumb::default()
            }),
            fanart: Some(crate::nfo::Fanart {
                thumb: vec![Thumb {
                    value: remote("https://img.example/poster.jpg"),
                    ..Thumb::default()
                }],
            }),
            ..Movie::default()
        };

        let call_count = RefCell::new(0usize);
        let files =
            localize_movie_images_with_fetcher(&mut movie, dir.path(), "Movie", |url: &str| {
                assert_eq!(url, "https://img.example/poster.jpg");
                *call_count.borrow_mut() += 1;
                Ok(vec![1, 2, 3])
            })
            .expect("localize");

        assert_eq!(*call_count.borrow(), 1);
        assert_eq!(files, vec!["Movie-poster.jpg".to_string()]);
        assert_eq!(
            movie.thumb.as_ref().and_then(|thumb| thumb.value.clone()),
            files.first().cloned()
        );
        assert_eq!(
            movie
                .fanart
                .as_ref()
                .and_then(|fanart| fanart.thumb.first())
                .and_then(|thumb| thumb.value.clone()),
            files.first().cloned()
        );
        assert!(dir.path().join("Movie-poster.jpg").exists());
    }

    #[test]
    fn localize_skips_network_when_local_file_exists() {
        let dir = tempdir().expect("temp dir");
        let existing = dir.path().join("Movie-poster.jpg");
        std::fs::write(&existing, [9, 8, 7]).expect("seed file");

        let mut movie = Movie {
            thumb: Some(Thumb {
                value: remote("https://img.example/poster.jpg"),
                ..Thumb::default()
            }),
            ..Movie::default()
        };

        let call_count = RefCell::new(0usize);
        let files =
            localize_movie_images_with_fetcher(&mut movie, dir.path(), "Movie", |_url: &str| {
                *call_count.borrow_mut() += 1;
                Ok(vec![1, 2, 3])
            })
            .expect("localize");

        assert_eq!(*call_count.borrow(), 0);
        assert_eq!(files, vec!["Movie-poster.jpg".to_string()]);
        assert_eq!(
            std::fs::read(existing).expect("read existing"),
            vec![9, 8, 7],
            "existing local file should be reused"
        );
    }

    #[test]
    fn localize_keeps_non_remote_fields_unchanged() {
        let dir = tempdir().expect("temp dir");
        let mut movie = Movie {
            thumb: Some(Thumb {
                value: Some("poster.jpg".to_string()),
                preview: Some("./preview.jpg".to_string()),
                ..Thumb::default()
            }),
            ..Movie::default()
        };

        let files =
            localize_movie_images_with_fetcher(&mut movie, dir.path(), "Movie", |_url: &str| {
                panic!("fetcher should not be called")
            })
            .expect("localize");

        assert!(files.is_empty());
        assert_eq!(
            movie.thumb.as_ref().and_then(|thumb| thumb.value.clone()),
            Some("poster.jpg".to_string())
        );
        assert_eq!(
            movie.thumb.as_ref().and_then(|thumb| thumb.preview.clone()),
            Some("./preview.jpg".to_string())
        );
    }

    #[test]
    fn localize_rejects_http_image_url() {
        let dir = tempdir().expect("temp dir");
        let mut movie = Movie {
            thumb: Some(Thumb {
                value: remote("http://img.example/poster.jpg"),
                ..Thumb::default()
            }),
            ..Movie::default()
        };

        let error =
            localize_movie_images_with_fetcher(&mut movie, dir.path(), "Movie", |_url: &str| {
                panic!("fetcher should not be called for http")
            })
            .expect_err("http image url should be rejected");

        match error {
            AppError::ImageHttpNotAllowed { url } => {
                assert_eq!(url, "http://img.example/poster.jpg")
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
