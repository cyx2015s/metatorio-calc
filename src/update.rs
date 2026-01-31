use std::{
    io::{BufRead, Write},
    sync::Arc,
};

use egui::mutex::Mutex;

use crate::toast;

const REPO_OWNER: &str = "cyx2015s";
const REPO_NAME: &str = "metatorio-calc";
const BIN_NAME: &str = "metatorio";

// 包装一个 ReleaseUpdate 以添加下载进度回调功能
pub struct UpdateWrapper<T>
where
    T: self_update::update::ReleaseUpdate + ?Sized,
{
    inner: Box<T>,
    progress_callback: Option<Box<dyn Fn(u64, u64) + 'static>>,
}

impl<T> UpdateWrapper<T>
where
    T: self_update::update::ReleaseUpdate + ?Sized,
{
    pub fn new(inner: Box<T>) -> Self {
        Self {
            inner,
            progress_callback: None,
        }
    }

    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(u64, u64) + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }
}

impl<T: self_update::update::ReleaseUpdate + ?Sized> self_update::update::ReleaseUpdate
    for UpdateWrapper<T>
{
    fn get_latest_release(&self) -> self_update::errors::Result<self_update::update::Release> {
        self.inner.get_latest_release()
    }

    fn get_latest_releases(
        &self,
        current_version: &str,
    ) -> self_update::errors::Result<Vec<self_update::update::Release>> {
        self.inner.get_latest_releases(current_version)
    }

    fn get_release_version(
        &self,
        ver: &str,
    ) -> self_update::errors::Result<self_update::update::Release> {
        self.inner.get_release_version(ver)
    }

    fn current_version(&self) -> String {
        self.inner.current_version()
    }

    fn target(&self) -> String {
        self.inner.target()
    }

    fn target_version(&self) -> Option<String> {
        self.inner.target_version()
    }

    fn bin_name(&self) -> String {
        self.inner.bin_name()
    }

    fn bin_install_path(&self) -> std::path::PathBuf {
        self.inner.bin_install_path()
    }

    fn bin_path_in_archive(&self) -> String {
        self.inner.bin_path_in_archive()
    }

    fn show_download_progress(&self) -> bool {
        self.inner.show_download_progress()
    }

    fn show_output(&self) -> bool {
        self.inner.show_output()
    }

    fn no_confirm(&self) -> bool {
        self.inner.no_confirm()
    }

    fn progress_template(&self) -> String {
        self.inner.progress_template()
    }

    fn progress_chars(&self) -> String {
        self.inner.progress_chars()
    }

    fn auth_token(&self) -> Option<String> {
        self.inner.auth_token()
    }

    fn update(&self) -> self_update::errors::Result<self_update::Status> {
        let current_version = self.current_version();
        self.update_extended()
            .map(|s| s.into_status(current_version))
    }

    fn api_headers(
        &self,
        auth_token: &Option<String>,
    ) -> self_update::errors::Result<reqwest::header::HeaderMap> {
        self.inner.api_headers(auth_token)
    }

    fn identifier(&self) -> Option<String> {
        self.inner.identifier()
    }

    // 复制过来改了一下
    // https://github.com/jaemk/self_update/blob/master/src/update.rs
    fn update_extended(&self) -> self_update::errors::Result<self_update::update::UpdateStatus> {
        let bin_install_path = self.bin_install_path();
        let bin_name = self.bin_name();

        let current_version = self.current_version();
        let target = self.target();
        log::info!("检测目标架构…… {}", target);
        log::info!("检测当前版本…… v{}", current_version);

        let release = match self.target_version() {
            None => {
                log::info!("检测最新发布版本……");
                let releases = self.get_latest_releases(&current_version)?;
                let release = {
                    // Filter compatible version
                    let compatible_releases = releases
                        .iter()
                        .filter(|r| {
                            self_update::version::bump_is_compatible(&current_version, &r.version)
                                .unwrap_or(false)
                        })
                        .collect::<Vec<_>>();

                    // Get the first version
                    let release = compatible_releases.first().cloned();
                    if let Some(release) = release {
                        log::info!(
                            "v{} (有 {} 个兼容版本)",
                            release.version,
                            compatible_releases.len()
                        );
                        release.clone()
                    } else {
                        let release = releases.first();
                        if let Some(release) = release {
                            log::info!("v{} (有 {} 个版本可用)", release.version, releases.len());
                            release.clone()
                        } else {
                            return Ok(self_update::update::UpdateStatus::UpToDate);
                        }
                    }
                };

                {
                    log::info!("发现新版本！v{} --> v{}", current_version, release.version);
                }

                release
            }
            Some(ref ver) => {
                log::info!("查找 tag: {}", ver);
                self.get_release_version(ver)?
            }
        };

        let target_asset = release
            .asset_for(&target, self.identifier().as_deref())
            .ok_or_else(|| {
                self_update::errors::Error::Release(format!(
                    "没有找到适合 `{}` 架构的可执行文件",
                    target
                ))
            })?;

        log::info!("  * 当前可执行文件: {:?}", bin_install_path);
        log::info!("  * 新可执行文件版本: {:?}", target_asset.name);
        log::info!("  * 新可执行文件下载地址: {:?}", target_asset.download_url);
        log::info!("新版本将被下载/解压，现有的可执行文件文件将被替换。");

        let tmp_archive_dir = tempfile::TempDir::new()?;
        let tmp_archive_path = tmp_archive_dir.path().join(&target_asset.name);
        let tmp_archive = std::fs::File::create(&tmp_archive_path)?;
    
        log::info!("下载中…… {}", tmp_archive_path.display());

        let download_url = &target_asset.download_url;

        log::info!("下载地址: {}", download_url);

        let response = reqwest::blocking::Client::new()
            .get(download_url)
            .headers({
                let mut headers = reqwest::header::HeaderMap::new();
                if let Some(token) = self.auth_token() {
                    headers.insert(
                        reqwest::header::AUTHORIZATION,
                        format!("token {}", token).parse().unwrap(),
                    );
                }
                headers.insert(
                    reqwest::header::ACCEPT,
                    "application/octet-stream".parse().unwrap(),
                );
                headers.insert(
                    reqwest::header::USER_AGENT,
                    "metatorio-calc/self-update".parse().unwrap(),
                );
                headers
            })
            .send()
            .map_err(|err| self_update::errors::Error::Network(err.to_string()))?;

        let total_size = response.content_length().unwrap_or(0);
        log::info!("文件大小: {} bytes", total_size);
        let mut downloaded = 0u64;

        let mut reader = std::io::BufReader::new(response);
        let mut writer = std::io::BufWriter::new(tmp_archive);
        loop {
            let written = {
                let buf = reader.fill_buf()?;
                writer.write_all(buf)?;
                buf.len()
            };
            if written == 0 {
                break;
            }
            reader.consume(written);
            downloaded += written as u64;
            if let Some(ref callback) = self.progress_callback {
                callback(downloaded, total_size);
            }
        }
        
        drop(writer);

        let bin_path_str = std::borrow::Cow::Owned(self.bin_path_in_archive());

        /// Substitute the `var` variable in a string with the given `val` value.
        ///
        /// Variable format: `{{ var }}`
        fn substitute<'a: 'b, 'b>(str: &'a str, var: &str, val: &str) -> std::borrow::Cow<'b, str> {
            let format = format!(r"\{{\{{[[:space:]]*{}[[:space:]]*\}}\}}", var);
            regex::Regex::new(&format).unwrap().replace_all(str, val)
        }

        let bin_path_str = substitute(&bin_path_str, "version", &release.version);
        let bin_path_str = substitute(&bin_path_str, "target", &target);
        let bin_path_str = substitute(&bin_path_str, "bin", &bin_name);
        let bin_path_str = bin_path_str.as_ref();

        self_update::Extract::from_source(&tmp_archive_path)
            .extract_file(tmp_archive_dir.path(), bin_path_str)?;
        let new_exe = tmp_archive_dir.path().join(bin_path_str);
        log::info!("解压的文件: {}", new_exe.display());
        let new_exe_file = std::fs::File::open(&new_exe)?;
        log::info!("新文件的大小是: {}", new_exe_file.metadata()?.len());
        std::thread::sleep(std::time::Duration::from_secs(10));
        log::info!("完成");

        log::info!("替换可执行文件中……");
        self_update::self_replace::self_replace(new_exe)?;
        log::info!("完成");

        Ok(self_update::update::UpdateStatus::Updated(release))
    }
}

