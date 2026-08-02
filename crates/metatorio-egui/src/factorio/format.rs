const LARGE_UNITS: [&str; 11] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];

pub fn signed_compact_number(num: f64) -> String {
    if num.is_sign_negative() {
        format!("-{}", compact_number(-num))
    } else {
        format!("+{}", compact_number(num))
    }
}

pub fn compact_number(num: f64) -> String {
    let abs_num = num.abs();

    match abs_num {
        n if n < 1e-15 => String::from("0"),
        n if n < 1e-9 => format_with_unit(num * 1e12, "p"),
        n if n < 0.01 => format_with_unit(num * 1e6, "μ"),
        n => {
            let mut unit_idx = 0;
            let mut n = n;
            if n > 10000.0 {
                while n > 1000.0 && unit_idx < LARGE_UNITS.len() - 1 {
                    unit_idx += 1;
                    n /= 1000.0;
                }
            }
            format_with_unit(n * num.signum(), LARGE_UNITS[unit_idx])
        }
    }
}

fn format_with_unit(value: f64, unit: &str) -> String {
    let abs_value = value.abs();
    if unit.is_empty() {
        if abs_value < 10.0 {
            // 对于小于10的值，最多保留2位小数
            let formatted = format!("{:.2}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else if abs_value < 1000.0 {
            // 对于大于10的值，最多保留1位小数
            let formatted = format!("{:.1}", value);
            format!(
                "{}{}",
                formatted.trim_end_matches('0').trim_end_matches('.'),
                unit
            )
        } else {
            // 对于大于等于1000的值，取整
            format!("{}{}", value.round(), unit)
        }
    } else if abs_value < 10.0 {
        // 对于小于10的值，最多保留2位小数
        let formatted = format!("{:.2}", value);
        format!(
            "{}{}",
            formatted.trim_end_matches('0').trim_end_matches('.'),
            unit
        )
    } else if abs_value < 100.0 {
        // 对于10-100的值，最多保留1位小数
        let formatted = format!("{:.1}", value);
        format!(
            "{}{}",
            formatted.trim_end_matches('0').trim_end_matches('.'),
            unit
        )
    } else {
        // 对于大于等于100的值，取整
        format!("{}{}", value.round(), unit)
    }
}

pub fn parse_number(n: &str) -> Option<f64> {
    let re = regex::Regex::new(r"^-?[\d.e-]+[kMGTPEZYRQμp]?$").ok()?;
    if re.is_match(n) {
        let multiplier = match n.chars().next_back() {
            Some('p') => 0.000_000_000_001,
            Some('μ') => 0.000_001,
            Some('k') => 1_000.0,
            Some('M') => 1_000_000.0,
            Some('G') => 1_000_000_000.0,
            Some('T') => 1_000_000_000_000.0,
            Some('P') => 1_000_000_000_000_000.0,
            Some('E') => 1_000_000_000_000_000_000.0,
            Some('Z') => 1_000_000_000_000_000_000_000.0,
            Some('Y') => 1_000_000_000_000_000_000_000_000.0,
            Some('R') => 1_000_000_000_000_000_000_000_000_000.0,
            Some('Q') => 1_000_000_000_000_000_000_000_000_000_000.0,
            _ => 1.0,
        };
        let numeric_value: f64 = n
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .ok()?;
        Some(numeric_value * multiplier)
    } else {
        None
    }
}

pub fn parse_energy(n: &str) -> Option<f64> {
    let re = regex::Regex::new(r"^-?[\d.e-]+[kMGTPEZYRQμ]?[JW]$").ok()?;
    if re.is_match(n) {
        let mut multiplier = match n.chars().rev().nth(1) {
            Some('k') => 1_000.0,
            Some('M') => 1_000_000.0,
            Some('G') => 1_000_000_000.0,
            Some('T') => 1_000_000_000_000.0,
            Some('P') => 1_000_000_000_000_000.0,
            Some('E') => 1_000_000_000_000_000_000.0,
            Some('Z') => 1_000_000_000_000_000_000_000.0,
            Some('Y') => 1_000_000_000_000_000_000_000_000.0,
            Some('R') => 1_000_000_000_000_000_000_000_000_000.0,
            Some('Q') => 1_000_000_000_000_000_000_000_000_000_000.0,
            _ => 1.0,
        };
        let dimension_char = n.chars().last();
        if let Some('W') = dimension_char {
            multiplier /= 60.0
        }
        let numeric_value: f64 = n
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .ok()?;
        Some(numeric_value * multiplier)
    } else {
        None
    }
}

pub fn drag_value<T>(val: &mut T) -> egui::DragValue<'_>
where
    T: egui::emath::Numeric,
{
    egui::DragValue::new(val)
        .custom_parser(parse_number)
        .custom_formatter(|n, _| compact_number(n))
        .update_while_editing(false)
}

pub fn drag_watt<T>(val: &mut T) -> egui::DragValue<'_>
where
    T: egui::emath::Numeric,
{
    egui::DragValue::new(val)
        .suffix("W")
        .custom_parser(|s| parse_energy(s).map(|x| x * 60.0))
        .custom_formatter(|n, _| compact_number(n))
        .update_while_editing(false)
}

#[test]
fn test_compact_format() {
    dbg!(compact_number(1.1));
    dbg!(compact_number(114514.0));
    dbg!(compact_number(1919810.1));
    dbg!(compact_number(123456789.1));
    dbg!(compact_number(0.00011));
}
