use maud::{Markup, html};
use num_format::{Locale, ToFormattedString};

pub fn format_byte_size(size: u64) -> String {
    if size < 1024 {
        return format!("{} B", size)
    } else if size < 1024 * 1024 {
        return format!("{:.2} kB", size as f64 / 1000.0)
    } else {
        return format!("{:.2} MB", size as f64 / 1000_0000.0)
    }
}

pub fn format_integer(int: u64) -> Markup {
    let string = int.to_formatted_string(&Locale::en);
    let parts = string.split(",").collect::<Vec<_>>();
    html! {
        @for part in parts.iter().take(parts.len() - 1) {
            span.digit-sep { (part) }
        }
        span { (parts[parts.len() - 1]) }
    }
}

pub fn format_difficulty(difficulty: f64) -> Markup {
    let est_hashrate = difficulty * (0xffffffffu64 as f64) / 600.0;
    let hashrate= if est_hashrate < 1e12 {
        html! { (format!("{:.2} GH/s", est_hashrate / 1e9)) }
    } else if est_hashrate < 1e15 {
        html! { (format!("{:.2} TH/s", est_hashrate / 1e12)) }
    } else if est_hashrate < 1e18 {
        html! { (format!("{:.2} PH/s", est_hashrate / 1e15)) }
    } else {
        html! { (format!("{:.2} EH/s", est_hashrate / 1e18)) }
    };
    let num_digits = difficulty.log10().floor();
    let exponent = (num_digits / 3.0) as u32;
    let difficulty = match exponent {
        0 => html! { (format!("{:.0}", difficulty)) },
        1 => html! { (format!("{:.2}", difficulty / 1e3)) " ×10" sup { "3" } },
        2 => html! { (format!("{:.2}", difficulty / 1e6)) " ×10" sup { "6" } },
        3 => html! { (format!("{:.2}", difficulty / 1e9)) " ×10" sup { "9" } },
        4 => html! { (format!("{:.2}", difficulty / 1e12)) " ×10" sup { "12" } },
        _ => html! { (format!("{:.2}", difficulty / 1e15)) " ×10" sup { "15" } },
    };
    html! {
        (difficulty)
        small {
            " (10 min. blocks = "
            (hashrate)
            ")"
        }
    }
}
