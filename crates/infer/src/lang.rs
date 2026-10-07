/// Language routing. Product language is English: every generated answer is
/// English. Detection still recognizes Indonesian queries so they route to
/// the correct intent and are answered in English (with the original request
/// echoed for transparency). Detection keywords are routing data, not output.
pub fn detect(query: &str) -> &'static str {
    let q = query.to_lowercase();
    if q.trim().is_empty() {
        return "en";
    }
    // Indonesian markers: used only to recognize the query language.
    let id_markers = [
        "kamu",
        "saya",
        "yang",
        "dengan",
        "bagaimana",
        "buatkan",
        "tolong",
        "cara",
        "apa",
        "siapa",
        "berapa",
        "tulis",
        "bikin",
        "perbaiki",
        "tambahkan",
        "jelaskan",
        "pengembangan",
        "dikembangkan",
        "anak",
        "muda",
        "teknik",
        "informatika",
        "tugas",
        "catatan",
        "harian",
        "keamanan",
        "siber",
        "jalankan",
        "perintah",
    ];
    let en_markers = [
        "how", "what", "who", "please", "create", "make", "write", "fix", "explain", "show",
        "give", "the", "and",
    ];
    let id_score = id_markers.iter().filter(|w| q.contains(*w)).count();
    let en_score = en_markers
        .iter()
        .filter(|w| q.split_whitespace().any(|tok| tok == **w) || q.contains(&format!(" {w} ")))
        .count();
    if id_score > 0 && id_score >= en_score {
        "id"
    } else {
        "en"
    }
}

/// Returns true when the query was written in Indonesian. Callers use this
/// to echo the original phrasing, never to switch the answer language.
pub fn is_indonesian(query: &str) -> bool {
    detect(query) == "id"
}

/// System prompt anchor. English-only: the model always answers in English.
/// Identity is enforced here so a connected model answers correctly without
/// any hardcoded reply block.
pub fn system_for(_lang: &str) -> String {
    format!("{} Always answer in English.", crate::ZAI_SYSTEM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_id_en() {
        assert_eq!(detect("kamu siapa?"), "id");
        assert_eq!(detect("buatkan fungsi fibonacci"), "id");
        assert_eq!(detect("who are you please explain"), "en");
        assert_eq!(detect("please create a REST API and show me how"), "en");
        assert_eq!(detect(""), "en");
        assert_eq!(detect("hello"), "en");
    }

    #[test]
    fn system_is_english_only() {
        let s = system_for("id");
        assert!(s.contains("Always answer in English"));
        assert!(!s.contains("Bahasa Indonesia"));
    }
}
