//! DeepSeek 费用估算（人民币）。
//!
//! 价格取自官网价目表（元/百万 tokens），高峰时段为北京时间周一至周五
//! 9:00-12:00、14:00-18:00，空闲时段按高峰价的一半计算；中国法定节假日
//! 无法自动识别，统一按普通工作日处理。
//! 来源: https://api-docs.deepseek.com/zh-cn/quick_start/pricing

use chrono::{Datelike, FixedOffset, TimeZone, Timelike, Weekday};

const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const TOKENS_PER_MILLION: f64 = 1_000_000.0;

#[derive(Clone, Copy)]
struct PeakPrice {
    cache_hit_cny: f64,
    cache_miss_cny: f64,
    output_cny: f64,
}

impl PeakPrice {
    fn off_peak(self) -> Self {
        Self {
            cache_hit_cny: self.cache_hit_cny / 2.0,
            cache_miss_cny: self.cache_miss_cny / 2.0,
            output_cny: self.output_cny / 2.0,
        }
    }
}

fn peak_price(model: &str) -> Option<PeakPrice> {
    match model.trim().to_ascii_lowercase().as_str() {
        "deepseek-flash" => Some(PeakPrice {
            cache_hit_cny: 0.04,
            cache_miss_cny: 2.0,
            output_cny: 8.0,
        }),
        "deepseek-v4-pro" => Some(PeakPrice {
            cache_hit_cny: 0.30,
            cache_miss_cny: 9.0,
            output_cny: 27.0,
        }),
        _ => None,
    }
}

fn is_peak_hour(created_at: i64) -> bool {
    let Some(offset) = FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS) else {
        return false;
    };
    let Some(time) = offset.timestamp_opt(created_at, 0).single() else {
        return false;
    };
    if matches!(time.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let hour = time.hour();
    (9..12).contains(&hour) || (14..18).contains(&hour)
}

/// 按请求发生时间估算单条请求的人民币费用；未收录的 DeepSeek 模型返回 0。
pub(crate) fn estimate_deepseek_cost_cny(
    model: &str,
    created_at: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> f64 {
    let Some(peak) = peak_price(model) else {
        return 0.0;
    };
    let price = if is_peak_hour(created_at) {
        peak
    } else {
        peak.off_peak()
    };
    let input_tokens = input_tokens.max(0);
    let cache_hit_tokens = cached_input_tokens.max(0).min(input_tokens);
    let cache_miss_tokens = input_tokens - cache_hit_tokens;
    let output_tokens = output_tokens.max(0);
    (cache_hit_tokens as f64 * price.cache_hit_cny
        + cache_miss_tokens as f64 * price.cache_miss_cny
        + output_tokens as f64 * price.output_cny)
        / TOKENS_PER_MILLION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beijing_ts(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS)
            .expect("beijing offset")
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("valid beijing datetime")
            .timestamp()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    // 2026-09-30 为周三，2026-10-03 为周六。
    #[test]
    fn peak_and_off_peak_windows_use_beijing_time() {
        let cases = [
            (9, 0, true),
            (11, 59, true),
            (12, 0, false),
            (14, 0, true),
            (17, 59, true),
            (18, 0, false),
            (0, 30, false),
            (23, 30, false),
        ];
        for (hour, minute, expected) in cases {
            assert_eq!(
                is_peak_hour(beijing_ts(2026, 9, 30, hour, minute)),
                expected,
                "hour {hour}:{minute:02}"
            );
        }
        assert!(!is_peak_hour(beijing_ts(2026, 10, 3, 10, 0)));
        assert!(!is_peak_hour(beijing_ts(2026, 10, 4, 15, 0)));
    }

    #[test]
    fn flash_cost_splits_cache_hit_and_miss() {
        let peak = beijing_ts(2026, 9, 30, 10, 0);
        let off_peak = beijing_ts(2026, 9, 30, 20, 0);
        // 1M 未命中输入 = 高峰 ¥2 / 空闲 ¥1
        assert_close(
            estimate_deepseek_cost_cny("deepseek-flash", peak, 1_000_000, 0, 0),
            2.0,
        );
        assert_close(
            estimate_deepseek_cost_cny("deepseek-flash", off_peak, 1_000_000, 0, 0),
            1.0,
        );
        // 输入已含缓存命中部分：1M 输入其中 1M 命中 = 高峰 ¥0.04
        assert_close(
            estimate_deepseek_cost_cny("deepseek-flash", peak, 1_000_000, 1_000_000, 0),
            0.04,
        );
        // 输出 1M = 高峰 ¥8
        assert_close(
            estimate_deepseek_cost_cny("deepseek-flash", peak, 0, 0, 1_000_000),
            8.0,
        );
    }

    #[test]
    fn v4_pro_cost_and_mixed_tokens() {
        let peak = beijing_ts(2026, 9, 30, 15, 0);
        // 1M 输入（0.5M 命中）+ 1M 输出 = 0.5*0.30 + 0.5*9 + 1*27 = ¥31.65
        assert_close(
            estimate_deepseek_cost_cny("deepseek-v4-pro", peak, 1_000_000, 500_000, 1_000_000),
            31.65,
        );
    }

    #[test]
    fn unknown_models_and_dirty_tokens_are_ignored() {
        let peak = beijing_ts(2026, 9, 30, 10, 0);
        assert_close(
            estimate_deepseek_cost_cny("deepseek-chat", peak, 1_000_000, 0, 1_000_000),
            0.0,
        );
        assert_close(estimate_deepseek_cost_cny("gpt-5", peak, 1_000, 0, 1_000), 0.0);
        // 缓存命中数超过输入总数时按输入总数封顶，负数按 0 处理
        assert_close(
            estimate_deepseek_cost_cny("deepseek-flash", peak, 10, 100, -5),
            0.000_000_4,
        );
    }

    #[test]
    fn model_name_matching_ignores_case_and_spaces() {
        let peak = beijing_ts(2026, 9, 30, 10, 0);
        assert_close(
            estimate_deepseek_cost_cny(" DeepSeek-Flash ", peak, 1_000_000, 0, 0),
            2.0,
        );
        // 只认官网标准模型名，版本化别名（如 deepseek-v4-flash）不计费
        assert_close(
            estimate_deepseek_cost_cny("deepseek-v4-flash", peak, 1_000_000, 0, 0),
            0.0,
        );
    }
}
