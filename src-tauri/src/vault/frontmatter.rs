use crate::types::NoteMeta;
use chrono::{NaiveDate, NaiveDateTime, Utc};
use gray_matter::engine::YAML;
use gray_matter::Matter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
struct RawFrontmatter {
    id: Option<String>,
    title: Option<String>,
    tags: Option<Vec<String>>,
    pinned: Option<bool>,
    created: Option<String>,
    modified: Option<String>,
}

/// Try to parse a date string in multiple common formats.
pub fn parse_date_flexible(s: &str) -> Option<chrono::DateTime<Utc>> {
    let s = s.trim();
    // RFC 3339 / ISO 8601 with timezone (e.g. 2024-01-15T10:30:00+00:00)
    if let Ok(dt) = s.parse::<chrono::DateTime<Utc>>() {
        return Some(dt);
    }
    // Try parsing as DateTime with fixed offset then convert
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // YYYY-MM-DD HH:MM:SS (no timezone)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(ndt.and_utc());
    }
    // YYYY-MM-DDTHH:MM:SS (no timezone, with T separator)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(ndt.and_utc());
    }
    // YYYY-MM-DD (date only)
    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return nd.and_hms_opt(0, 0, 0).map(|ndt| ndt.and_utc());
    }
    None
}

pub fn parse_note(raw: &str, filename: &str) -> (NoteMeta, String) {
    let matter = Matter::<YAML>::new();
    let result = matter.parse(raw);

    let fm: RawFrontmatter = result
        .data
        .and_then(|d| d.deserialize().ok())
        .unwrap_or_default();

    let title = fm.title.unwrap_or_else(|| filename_to_title(filename));

    let created = fm
        .created
        .and_then(|s| parse_date_flexible(&s))
        .unwrap_or_else(Utc::now);

    let modified = fm
        .modified
        .and_then(|s| parse_date_flexible(&s))
        .unwrap_or_else(Utc::now);

    // Don't generate UUID on read - empty string signals "no ID yet"
    let id = fm.id.unwrap_or_default();

    let meta = NoteMeta {
        id,
        title,
        tags: fm.tags.unwrap_or_default(),
        pinned: fm.pinned.unwrap_or(false),
        created,
        modified,
    };

    let content = result.content;
    (meta, content)
}

/// Ensure the body starts with `# Title` heading for external app compatibility.
/// If the body already starts with `# Title`, leave it as-is.
fn ensure_title_heading(body: &str, title: &str) -> String {
    let trimmed = body.trim_start();

    // Check if body already starts with `# Title`
    if let Some(rest) = trimmed.strip_prefix("# ") {
        let first_line_end = rest.find('\n').unwrap_or(rest.len());
        let heading_text = rest[..first_line_end].trim();
        if heading_text.eq_ignore_ascii_case(title.trim()) {
            return body.to_string();
        }
    }

    // Don't add heading to empty notes (new/blank notes)
    if trimmed.is_empty() {
        return body.to_string();
    }

    // Prepend `# Title` heading
    format!("# {}\n\n{}", title, body)
}

pub fn serialize_frontmatter(meta: &NoteMeta) -> String {
    let tags_str = if meta.tags.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            meta.tags
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    format!(
        "---\nid: \"{}\"\ntitle: \"{}\"\ntags: {}\npinned: {}\ncreated: {}\nmodified: {}\n---\n",
        meta.id,
        meta.title.replace('"', "\\\""),
        tags_str,
        meta.pinned,
        meta.created.to_rfc3339(),
        meta.modified.to_rfc3339(),
    )
}

pub fn update_note_raw(meta: &NoteMeta, body: &str) -> String {
    let fm = serialize_frontmatter(meta);
    let body_with_heading = ensure_title_heading(body, &meta.title);
    format!("{}{}", fm, body_with_heading)
}

/// Merge NoteMeta fields into existing frontmatter, preserving unknown YAML keys.
/// Falls back to serialize_frontmatter() if the original has no frontmatter.
pub fn merge_frontmatter(original_raw: &str, meta: &NoteMeta, body: &str) -> String {
    let matter = Matter::<YAML>::new();
    let parsed = matter.parse(original_raw);

    // If original had no frontmatter, use the simple serializer
    if parsed.matter.trim().is_empty() {
        return update_note_raw(meta, body);
    }

    // Parse existing YAML into a Mapping to preserve all keys
    let mut mapping: serde_yaml::Mapping = match serde_yaml::from_str(&parsed.matter) {
        Ok(m) => m,
        Err(_) => return update_note_raw(meta, body), // unparseable YAML, fall back
    };

    // Update only our known fields
    if !meta.id.is_empty() {
        mapping.insert(
            serde_yaml::Value::String("id".into()),
            serde_yaml::Value::String(meta.id.clone()),
        );
    }
    mapping.insert(
        serde_yaml::Value::String("title".into()),
        serde_yaml::Value::String(meta.title.clone()),
    );
    mapping.insert(
        serde_yaml::Value::String("tags".into()),
        serde_yaml::Value::Sequence(
            meta.tags
                .iter()
                .map(|t| serde_yaml::Value::String(t.clone()))
                .collect(),
        ),
    );
    mapping.insert(
        serde_yaml::Value::String("pinned".into()),
        serde_yaml::Value::Bool(meta.pinned),
    );
    mapping.insert(
        serde_yaml::Value::String("created".into()),
        serde_yaml::Value::String(meta.created.to_rfc3339()),
    );
    mapping.insert(
        serde_yaml::Value::String("modified".into()),
        serde_yaml::Value::String(meta.modified.to_rfc3339()),
    );

    // Serialize the mapping back to YAML
    let yaml_str = match serde_yaml::to_string(&serde_yaml::Value::Mapping(mapping)) {
        Ok(s) => s,
        Err(_) => return update_note_raw(meta, body),
    };

    let body_with_heading = ensure_title_heading(body, &meta.title);
    format!("---\n{}---\n{}", yaml_str, body_with_heading)
}