pub fn create_update_downloader()
-> Result<UpdateWrapper<dyn self_update::update::ReleaseUpdate>, crate::error::AppError> {
    let release_update = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(self_update::cargo_crate_version!())
        .build()?;

    Ok(UpdateWrapper::new(release_update))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DownloadProgress {
    Pending,
    InProgress(u64, u64),
    Completed,
}

lazy_static::lazy_static!(
    pub static ref DOWNLOAD_PROGRESS: Arc<Mutex<DownloadProgress>> = Arc::new(Mutex::new(DownloadProgress::Pending));
);

pub fn get_download_progress() -> DownloadProgress {
    *DOWNLOAD_PROGRESS.lock()
}

pub fn set_download_progress(progress: DownloadProgress) {
    *DOWNLOAD_PROGRESS.lock() = progress;
}

pub fn update() -> Result<(), crate::error::AppError> {
    let mut updater = create_update_downloader()?;
    updater = updater.with_progress_callback(move |current, total| {
        set_download_progress(DownloadProgress::InProgress(current, total));
    });
    toast::download();
    match self_update::update::ReleaseUpdate::update_extended(&updater) {
        Ok(self_update::update::UpdateStatus::UpToDate) => {
            log::info!("当前已是最新版本");
            set_download_progress(DownloadProgress::Completed);
            return Ok(());
        }
        Ok(self_update::update::UpdateStatus::Updated(release)) => {
            log::info!("已更新到新版本 v{}", release.version);
        }
        Err(err) => {
            log::error!("更新失败: {}", err);
            set_download_progress(DownloadProgress::Pending);
            return Err(crate::error::AppError::Update(err.to_string()));
        }
    }
    set_download_progress(DownloadProgress::Completed);

    Ok(())
}