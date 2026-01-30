use std::io::{BufRead, Write};

use self_update::update::ReleaseUpdate;

use crate::error::AppError;

const REPO_OWNER: &str = "cyx2015s";
const REPO_NAME: &str = "metatorio-calc";
const BIN_NAME: &str = "metatorio";

pub struct UpdateWrapper<T>
where
    T: self_update::update::ReleaseUpdate + ?Sized,
{
    inner: Box<T>,
    progress_callback: Option<Box<dyn Fn(u64, u64)>>,
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

    // 复制过来改了一下
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
                    let qualifier = if self_update::version::bump_is_compatible(
                        &current_version,
                        &release.version,
                    )? {
                        ""
                    } else {
                        "不"
                    };
                    log::info!("新版本{}兼容的", qualifier);
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

        let prompt_confirmation = !self.no_confirm();
        if self.show_output() || prompt_confirmation {
            log::info!("\n{} 发布状态:", bin_name);
            log::info!("  * 当前可执行文件: {:?}", bin_install_path);
            log::info!("  * 新可执行文件版本: {:?}", target_asset.name);
            log::info!("  * 新可执行文件下载地址: {:?}", target_asset.download_url);
            log::info!("\n新版本将被下载/解压，现有的可执行文件文件将被替换。");
        }

        let tmp_archive_dir = tempfile::TempDir::new()?;
        let tmp_archive_path = tmp_archive_dir.path().join(&target_asset.name);
        let tmp_archive = std::fs::File::create(&tmp_archive_path)?;

        log::info!("下载中……");

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
                headers.insert(reqwest::header::ACCEPT, "application/octet-stream".parse().unwrap());
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

        log::info!("解压中……");

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

        log::info!("完成");

        log::info!("替换可执行文件中……");
        self_update::self_replace::self_replace(new_exe)?;
        log::info!("完成");

        Ok(self_update::update::UpdateStatus::Updated(release))
    }
}

pub fn update() -> Result<(), AppError> {
    let release_builder = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .unwrap();

    log::info!("发现的 release 列表: {:?}", release_builder.fetch()?);

    let status_builder = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(true)
        .current_version("0.9.0")
        .build()?;

    let updater = UpdateWrapper::new(status_builder).with_progress_callback(|current, total| {
        if total > 0 {
            eprintln!("下载进度: {}% ({:08}/{:08} bytes)\r", current * 100 / total, current, total);
        } else {
            eprintln!("下载进度: {:08} bytes\r", current);
        }
    });

    updater.update_extended()?;

    Ok(())
}

#[test]
fn test_update() -> Result<(), AppError> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .try_init()
        .unwrap();
    update()?;

    Ok(())
}
