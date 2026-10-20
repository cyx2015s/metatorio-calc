use std::{path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// Retain OS requests until the frontend has subscribed and finished booting.
#[derive(Default)]
pub struct OpenRequests(Mutex<Vec<String>>);

impl OpenRequests {
    pub fn push(&self, files: impl IntoIterator<Item = PathBuf>) {
        if let Ok(mut pending) = self.0.lock() {
            for file in files {
                let path = file.to_string_lossy().into_owned();
                if !pending.contains(&path) {
                    pending.push(path);
                }
            }
        }
    }
    pub fn take(&self) -> Result<Vec<String>, String> {
        self.0
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .map_err(|error| error.to_string())
    }
}

pub fn request(app: &AppHandle, files: impl IntoIterator<Item = PathBuf>) {
    app.state::<OpenRequests>().push(files);
    let _ = app.emit("project-open-requested", ());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_requests_survive_before_subscription_and_drain_once() {
        let requests = OpenRequests::default();
        requests.push([
            PathBuf::from("中文 空格.metatorio"),
            PathBuf::from("two.json"),
        ]);
        requests.push([PathBuf::from("中文 空格.metatorio")]);
        assert_eq!(
            requests.take().unwrap(),
            ["中文 空格.metatorio", "two.json"]
        );
        assert!(requests.take().unwrap().is_empty());
        requests.push([PathBuf::from("later.metatorio")]);
        assert_eq!(requests.take().unwrap(), ["later.metatorio"]);
    }
}
