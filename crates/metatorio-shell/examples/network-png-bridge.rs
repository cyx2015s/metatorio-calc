//! Stdio bridge used by browser regressions to exercise the real native encoder
//! without opening a desktop save dialog. One JSON request/response per line.
use metatorio_shell::network_png::NetworkPngExports;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
fn main() {
    let exports = NetworkPngExports::default();
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let result = (|| -> Result<Value, String> {
            let value: Value = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let number = |key: &str| value[key].as_u64().ok_or_else(|| format!("missing {key}"));
            match value["cmd"].as_str().unwrap_or_default() {
                "begin" => serde_json::to_value(exports.begin(
                    value["path"].as_str().ok_or("missing path")?.into(),
                    u32::try_from(number("width")?).map_err(|e| e.to_string())?,
                    u32::try_from(number("height")?).map_err(|e| e.to_string())?,
                )?)
                .map_err(|e| e.to_string()),
                "tile" => {
                    exports.tile(
                        number("id")?,
                        u32::try_from(number("x")?).map_err(|e| e.to_string())?,
                        u32::try_from(number("y")?).map_err(|e| e.to_string())?,
                        value["imageBase64"].as_str().ok_or("missing PNG")?,
                    )?;
                    Ok(Value::Null)
                }
                "finish" => Ok(json!(exports.finish(number("id")?)?)),
                "abort" => {
                    exports.abort(number("id")?)?;
                    Ok(Value::Null)
                }
                _ => Err("unknown command".into()),
            }
        })();
        let response = match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error}),
        };
        if writeln!(out, "{response}")
            .and_then(|_| out.flush())
            .is_err()
        {
            break;
        }
    }
}
