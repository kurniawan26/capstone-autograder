use crate::source::{LineIndex, SourceFile};

#[derive(Debug, Clone)]
pub struct Finding {
    pub capability: &'static str,
    pub matched: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub snippet: String,
}

fn blank_comments(content: &str, open: &str, close: &str) -> String {
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut index = 0usize;

    while index < content.len() {
        if content[index..].starts_with(open) {
            let end = content[index + open.len()..]
                .find(close)
                .map(|offset| index + open.len() + offset + close.len())
                .unwrap_or(content.len());
            for &byte in &bytes[index..end] {
                out.push(if byte == b'\n' { '\n' } else { ' ' });
            }
            index = end;
            continue;
        }
        let ch = content[index..].chars().next().unwrap();
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

fn is_css(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".css") || lower.ends_with(".scss") || lower.ends_with(".sass")
}

fn snippet_at(content: &str, offset: usize) -> String {
    let line = content[..offset]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(0);
    let end = content[offset..]
        .find('\n')
        .map(|position| offset + position)
        .unwrap_or(content.len());
    let raw = content[line..end].trim();
    if raw.len() > 160 {
        format!("{} …", &raw[..160])
    } else {
        raw.to_string()
    }
}

struct Needle {
    capability: &'static str,
    pattern: &'static str,
    label: &'static str,
}

const CSS_NEEDLES: &[Needle] = &[
    Needle {
        capability: "responsive_layout",
        pattern: "@media",
        label: "@media query",
    },
    Needle {
        capability: "css_framework",
        pattern: "@tailwind",
        label: "@tailwind directive",
    },
    Needle {
        capability: "responsive_layout",
        pattern: "grid-template-columns",
        label: "CSS grid layout",
    },
];

const HTML_NEEDLES: &[Needle] = &[
    Needle {
        capability: "responsive_layout",
        pattern: "viewport",
        label: "meta viewport",
    },
    Needle {
        capability: "css_framework",
        pattern: "tailwind",
        label: "Tailwind",
    },
    Needle {
        capability: "css_framework",
        pattern: "bootstrap",
        label: "Bootstrap",
    },
    Needle {
        capability: "semantic_html",
        pattern: "<main",
        label: "<main>",
    },
    Needle {
        capability: "semantic_html",
        pattern: "<header",
        label: "<header>",
    },
    Needle {
        capability: "semantic_html",
        pattern: "<nav",
        label: "<nav>",
    },
    Needle {
        capability: "semantic_html",
        pattern: "<section",
        label: "<section>",
    },
    Needle {
        capability: "semantic_html",
        pattern: "<article",
        label: "<article>",
    },
    Needle {
        capability: "semantic_html",
        pattern: "<footer",
        label: "<footer>",
    },
];

pub fn analyze(markup: &[SourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for file in markup {
        let css = is_css(&file.path);
        let cleaned = if css {
            blank_comments(&file.content, "/*", "*/")
        } else {
            blank_comments(&file.content, "<!--", "-->")
        };
        let lowered = cleaned.to_ascii_lowercase();
        let index = LineIndex::new(&cleaned);
        let needles = if css { CSS_NEEDLES } else { HTML_NEEDLES };

        for needle in needles {
            let mut from = 0usize;
            while let Some(offset) = lowered[from..].find(needle.pattern) {
                let absolute = from + offset;
                let (line, column) = index.locate(absolute);
                findings.push(Finding {
                    capability: needle.capability,
                    matched: needle.label.to_string(),
                    file: file.path.clone(),
                    line,
                    column,
                    snippet: snippet_at(&cleaned, absolute),
                });
                from = absolute + needle.pattern.len();
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            content: content.to_string(),
        }
    }

    fn caps(findings: &[Finding]) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = findings.iter().map(|f| f.capability).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    #[test]
    fn detects_media_query() {
        let files = vec![file(
            "style.css",
            "body { margin: 0 }\n@media (max-width: 768px) { body { font-size: 14px } }\n",
        )];
        let found = analyze(&files);
        assert!(caps(&found).contains(&"responsive_layout"));
        assert_eq!(found[0].line, 2, "nomor baris harus tepat");
    }

    #[test]
    fn ignores_css_comments() {
        let files = vec![file(
            "style.css",
            "/* nanti tambahkan @media (max-width: 768px) di sini */\nbody { margin: 0 }\n",
        )];
        assert!(
            !caps(&analyze(&files)).contains(&"responsive_layout"),
            "@media di dalam komentar CSS tidak boleh dihitung"
        );
    }

    #[test]
    fn ignores_html_comments() {
        let files = vec![file(
            "index.html",
            "<!-- <meta name=\"viewport\" content=\"width=device-width\"> masih TODO -->\n<title>x</title>\n",
        )];
        assert!(
            !caps(&analyze(&files)).contains(&"responsive_layout"),
            "meta viewport di dalam komentar HTML tidak boleh dihitung"
        );
    }

    #[test]
    fn detects_viewport_and_frameworks() {
        let files = vec![file(
            "index.html",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
             <link href=\"https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css\" rel=\"stylesheet\">\n\
             <main><section>hai</section></main>\n",
        )];
        let found = caps(&analyze(&files));
        assert!(found.contains(&"responsive_layout"));
        assert!(found.contains(&"css_framework"));
        assert!(found.contains(&"semantic_html"));
    }

    #[test]
    fn line_numbers_survive_comment_blanking() {
        let files = vec![file(
            "style.css",
            "/* baris satu\n   baris dua */\n@media screen { a { color: red } }\n",
        )];
        let found = analyze(&files);
        assert_eq!(
            found[0].line, 3,
            "blanking komentar harus menjaga nomor baris"
        );
    }
}

#[cfg(test)]
mod counting {
    use super::*;

    fn file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn tailwind_cdn_counted_once() {
        let files = vec![file(
            "index.html",
            "<script src=\"https://cdn.tailwindcss.com\"></script>\n",
        )];
        let hits = analyze(&files)
            .into_iter()
            .filter(|f| f.capability == "css_framework")
            .count();
        assert_eq!(
            hits, 1,
            "satu tautan Tailwind tidak boleh dihitung dua kali"
        );
    }

    #[test]
    fn each_semantic_tag_counted_separately() {
        let files = vec![file(
            "index.html",
            "<header>a</header><main>b</main><footer>c</footer>\n",
        )];
        let hits = analyze(&files)
            .into_iter()
            .filter(|f| f.capability == "semantic_html")
            .count();
        assert_eq!(hits, 3);
    }
}
