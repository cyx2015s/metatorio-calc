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
        let s = String::from_utf8_lossy(buf).to_string();
        let mut vec = self.vec.lock().unwrap();
        vec.push(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
