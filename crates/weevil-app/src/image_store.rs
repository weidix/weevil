use std::collections::{HashMap, HashSet};
use std::path::Path;

use tokio::fs;
use weevil_lua::{HttpClient, HttpRequestOptions, TrustedUrl};

use crate::errors::AppError;
use crate::nfo::{Movie, Thumb};

pub(crate) async fn localize_movie_images(
    movie: &mut Movie,
    target_dir: &Path,
    file_base: &str,
    trusted_urls: &[TrustedUrl],
) -> Result<Vec<String>, AppError> {
    let client = HttpClient::new(trusted_urls.to_vec()).map_err(AppError::LuaPlugin)?;
    let options = HttpRequestOptions::default();
    localize_movie_images_with_fetcher(movie, target_dir, file_base, |url| {
        let url = url.to_string();
        let client = client.clone();
        let options = options.clone();
        async move {
            client
                .get_bytes_async(&url, &options)
                .await
                .map_err(AppError::LuaPlugin)
        }
    })
    .await
}

async fn localize_movie_images_with_fetcher<F, Fut>(
    movie: &mut Movie,
    target_dir: &Path,
    file_base: &str,
    mut fetcher: F,
) -> Result<Vec<String>, AppError>
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, AppError>>,
{
    let mut state = LocalizeState::new(target_dir);

    if let Some(thumb) = movie.thumb.as_mut() {
        localize_thumb(
            thumb,
            &format!("{file_base}-poster"),
            &mut state,
            &mut fetcher,
        )
        .await?;
    }

    if let Some(fanart) = movie.fanart.as_mut() {
        for (index, thumb) in fanart.thumb.iter_mut().enumerate() {
            let name = if index == 0 {
                format!("{file_base}-fanart")
            } else {
                format!("{file_base}-fanart-{}", index + 1)
            };
            localize_thumb(thumb, &name, &mut state, &mut fetcher).await?;
        }
    }

    Ok(state.local_files)
}

async fn localize_thumb<F, Fut>(
    thumb: &mut Thumb,
    base_name: &str,
    state: &mut LocalizeState<'_>,
    fetcher: &mut F,
) -> Result<(), AppError>
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, AppError>>,
{
    localize_field(&mut thumb.value, base_name, state, fetcher).await?;
    localize_field(
        &mut thumb.preview,
        &format!("{base_name}-preview"),
        state,
        fetcher,
    )
    .await?;
    Ok(())
}

async fn localize_field<F, Fut>(
    field: &mut Option<String>,
    base_name: &str,
    state: &mut LocalizeState<'_>,
    fetcher: &mut F,
) -> Result<(), AppError>
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, AppError>>,
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
    let local_name = state.resolve(value, base_name, fetcher).await?;
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

    async fn resolve<F, Fut>(
        &mut self,
        remote_url: &str,
        base_name: &str,
        fetcher: &mut F,
    ) -> Result<String, AppError>
    where
        F: FnMut(&str) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>, AppError>>,
    {
        if let Some(local_name) = self.url_to_local.get(remote_url) {
            return Ok(local_name.clone());
        }

        let extension = infer_extension(remote_url);
        let local_name = self.allocate_name(base_name, &extension);
        let local_path = self.target_dir.join(&local_name);
        if !fs::try_exists(&local_path)
            .await
            .map_err(|source| AppError::FetchRuntime {
                reason: format!("failed to inspect image output path {local_path:?}: {source}"),
            })?
        {
            let content = fetcher(remote_url).await?;
            fs::write(&local_path, content)
                .await
                .map_err(|source| AppError::OutputWrite {
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tempfile::tempdir;

    use super::*;

    fn remote(url: &str) -> Option<String> {
        Some(url.to_string())
    }

    #[tokio::test]
    async fn localize_downloads_each_remote_image_once() {
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

        let call_count = Arc::new(AtomicUsize::new(0));
        let files = localize_movie_images_with_fetcher(&mut movie, dir.path(), "Movie", {
            let call_count = Arc::clone(&call_count);
            move |url: &str| {
                let url = url.to_string();
                let call_count = Arc::clone(&call_count);
                async move {
                    assert_eq!(url, "https://img.example/poster.jpg");
                    call_count.fetch_add(1, Ordering::SeqCst);
                    Ok(vec![1, 2, 3])
                }
            }
        })
        .await
        .expect("localize");

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
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
        assert!(
            tokio::fs::try_exists(dir.path().join("Movie-poster.jpg"))
                .await
                .expect("exists")
        );
    }

    #[tokio::test]
    async fn localize_skips_network_when_local_file_exists() {
        let dir = tempdir().expect("temp dir");
        let existing = dir.path().join("Movie-poster.jpg");
        tokio::fs::write(&existing, [9, 8, 7])
            .await
            .expect("seed file");

        let mut movie = Movie {
            thumb: Some(Thumb {
                value: remote("https://img.example/poster.jpg"),
                ..Thumb::default()
            }),
            ..Movie::default()
        };

        let call_count = Arc::new(AtomicUsize::new(0));
        let files = localize_movie_images_with_fetcher(&mut movie, dir.path(), "Movie", {
            let call_count = Arc::clone(&call_count);
            move |_url: &str| {
                let call_count = Arc::clone(&call_count);
                async move {
                    call_count.fetch_add(1, Ordering::SeqCst);
                    Ok(vec![1, 2, 3])
                }
            }
        })
        .await
        .expect("localize");

        assert_eq!(call_count.load(Ordering::SeqCst), 0);
        assert_eq!(files, vec!["Movie-poster.jpg".to_string()]);
        assert_eq!(
            tokio::fs::read(existing).await.expect("read existing"),
            vec![9, 8, 7],
            "existing local file should be reused"
        );
    }

    #[tokio::test]
    async fn localize_keeps_non_remote_fields_unchanged() {
        let dir = tempdir().expect("temp dir");
        let mut movie = Movie {
            thumb: Some(Thumb {
                value: Some("poster.jpg".to_string()),
                preview: Some("./preview.jpg".to_string()),
                ..Thumb::default()
            }),
            ..Movie::default()
        };

        let files = localize_movie_images_with_fetcher(
            &mut movie,
            dir.path(),
            "Movie",
            |_url: &str| async move { panic!("fetcher should not be called") },
        )
        .await
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

    #[tokio::test]
    async fn localize_rejects_http_image_url() {
        let dir = tempdir().expect("temp dir");
        let mut movie = Movie {
            thumb: Some(Thumb {
                value: remote("http://img.example/poster.jpg"),
                ..Thumb::default()
            }),
            ..Movie::default()
        };

        let error = localize_movie_images_with_fetcher(
            &mut movie,
            dir.path(),
            "Movie",
            |_url: &str| async move { panic!("fetcher should not be called for http") },
        )
        .await
        .expect_err("http image url should be rejected");

        match error {
            AppError::ImageHttpNotAllowed { url } => {
                assert_eq!(url, "http://img.example/poster.jpg")
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