pub fn filename_to_title(filename: &str) -> String {
    let stem = filename.trim_end_matches(".md");
    // Only replace dashes/underscores acting as word separators (between non-space chars).
    // Keep dashes that are surrounded by spaces (e.g. "Title - Subtitle").
    let mut result = String::with_capacity(stem.len());
    let chars: Vec<char> = stem.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if (ch == '-' || ch == '_')
            && i > 0
            && i < chars.len() - 1
            && chars[i - 1] != ' '
            && chars[i + 1] != ' '
        {
            result.push(' ');
        } else {
            result.push(ch);
        }
    }
    result
}

/// Quick title extraction from raw note content (frontmatter only, no full parse)
pub fn extract_title(raw: &str) -> Option<String> {
    let matter = Matter::<YAML>::new();
    let result = matter.parse(raw);
    let fm: RawFrontmatter = result
        .data
        .and_then(|d| d.deserialize().ok())
        .unwrap_or_default();
    fm.title
}

pub fn extract_preview(content: &str, max_len: usize) -> String {
    // Filter out headings and empty lines BEFORE stripping markdown,
    // since strip_html_and_markdown collapses all whitespace into one line.
    let filtered: String = content
        .lines()
        .filter(|l| {
            let trimmed = l.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("---")
                && !trimmed.starts_with("```")
        })
        .take(5)
        .collect::<Vec<_>>()
        .join("\n");

    let text = strip_html_and_markdown(&filtered);
    let trimmed = text.trim().to_string();

    if trimmed.len() > max_len {
        let mut end = max_len;
        while end > 0 && !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &trimmed[..end])
    } else {
        trimmed
    }
}

fn strip_html_and_markdown(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_tag = false;

    while let Some(ch) = chars.next() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                // Add a space after closing tags to separate words
                result.push(' ');
            }
            continue;
        }

        if ch == '<' {
            in_tag = true;
            continue;
        }

        // Skip markdown image syntax: ![alt](url)
        if ch == '!' && chars.peek() == Some(&'[') {
            // Consume ![...](...) entirely
            chars.next(); // skip '['
            let mut depth = 1;
            // Skip alt text
            for c in chars.by_ref() {
                if c == '[' {
                    depth += 1;
                }
                if c == ']' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            // Skip (url) if present
            if chars.peek() == Some(&'(') {
                chars.next();
                let mut depth = 1;
                for c in chars.by_ref() {
                    if c == '(' {
                        depth += 1;
                    }
                    if c == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }
            continue;
        }

        // Markdown links: [text](url) - keep the text, skip the URL
        if ch == '[' {
            let mut link_text = String::new();
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '[' {
                    depth += 1;
                }
                if c == ']' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                link_text.push(c);
            }
            // Skip (url) if present
            if chars.peek() == Some(&'(') {
                chars.next();
                let mut depth = 1;
                for c in chars.by_ref() {
                    if c == '(' {
                        depth += 1;
                    }
                    if c == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                result.push_str(&link_text);
            } else {
                // Not a link, just brackets
                result.push('[');
                result.push_str(&link_text);
                result.push(']');
            }
            continue;
        }

        // Strip bold/italic markers
        if ch == '*' || ch == '_' {
            // Skip consecutive * or _
            while chars.peek() == Some(&ch) {
                chars.next();
            }
            continue;
        }

        // Strip strikethrough ~~
        if ch == '~' && chars.peek() == Some(&'~') {
            chars.next();
            continue;
        }

        // Strip inline code backticks
        if ch == '`' {
            while chars.peek() == Some(&'`') {
                chars.next();
            }
            continue;
        }

        result.push(ch);
    }

    // Collapse multiple spaces/newlines into single spaces
    let mut collapsed = String::with_capacity(result.len());
    let mut last_was_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                collapsed.push(' ');
                last_was_space = true;
            }
        } else {
            collapsed.push(ch);
            last_was_space = false;
        }
    }

    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_preview_basic() {
        let content =
            "## SIM Kaarten\n\n### Artjom\n\n**Telefoonnummer**\n\n**== 0456 74 49 91 ==**\n";
        let preview = extract_preview(content, 120);
        eprintln!("Preview: {:?}", preview);
        assert!(!preview.is_empty(), "Preview should not be empty");
    }

    #[test]
    fn test_extract_preview_no_frontmatter() {
        let content = "# Android flash\n\nAndroid flash\n\n[TWRP](https://twrp.me/)\n\nNotes;\n\n- Samsung S6\n";
        let preview = extract_preview(content, 120);
        eprintln!("Preview: {:?}", preview);
        assert!(!preview.is_empty(), "Preview should not be empty");
    }

    #[test]
    fn test_strip_html_markdown() {
        let input = "**Telefoonnummer**\n\n**== 0456 ==**";
        let stripped = strip_html_and_markdown(input);
        eprintln!("Stripped: {:?}", stripped);
        assert!(stripped.contains("Telefoonnummer"));
    }
}
