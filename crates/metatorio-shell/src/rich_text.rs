//! Plain projection of Factorio rich text for catalog matching. Never rewrites
//! prototype IDs or locale source. Keep parity with the frontend via shared cases.

pub fn plain_text(source: &str) -> String {
    plain_text_with(source, |_, _| None)
}

pub fn plain_text_with(
    source: &str,
    mut resolve: impl FnMut(&str, &str) -> Option<String>,
) -> String {
    let mut result = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find('[') {
        let start = cursor + offset;
        let Some(offset) = source[start + 1..].find(']') else {
            break;
        };
        let end = start + 1 + offset;
        let body = &source[start + 1..end];
        if let Some(nested) = body.rfind('[') {
            result.push_str(&source[cursor..start + 1 + nested]);
            cursor = start + 1 + nested;
            continue;
        }
        result.push_str(&source[cursor..start]);
        let (tag, value) = body.split_once('=').unwrap_or((body, ""));
        let valid_name = |name: &str| {
            !name.is_empty()
                && name.len() <= 1024
                && name != "."
                && name != ".."
                && !name
                    .chars()
                    .any(|ch| ch.is_whitespace() || matches!(ch, ',' | '[' | ']' | '/' | '\\'))
        };
        let reference = matches!(
            tag,
            "item"
                | "entity"
                | "technology"
                | "recipe"
                | "item-group"
                | "fluid"
                | "tile"
                | "virtual-signal"
                | "achievement"
                | "quality"
                | "planet"
                | "space-location"
                | "shortcut"
        );
        if body.contains(['\n', '\r']) {
            result.push_str(&source[start..=end]);
        } else if matches!(body, "/color" | ".color" | "/font" | ".font")
            || (matches!(tag, "color" | "font") && body.contains('='))
        {
            // Styling metadata must not break words or become searchable.
        } else if reference && body.contains('=') {
            let mut parts = value.split(',');
            let name = parts.next().unwrap_or_default();
            let quality = if matches!(
                tag,
                "item" | "entity" | "recipe" | "fluid" | "virtual-signal"
            ) {
                parts.find_map(|part| part.strip_prefix("quality="))
            } else {
                None
            };
            if valid_name(name) && quality.is_none_or(|q| q.is_empty() || valid_name(q)) {
                result.push(' ');
                result.push_str(&resolve(tag, name).unwrap_or_else(|| name.into()));
                if let Some(quality) = quality.filter(|q| !q.is_empty() && *q != "normal") {
                    result.push_str(" (");
                    result.push_str(&resolve("quality", quality).unwrap_or_else(|| quality.into()));
                    result.push(')');
                }
                result.push(' ');
            } else {
                result.push_str(&source[start..=end]);
            }
        } else if tag == "img" && body.contains('=') {
            if let Some((kind, name)) = value.split_once(['/', '.'])
                && matches!(
                    kind,
                    "item"
                        | "entity"
                        | "technology"
                        | "recipe"
                        | "item-group"
                        | "fluid"
                        | "tile"
                        | "virtual-signal"
                        | "achievement"
                        | "quality"
                        | "planet"
                        | "space-location"
                        | "shortcut"
                        | "equipment"
                        | "ammo-category"
                        | "decorative"
                        | "space-connection"
                        | "surface"
                        | "airborne-pollutant"
                        | "asteroid-chunk"
                        | "utility"
                )
                && valid_name(name)
            {
                result.push(' ');
                result.push_str(&resolve(kind, name).unwrap_or_else(|| name.into()));
                result.push(' ');
            } else {
                result.push_str(value);
            }
        } else if tag == "tooltip" && value.contains(',') {
            result.push_str(value.rsplit_once(',').unwrap().0);
        } else if tag == "gps" && body.contains('=') {
            result.push_str("GPS ");
            result.push_str(value);
        } else if matches!(
            tag,
            "train" | "train-stop" | "space-platform" | "armor" | "tip"
        ) && body.contains('=')
        {
            result.push_str(tag);
            result.push_str(": ");
            result.push_str(value);
        } else if tag == "special-item" && body.contains('=') {
            result.push_str("Blueprint");
        } else if body == "space-age" {
            result.push_str("Space Age");
        } else {
            result.push_str(&source[start..=end]);
        }
        cursor = end + 1;
    }
    result.push_str(&source[cursor..]);
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_frontend_backend_cases() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/rich-text.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            assert_eq!(
                super::plain_text(source),
                case["plain"].as_str().unwrap(),
                "{source}"
            );
            if let Some(normalized) = case["normalized"].as_str() {
                assert_eq!(
                    crate::app::normalize_for_match(source),
                    normalized,
                    "{source}"
                );
            }
        }
    }
}
