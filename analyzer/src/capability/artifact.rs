#[derive(Debug, Clone)]
pub struct Finding {
    pub capability: &'static str,
    pub matched: String,
    pub file: String,
    pub snippet: String,
}

struct Rule {
    capability: &'static str,
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
    path_contains: &'static [&'static str],
    label: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        capability: "tensorboard_integration",
        extensions: &[],
        filenames: &[],
        path_contains: &["events.out.tfevents", "tensorboard"],
        label: "log TensorBoard",
    },
    Rule {
        capability: "saved_model_artifact",
        extensions: &["keras", "h5", "tflite", "onnx"],
        filenames: &["saved_model.pb", "model.safetensors"],
        path_contains: &["saved_model/"],
        label: "model terlatih",
    },
    Rule {
        capability: "notebook_present",
        extensions: &["ipynb"],
        filenames: &[],
        path_contains: &[],
        label: "notebook",
    },
];

fn extension_of(path: &str) -> Option<&str> {
    path.rsplit_once('.').map(|(_, ext)| ext)
}

pub fn analyze(all_paths: &[String]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for path in all_paths {
        let lower = path.to_ascii_lowercase();
        let name = lower.rsplit('/').next().unwrap_or(&lower).to_string();

        for rule in RULES {
            let by_extension = extension_of(&lower)
                .map(|ext| rule.extensions.contains(&ext))
                .unwrap_or(false);
            let by_name = rule.filenames.contains(&name.as_str());
            let by_path = rule
                .path_contains
                .iter()
                .any(|fragment| lower.contains(fragment));

            if by_extension || by_name || by_path {
                findings.push(Finding {
                    capability: rule.capability,
                    matched: rule.label.to_string(),
                    file: path.clone(),
                    snippet: format!("{} ditemukan di submission: {path}", rule.label),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    fn caps(findings: &[Finding]) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = findings.iter().map(|f| f.capability).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    #[test]
    fn detects_saved_model_formats() {
        for path in [
            "ml/model.keras",
            "ml/saved_model/saved_model.pb",
            "ml/model.tflite",
        ] {
            let found = caps(&analyze(&paths(&[path])));
            assert!(
                found.contains(&"saved_model_artifact"),
                "gagal mendeteksi {path}"
            );
        }
    }

    #[test]
    fn detects_logs_and_notebook() {
        let found = caps(&analyze(&paths(&[
            "docs/laporan-teknis.pdf",
            "design/mockup-beranda.png",
            "logs/train/events.out.tfevents.1700000000.host",
            "notebooks/eda.ipynb",
        ])));
        assert!(found.contains(&"tensorboard_integration"));
        assert!(found.contains(&"notebook_present"));
    }

    #[test]
    fn plain_project_has_no_false_findings() {
        let found = caps(&analyze(&paths(&[
            "index.html",
            "js/app.js",
            "css/style.css",
            "package.json",
        ])));
        assert!(found.is_empty(), "tidak boleh ada temuan palsu: {found:?}");
    }
}
