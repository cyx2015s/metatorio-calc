use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct MemoryLogger {
    pub vec: Arc<Mutex<Vec<String>>>,
}

lazy_static::lazy_static!(
    pub static ref MEMLOG: MemoryLogger = MemoryLogger::default();
);
impl std::io::Write for MemoryLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(buf)
            .to_string()
            .lines()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        let mut vec = self.vec.lock().unwrap();
        vec.extend(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
