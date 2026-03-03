#[derive(Clone, Debug)]
struct Tokens {
    raw: Vec<String>,
    lower: Vec<String>,
}

pub(crate) fn subtitle_suffix(video_stem: &str, subtitle_stem: &str) -> Option<String> {
    if video_stem == subtitle_stem {
        return Some(String::new());
    }

    let video_tokens = tokenize_stem(video_stem);
    let subtitle_tokens = tokenize_stem(subtitle_stem);
    let video_variants = token_variants(&video_tokens);
    let subtitle_variants = token_variants(&subtitle_tokens);

    for video in &video_variants {
        for subtitle in &subtitle_variants {
            if subtitle.lower == video.lower {
                return Some(String::new());
            }
            if subtitle.lower.starts_with(&video.lower) {
                let offset = video.raw.len();
                return Some(build_suffix(
                    &subtitle.raw[offset..],
                    &subtitle.lower[offset..],
                ));
            }
            if video.lower.starts_with(&subtitle.lower) && short_name_ok(subtitle) {
                return Some(String::new());
            }
        }
    }

    None
}

fn tokenize_stem(stem: &str) -> Tokens {
    let mut raw = Vec::new();
    let mut lower = Vec::new();
    let mut raw_current = String::new();
    let mut lower_current = String::new();

    for ch in stem.chars() {
        if ch.is_alphanumeric() {
            raw_current.push(ch);
            for lower_ch in ch.to_lowercase() {
                lower_current.push(lower_ch);
            }
        } else if !raw_current.is_empty() {
            raw.push(std::mem::take(&mut raw_current));
            lower.push(std::mem::take(&mut lower_current));
        }
    }

    if !raw_current.is_empty() {
        raw.push(raw_current);
        lower.push(lower_current);
    }

    Tokens { raw, lower }
}

fn token_variants(tokens: &Tokens) -> Vec<Tokens> {
    let mut variants = Vec::new();
    push_unique_variant(&mut variants, tokens.clone());
    let no_noise = filter_tokens(tokens, is_noise_token);
    push_unique_variant(&mut variants, no_noise.clone());
    let no_year = filter_tokens(&no_noise, is_year_token);
    push_unique_variant(&mut variants, no_year);
    variants
}

fn push_unique_variant(variants: &mut Vec<Tokens>, candidate: Tokens) {
    if candidate.lower.is_empty() {
        return;
    }
    if variants
        .iter()
        .any(|variant| variant.lower == candidate.lower)
    {
        return;
    }
    variants.push(candidate);
}

fn filter_tokens(tokens: &Tokens, drop: fn(&str) -> bool) -> Tokens {
    let mut raw = Vec::new();
    let mut lower = Vec::new();
    for (raw_token, lower_token) in tokens.raw.iter().zip(tokens.lower.iter()) {
        if drop(lower_token) {
            continue;
        }
        raw.push(raw_token.clone());
        lower.push(lower_token.clone());
    }
    Tokens { raw, lower }
}

fn is_noise_token(token: &str) -> bool {
    if token.len() >= 4
        && token.ends_with('p')
        && token[..token.len() - 1]
            .chars()
            .all(|ch| ch.is_ascii_digit())
    {
        return true;
    }

    matches!(
        token,
        "x264"
            | "h264"
            | "x265"
            | "h265"
            | "hevc"
            | "hdr"
            | "uhd"
            | "bluray"
            | "bdrip"
            | "brrip"
            | "webrip"
            | "webdl"
            | "remux"
            | "aac"
            | "dts"
            | "ac3"
            | "xvid"
    )
}

fn is_year_token(token: &str) -> bool {
    if token.len() != 4 {
        return false;
    }
    if !token.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    match token.parse::<u16>() {
        Ok(year) => (1900..=2100).contains(&year),
        Err(_) => false,
    }
}

fn short_name_ok(tokens: &Tokens) -> bool {
    let len = tokens
        .lower
        .iter()
        .map(|token| token.chars().count())
        .sum::<usize>();
    len >= 2
}

fn build_suffix(raw_tokens: &[String], lower_tokens: &[String]) -> String {
    if raw_tokens.is_empty() {
        String::new()
    } else {
        let (language, consumed) = detect_language_prefix(lower_tokens);
        let mut parts = Vec::new();
        if let Some(language) = language {
            parts.push(language.to_string());
        }
        if consumed < raw_tokens.len() {
            parts.extend(raw_tokens[consumed..].iter().cloned());
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!(".{}", parts.join("."))
        }
    }
}

