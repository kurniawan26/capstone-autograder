use std::io::{Cursor, Read};
use zip::ZipArchive;

pub struct SourceFile {
    pub path: String,
    pub content: String,
}

pub struct Scan {
    pub files: Vec<SourceFile>,
    pub python: Vec<SourceFile>,
    pub notebooks: Vec<SourceFile>,
    pub markup: Vec<SourceFile>,
    pub manifests: Vec<SourceFile>,
    pub all_paths: Vec<String>,
    pub skipped: usize,
    pub bytes: usize,
    pub root: Option<String>,
    pub truncated: bool,
    pub languages_present: Vec<String>,
    pub languages_unsupported: Vec<String>,
    pub manifests_read: Vec<String>,
    pub manifests_unsupported: Vec<String>,
}

const MANIFESTS: &[&str] = &[
    "package.json",
    "requirements.txt",
    "pyproject.toml",
    "Pipfile",
    "go.mod",
    "composer.json",
    "pubspec.yaml",
    "build.gradle",
    "build.gradle.kts",
    "pom.xml",
    "Gemfile",
    "Cargo.toml",
];

const MANIFESTS_PARSED: &[&str] = &[
    "package.json",
    "requirements.txt",
    "pyproject.toml",
    "Pipfile",
    "go.mod",
    "composer.json",
];

pub fn manifest_name(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if path
        .split('/')
        .any(|segment| EXCLUDED_SEGMENTS.contains(&segment))
    {
        return None;
    }
    MANIFESTS.iter().copied().find(|known| *known == name)
}

pub fn is_manifest(path: &str) -> bool {
    manifest_name(path).is_some()
}

pub fn manifest_is_parsed(name: &str) -> bool {
    MANIFESTS_PARSED.contains(&name)
}

