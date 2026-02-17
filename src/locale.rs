use std::sync::RwLock;

lazy_static::lazy_static! {
    pub static ref LOCALE: RwLock<String> = RwLock::new("zh-CN".to_string());
}

pub fn set_locale(locale: &str) {
    let mut loc = LOCALE.write().unwrap();
    *loc = locale.to_string();
}

pub fn get_locale() -> String {
    LOCALE.read().unwrap().clone()
}