fn detect_language_prefix(tokens: &[String]) -> (Option<&'static str>, usize) {
    for pattern in LANGUAGE_PATTERNS {
        if tokens.len() < pattern.tokens.len() {
            continue;
        }
        if pattern
            .tokens
            .iter()
            .enumerate()
            .all(|(idx, token)| tokens[idx] == *token)
        {
            return (Some(pattern.canonical), pattern.tokens.len());
        }
    }
    (None, 0)
}

struct LanguagePattern {
    tokens: &'static [&'static str],
    canonical: &'static str,
}

const LANGUAGE_PATTERNS: &[LanguagePattern] = &[
    LanguagePattern {
        tokens: &["zh", "cn"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["zh", "hans"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["zh", "tw"],
        canonical: "zh-TW",
    },
    LanguagePattern {
        tokens: &["zh", "hant"],
        canonical: "zh-TW",
    },
    LanguagePattern {
        tokens: &["zh", "hk"],
        canonical: "zh-HK",
    },
    LanguagePattern {
        tokens: &["en", "us"],
        canonical: "en-US",
    },
    LanguagePattern {
        tokens: &["en", "gb"],
        canonical: "en-GB",
    },
    LanguagePattern {
        tokens: &["pt", "br"],
        canonical: "pt-BR",
    },
    LanguagePattern {
        tokens: &["pt", "pt"],
        canonical: "pt-PT",
    },
    LanguagePattern {
        tokens: &["zhcn"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["zhtw"],
        canonical: "zh-TW",
    },
    LanguagePattern {
        tokens: &["zhhk"],
        canonical: "zh-HK",
    },
    LanguagePattern {
        tokens: &["zhhans"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["zhhant"],
        canonical: "zh-TW",
    },
    LanguagePattern {
        tokens: &["enus"],
        canonical: "en-US",
    },
    LanguagePattern {
        tokens: &["engb"],
        canonical: "en-GB",
    },
    LanguagePattern {
        tokens: &["ptbr"],
        canonical: "pt-BR",
    },
    LanguagePattern {
        tokens: &["ptpt"],
        canonical: "pt-PT",
    },
    LanguagePattern {
        tokens: &["zh"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["chs"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["cht"],
        canonical: "zh-TW",
    },
    LanguagePattern {
        tokens: &["ch"],
        canonical: "zh-CN",
    },
    LanguagePattern {
        tokens: &["en"],
        canonical: "en",
    },
    LanguagePattern {
        tokens: &["eng"],
        canonical: "en",
    },
    LanguagePattern {
        tokens: &["ja"],
        canonical: "ja",
    },
    LanguagePattern {
        tokens: &["jp"],
        canonical: "ja",
    },
    LanguagePattern {
        tokens: &["jpn"],
        canonical: "ja",
    },
    LanguagePattern {
        tokens: &["ko"],
        canonical: "ko",
    },
    LanguagePattern {
        tokens: &["kr"],
        canonical: "ko",
    },
    LanguagePattern {
        tokens: &["kor"],
        canonical: "ko",
    },
    LanguagePattern {
        tokens: &["fr"],
        canonical: "fr",
    },
    LanguagePattern {
        tokens: &["fra"],
        canonical: "fr",
    },
    LanguagePattern {
        tokens: &["fre"],
        canonical: "fr",
    },
    LanguagePattern {
        tokens: &["de"],
        canonical: "de",
    },
    LanguagePattern {
        tokens: &["deu"],
        canonical: "de",
    },
    LanguagePattern {
        tokens: &["ger"],
        canonical: "de",
    },
    LanguagePattern {
        tokens: &["es"],
        canonical: "es",
    },
    LanguagePattern {
        tokens: &["spa"],
        canonical: "es",
    },
    LanguagePattern {
        tokens: &["it"],
        canonical: "it",
    },
    LanguagePattern {
        tokens: &["ita"],
        canonical: "it",
    },
    LanguagePattern {
        tokens: &["pt"],
        canonical: "pt",
    },
    LanguagePattern {
        tokens: &["por"],
        canonical: "pt",
    },
    LanguagePattern {
        tokens: &["ru"],
        canonical: "ru",
    },
    LanguagePattern {
        tokens: &["rus"],
        canonical: "ru",
    },
    LanguagePattern {
        tokens: &["ar"],
        canonical: "ar",
    },
    LanguagePattern {
        tokens: &["ara"],
        canonical: "ar",
    },
    LanguagePattern {
        tokens: &["th"],
        canonical: "th",
    },
    LanguagePattern {
        tokens: &["tha"],
        canonical: "th",
    },
    LanguagePattern {
        tokens: &["vi"],
        canonical: "vi",
    },
    LanguagePattern {
        tokens: &["vie"],
        canonical: "vi",
    },
    LanguagePattern {
        tokens: &["id"],
        canonical: "id",
    },
    LanguagePattern {
        tokens: &["ind"],
        canonical: "id",
    },
    LanguagePattern {
        tokens: &["ms"],
        canonical: "ms",
    },
    LanguagePattern {
        tokens: &["msa"],
        canonical: "ms",
    },
    LanguagePattern {
        tokens: &["tr"],
        canonical: "tr",
    },
    LanguagePattern {
        tokens: &["tur"],
        canonical: "tr",
    },
    LanguagePattern {
        tokens: &["nl"],
        canonical: "nl",
    },
    LanguagePattern {
        tokens: &["nld"],
        canonical: "nl",
    },
    LanguagePattern {
        tokens: &["dut"],
        canonical: "nl",
    },
    LanguagePattern {
        tokens: &["sv"],
        canonical: "sv",
    },
    LanguagePattern {
        tokens: &["swe"],
        canonical: "sv",
    },
    LanguagePattern {
        tokens: &["no"],
        canonical: "no",
    },
    LanguagePattern {
        tokens: &["nor"],
        canonical: "no",
    },
    LanguagePattern {
        tokens: &["da"],
        canonical: "da",
    },
    LanguagePattern {
        tokens: &["dan"],
        canonical: "da",
    },
    LanguagePattern {
        tokens: &["fi"],
        canonical: "fi",
    },
    LanguagePattern {
        tokens: &["fin"],
        canonical: "fi",
    },
    LanguagePattern {
        tokens: &["pl"],
        canonical: "pl",
    },
    LanguagePattern {
        tokens: &["pol"],
        canonical: "pl",
    },
    LanguagePattern {
        tokens: &["cs"],
        canonical: "cs",
    },
    LanguagePattern {
        tokens: &["ces"],
        canonical: "cs",
    },
    LanguagePattern {
        tokens: &["cze"],
        canonical: "cs",
    },
    LanguagePattern {
        tokens: &["hu"],
        canonical: "hu",
    },
    LanguagePattern {
        tokens: &["hun"],
        canonical: "hu",
    },
    LanguagePattern {
        tokens: &["ro"],
        canonical: "ro",
    },
    LanguagePattern {
        tokens: &["ron"],
        canonical: "ro",
    },
    LanguagePattern {
        tokens: &["rum"],
        canonical: "ro",
    },
    LanguagePattern {
        tokens: &["el"],
        canonical: "el",
    },
    LanguagePattern {
        tokens: &["ell"],
        canonical: "el",
    },
    LanguagePattern {
        tokens: &["gre"],
        canonical: "el",
    },
    LanguagePattern {
        tokens: &["he"],
        canonical: "he",
    },
    LanguagePattern {
        tokens: &["heb"],
        canonical: "he",
    },
    LanguagePattern {
        tokens: &["hi"],
        canonical: "hi",
    },
    LanguagePattern {
        tokens: &["hin"],
        canonical: "hi",
    },
    LanguagePattern {
        tokens: &["uk"],
        canonical: "uk",
    },
    LanguagePattern {
        tokens: &["ukr"],
        canonical: "uk",
    },
    LanguagePattern {
        tokens: &["bg"],
        canonical: "bg",
    },
    LanguagePattern {
        tokens: &["bul"],
        canonical: "bg",
    },
    LanguagePattern {
        tokens: &["sr"],
        canonical: "sr",
    },
    LanguagePattern {
        tokens: &["srp"],
        canonical: "sr",
    },
    LanguagePattern {
        tokens: &["hr"],
        canonical: "hr",
    },
    LanguagePattern {
        tokens: &["hrv"],
        canonical: "hr",
    },
];