const LANGUAGE_BY_EXTENSION: &[(&str, &str)] = &[
    ("js", "javascript"),
    ("jsx", "javascript"),
    ("mjs", "javascript"),
    ("cjs", "javascript"),
    ("ts", "typescript"),
    ("tsx", "typescript"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("py", "python"),
    ("ipynb", "python"),
    ("go", "go"),
    ("php", "php"),
    ("rb", "ruby"),
    ("java", "java"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("dart", "dart"),
    ("rs", "rust"),
    ("cs", "csharp"),
    ("html", "html"),
    ("css", "css"),
];

pub const ANALYSED_LANGUAGES: &[&str] = &["javascript", "typescript", "html", "css", "python"];

pub fn is_python(path: &str) -> bool {
    if path
        .split('/')
        .any(|segment| EXCLUDED_SEGMENTS.contains(&segment))
    {
        return false;
    }
    path.to_ascii_lowercase().ends_with(".py")
}

pub fn is_notebook(path: &str) -> bool {
    if path
        .split('/')
        .any(|segment| EXCLUDED_SEGMENTS.contains(&segment))
    {
        return false;
    }
    path.to_ascii_lowercase().ends_with(".ipynb")
}

const MARKUP_EXTENSIONS: &[&str] = &["html", "htm", "css", "scss", "sass"];

pub fn is_markup(path: &str) -> bool {
    if path
        .split('/')
        .any(|segment| EXCLUDED_SEGMENTS.contains(&segment))
    {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains(".min.") {
        return false;
    }
    match lower.rsplit_once('.') {
        Some((_, ext)) => MARKUP_EXTENSIONS.contains(&ext),
        None => false,
    }
}

pub fn language_of(path: &str) -> Option<&'static str> {
    if path
        .split('/')
        .any(|segment| EXCLUDED_SEGMENTS.contains(&segment))
    {
        return None;
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains(".min.") {
        return None;
    }
    let (_, ext) = lower.rsplit_once('.')?;
    LANGUAGE_BY_EXTENSION
        .iter()
        .find(|(candidate, _)| *candidate == ext)
        .map(|(_, language)| *language)
}

const MAX_FILES: usize = 3_000;
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 5_000;
const MAX_NOTEBOOK_BYTES: usize = 16 * 1024 * 1024;

const EXCLUDED_SEGMENTS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".output",
    "vendor",
    "bower_components",
    "coverage",
    ".cache",
    "__pycache__",
    ".venv",
];

const EXTENSIONS: &[&str] = &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];

pub fn is_candidate(path: &str) -> bool {
    if path.ends_with('/') {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains(".min.") || lower.ends_with(".d.ts") {
        return false;
    }
    if lower
        .split('/')
        .any(|segment| EXCLUDED_SEGMENTS.contains(&segment))
    {
        return false;
    }
    match lower.rsplit_once('.') {
        Some((_, ext)) => EXTENSIONS.contains(&ext),
        None => false,
    }
}

fn looks_minified(content: &str) -> bool {
    content.lines().any(|line| line.len() > MAX_LINE_BYTES)
}

fn common_root(paths: &[String]) -> Option<String> {
    let first = paths.first()?;
    let candidate = first.split('/').next()?.to_string();
    if candidate.is_empty() || !first.contains('/') {
        return None;
    }
    let prefix = format!("{}/", candidate);
    if paths.iter().all(|p| p.starts_with(&prefix)) {
        Some(candidate)
    } else {
        None
    }
}

fn entry_name(entry: &zip::read::ZipFile<'_, Cursor<&[u8]>>) -> Option<String> {
    entry
        .enclosed_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
}

pub fn scan_zip(zip_bytes: &[u8]) -> Result<Scan, String> {
    let mut archive =
        ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| format!("open zip: {e}"))?;

    let mut all_names = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("read zip entry {index}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        if let Some(name) = entry_name(&entry) {
            all_names.push(name);
        }
    }
    let root = common_root(&all_names);

    let mut files = Vec::new();
    let mut python = Vec::new();
    let mut notebooks = Vec::new();
    let mut markup = Vec::new();
    let mut manifests = Vec::new();
    let mut skipped = 0usize;
    let mut bytes = 0usize;
    let mut truncated = false;

    for index in 0..archive.len() {
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(e) => return Err(format!("read zip entry {index}: {e}")),
        };

        let name = match entry.enclosed_name() {
            Some(name) => name.to_string_lossy().replace('\\', "/"),
            None => {
                skipped += 1;
                continue;
            }
        };

        let manifest = is_manifest(&name);
        let markup_file = !manifest && is_markup(&name);
        let python_file = !manifest && is_python(&name);
        let notebook_file = !manifest && is_notebook(&name);
        if !manifest && !markup_file && !python_file && !notebook_file && !is_candidate(&name) {
            skipped += 1;
            continue;
        }

        let size_limit = if notebook_file {
            MAX_NOTEBOOK_BYTES
        } else {
            MAX_FILE_BYTES
        };
        if entry.size() as usize > size_limit {
            skipped += 1;
            continue;
        }

        if files.len() >= MAX_FILES || bytes >= MAX_TOTAL_BYTES {
            truncated = true;
            skipped += 1;
            continue;
        }

        let mut raw = Vec::with_capacity(entry.size() as usize);
        if entry.read_to_end(&mut raw).is_err() {
            skipped += 1;
            continue;
        }

        let content = match String::from_utf8(raw) {
            Ok(content) => content,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        if !notebook_file && looks_minified(&content) {
            skipped += 1;
            continue;
        }

        bytes += content.len();
        let source_file = SourceFile {
            path: name,
            content,
        };
        if manifest {
            manifests.push(source_file);
        } else if markup_file {
            markup.push(source_file);
        } else if python_file {
            python.push(source_file);
        } else if notebook_file {
            notebooks.push(source_file);
        } else {
            files.push(source_file);
        }
    }

    let mut all_paths = all_names;
    if let Some(prefix) = &root {
        let strip = format!("{}/", prefix);
        let trim = |path: &mut String| {
            if let Some(rest) = path.strip_prefix(&strip) {
                *path = rest.to_string();
            }
        };
        for file in &mut files {
            trim(&mut file.path);
        }
        for file in &mut markup {
            trim(&mut file.path);
        }
        for file in &mut python {
            trim(&mut file.path);
        }
        for file in &mut notebooks {
            trim(&mut file.path);
        }
        for file in &mut manifests {
            trim(&mut file.path);
        }
        for path in &mut all_paths {
            trim(path);
        }
    }

    let mut languages_present: Vec<String> = Vec::new();
    let mut manifests_present: Vec<String> = Vec::new();
    for path in &all_paths {
        if let Some(language) = language_of(path) {
            let owned = language.to_string();
            if !languages_present.contains(&owned) {
                languages_present.push(owned);
            }
        }
        if let Some(name) = manifest_name(path) {
            let owned = name.to_string();
            if !manifests_present.contains(&owned) {
                manifests_present.push(owned);
            }
        }
    }
    languages_present.sort();
    manifests_present.sort();

    let languages_unsupported: Vec<String> = languages_present
        .iter()
        .filter(|language| !ANALYSED_LANGUAGES.contains(&language.as_str()))
        .cloned()
        .collect();

    let (manifests_read, manifests_unsupported): (Vec<String>, Vec<String>) = manifests_present
        .iter()
        .cloned()
        .partition(|name| manifest_is_parsed(name));

    Ok(Scan {
        files,
        python,
        notebooks,
        markup,
        manifests,
        all_paths,
        skipped,
        bytes,
        root,
        truncated,
        languages_present,
        languages_unsupported,
        manifests_read,
        manifests_unsupported,
    })
}

pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0usize];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self { starts }
    }

    pub fn locate(&self, offset: usize) -> (usize, usize) {
        match self.starts.binary_search(&offset) {
            Ok(index) => (index + 1, 1),
            Err(index) => {
                let line = index.saturating_sub(1);
                let start = self.starts.get(line).copied().unwrap_or(0);
                (line + 1, offset.saturating_sub(start) + 1)
            }
        }
    }
}

pub fn snippet(source: &str, start: usize, end: usize) -> String {
    let clamped_end = end.min(source.len()).min(start + 240);
    let raw = source.get(start..clamped_end).unwrap_or("");
    let single_line = raw.lines().next().unwrap_or("").trim();
    if raw.len() < end - start || raw.lines().count() > 1 {
        format!("{single_line} …")
    } else {
        single_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            for (name, content) in entries {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(content.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn keeps_project_directories() {
        let zip = make_zip(&[
            ("index.html", "<!doctype html>"),
            ("js/api.js", "fetch('/x')"),
            ("js/main.js", "console.log(1)"),
        ]);
        let scan = scan_zip(&zip).unwrap();
        assert_eq!(scan.root, None, "js/ bukan wrapper, tidak boleh di-strip");
        let paths: Vec<&str> = scan.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"js/api.js"), "dapat: {paths:?}");
    }

    #[test]
    fn strips_wrapper_directory() {
        let zip = make_zip(&[
            ("submission-123/index.html", "<!doctype html>"),
            ("submission-123/js/api.js", "fetch('/x')"),
        ]);
        let scan = scan_zip(&zip).unwrap();
        assert_eq!(scan.root.as_deref(), Some("submission-123"));
        let paths: Vec<&str> = scan.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["js/api.js"]);
    }

    #[test]
    fn skips_dependencies_and_bundles() {
        let zip = make_zip(&[
            ("js/app.js", "console.log(1)"),
            ("node_modules/axios/index.js", "fetch('/x')"),
            ("dist/bundle.min.js", "fetch('/x')"),
            ("vendor/jquery.js", "$.ajax({})"),
        ]);
        let scan = scan_zip(&zip).unwrap();
        let paths: Vec<&str> = scan.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["js/app.js"], "hanya kode siswa yang dianalisis");
        assert_eq!(scan.skipped, 3);
    }

    #[test]
    fn line_index_locates_positions() {
        let source = "satu\ndua\ntiga";
        let index = LineIndex::new(source);
        assert_eq!(index.locate(0).0, 1);
        assert_eq!(index.locate(5).0, 2);
        assert_eq!(index.locate(9).0, 3);
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            for (name, content) in entries {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(content.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn reports_unsupported_languages() {
        let zip = make_zip(&[
            ("web/app.js", "fetch('/x')"),
            ("backend/main.go", "package main"),
            ("web/index.php", "<?php"),
            ("ml/train.py", "import tensorflow"),
            ("go.mod", "module x"),
            ("index.html", "<!doctype html>"),
        ]);
        let scan = scan_zip(&zip).unwrap();
        assert!(scan.languages_present.contains(&"go".to_string()));
        assert!(scan.languages_unsupported.contains(&"go".to_string()));
        assert!(scan.languages_unsupported.contains(&"php".to_string()));
        assert!(
            !scan.languages_unsupported.contains(&"python".to_string()),
            "python sekarang dianalisis lewat tree-sitter, bukan gap"
        );
        assert!(
            !scan
                .languages_unsupported
                .contains(&"javascript".to_string()),
            "javascript dianalisis, bukan gap"
        );
        assert!(
            !scan.languages_unsupported.contains(&"html".to_string()),
            "html bukan gap yang membuat check ragu"
        );
        assert!(scan.manifests_read.contains(&"go.mod".to_string()));
        assert_eq!(scan.python.len(), 1, "ml/train.py harus dikumpulkan");
    }

    #[test]
    fn collects_notebooks_separately() {
        let zip = make_zip(&[
            ("notebooks/eda.ipynb", "{\"cells\":[]}"),
            ("ml/train.py", "import tensorflow as tf"),
            ("app.js", "fetch('/x')"),
        ]);
        let scan = scan_zip(&zip).unwrap();
        assert_eq!(scan.notebooks.len(), 1);
        assert_eq!(scan.python.len(), 1);
        assert_eq!(scan.files.len(), 1);
    }

    #[test]
    fn pure_javascript_has_no_gap() {
        let zip = make_zip(&[("app.js", "fetch('/x')"), ("package.json", "{}")]);
        let scan = scan_zip(&zip).unwrap();
        assert!(scan.languages_unsupported.is_empty());
        assert!(scan.manifests_unsupported.is_empty());
    }
}
