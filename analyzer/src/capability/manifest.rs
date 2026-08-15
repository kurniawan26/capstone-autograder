use serde_json::Value;

use crate::source::SourceFile;

#[derive(Debug, Clone)]
pub struct Finding {
    pub capability: &'static str,
    pub matched: String,
    pub file: String,
    pub line: usize,
    pub snippet: String,
}

struct Rule {
    capability: &'static str,
    packages: &'static [&'static str],
    config_files: &'static [&'static str],
}

const RULES: &[Rule] = &[
    Rule {
        capability: "module_bundler",
        packages: &[
            "vite",
            "webpack",
            "parcel",
            "rollup",
            "esbuild",
            "@parcel/core",
            "snowpack",
            "turbopack",
        ],
        config_files: &[
            "vite.config.js",
            "vite.config.ts",
            "vite.config.mjs",
            "webpack.config.js",
            "webpack.config.ts",
            "webpack.common.js",
            "webpack.prod.js",
            "rollup.config.js",
            "rollup.config.mjs",
            ".parcelrc",
        ],
    },
    Rule {
        capability: "rest_api_server",
        packages: &[
            "express",
            "@hapi/hapi",
            "hapi",
            "fastify",
            "koa",
            "restify",
            "nest",
            "@nestjs/core",
            "flask",
            "fastapi",
            "django",
            "github.com/gin-gonic/gin",
            "github.com/gorilla/mux",
            "github.com/labstack/echo/v4",
            "github.com/gofiber/fiber/v2",
            "github.com/go-chi/chi/v5",
            "slim/slim",
            "laravel/framework",
            "symfony/framework-bundle",
            "codeigniter4/framework",
        ],
        config_files: &[],
    },
    Rule {
        capability: "database_persistence",
        packages: &[
            "mongoose",
            "mongodb",
            "pg",
            "mysql",
            "mysql2",
            "sequelize",
            "prisma",
            "@prisma/client",
            "sqlite3",
            "better-sqlite3",
            "knex",
            "typeorm",
            "lowdb",
            "firebase",
            "firebase-admin",
            "@supabase/supabase-js",
            "psycopg2",
            "psycopg2-binary",
            "sqlalchemy",
            "pymongo",
            "github.com/lib/pq",
            "github.com/go-sql-driver/mysql",
            "github.com/jackc/pgx/v5",
            "gorm.io/gorm",
            "go.mongodb.org/mongo-driver",
            "doctrine/orm",
            "illuminate/database",
        ],
        config_files: &["prisma/schema.prisma"],
    },
    Rule {
        capability: "ml_in_app",
        packages: &[
            "@tensorflow/tfjs",
            "@tensorflow/tfjs-node",
            "@tensorflow/tfjs-converter",
            "@tensorflow-models/mobilenet",
            "onnxruntime-web",
            "onnxruntime-node",
            "@xenova/transformers",
            "@huggingface/transformers",
            "tensorflow",
            "tensorflow-cpu",
            "tensorflow-gpu",
            "tensorflow-macos",
            "tensorflow-metal",
            "tf-nightly",
            "tflite-runtime",
            "torch",
            "scikit-learn",
            "keras",
            "github.com/galeone/tfgo",
        ],
        config_files: &[],
    },
    Rule {
        capability: "llm_api_client",
        packages: &[
            "openai",
            "@anthropic-ai/sdk",
            "anthropic",
            "@google/generative-ai",
            "google-generativeai",
            "google-genai",
            "cohere-ai",
            "replicate",
            "@mistralai/mistralai",
            "openai-php/client",
            "github.com/sashabaranov/go-openai",
            "google/generative-ai-php",
        ],
        config_files: &[],
    },
    Rule {
        capability: "automl_service",
        packages: &[
            "google-cloud-automl",
            "@google-cloud/automl",
            "google-cloud-aiplatform",
            "azureml-automl-core",
            "auto-sklearn",
            "autokeras",
            "h2o",
            "tpot",
            "pycaret",
        ],
        config_files: &[],
    },
    Rule {
        capability: "pretrained_model_hub",
        packages: &[
            "tensorflow-hub",
            "tensorflow_hub",
            "@tensorflow-models/coco-ssd",
            "@tensorflow-models/posenet",
            "timm",
            "huggingface-hub",
            "easyocr",
            "paddleocr",
            "pytesseract",
            "ultralytics",
            "face-recognition",
            "face_recognition",
            "mediapipe",
            "insightface",
            "deepface",
        ],
        config_files: &[],
    },
    Rule {
        capability: "http_request",
        packages: &[
            "axios",
            "ky",
            "superagent",
            "got",
            "node-fetch",
            "requests",
            "httpx",
            "aiohttp",
            "urllib3",
            "guzzlehttp/guzzle",
            "symfony/http-client",
            "github.com/go-resty/resty/v2",
        ],
        config_files: &[],
    },
    Rule {
        capability: "express_framework",
        packages: &["express"],
        config_files: &[],
    },
    Rule {
        capability: "ml_serving_api",
        packages: &[
            "fastapi", "flask", "uvicorn", "gunicorn", "litestar", "quart",
        ],
        config_files: &[],
    },
    Rule {
        capability: "css_framework",
        packages: &["tailwindcss", "bootstrap", "@tailwindcss/cli", "bulma"],
        config_files: &[
            "tailwind.config.js",
            "tailwind.config.ts",
            "tailwind.config.cjs",
        ],
    },
    Rule {
        capability: "tensorboard_integration",
        packages: &["tensorboard", "tensorboardx"],
        config_files: &[],
    },
    Rule {
        capability: "streamlit_dashboard",
        packages: &["streamlit"],
        config_files: &[".streamlit/config.toml"],
    },
];

fn line_of(content: &str, needle: &str) -> usize {
    content
        .lines()
        .position(|line| line.contains(needle))
        .map(|index| index + 1)
        .unwrap_or(1)
}

fn package_json_dependencies(content: &str) -> Vec<String> {
    let parsed: Value = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut names = Vec::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(Value::Object(map)) = parsed.get(section) {
            names.extend(map.keys().cloned());
        }
    }
    names
}

fn requirements_packages(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty() && !line.starts_with('-'))
        .map(|line| {
            line.split(['=', '<', '>', '!', '~', '[', ';', ' '])
                .next()
                .unwrap_or(line)
                .trim()
                .to_ascii_lowercase()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

fn composer_dependencies(content: &str) -> Vec<String> {
    let parsed: Value = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let mut names = Vec::new();
    for section in ["require", "require-dev"] {
        if let Some(Value::Object(map)) = parsed.get(section) {
            names.extend(map.keys().cloned());
        }
    }
    names
}

fn go_mod_modules(content: &str) -> Vec<String> {
    let mut modules = Vec::new();
    let mut in_block = false;

    for raw in content.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("require (") {
            in_block = true;
            continue;
        }
        if in_block {
            if line == ")" {
                in_block = false;
                continue;
            }
            if let Some(module) = line.split_whitespace().next() {
                modules.push(module.to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("require ") {
            if let Some(module) = rest.split_whitespace().next() {
                modules.push(module.to_string());
            }
        }
    }
    modules
}

fn declared_packages(file: &SourceFile) -> Vec<String> {
    let name = file.path.rsplit('/').next().unwrap_or(&file.path);
    match name {
        "package.json" => package_json_dependencies(&file.content),
        "requirements.txt" | "Pipfile" | "pyproject.toml" => requirements_packages(&file.content),
        "composer.json" => composer_dependencies(&file.content),
        "go.mod" => go_mod_modules(&file.content),
        _ => Vec::new(),
    }
}

pub fn analyze(manifests: &[SourceFile], all_paths: &[String]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for manifest in manifests {
        let packages = declared_packages(manifest);
        for rule in RULES {
            for package in &packages {
                let matches = rule
                    .packages
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(package));
                if !matches {
                    continue;
                }
                let line = line_of(&manifest.content, package);
                findings.push(Finding {
                    capability: rule.capability,
                    matched: package.clone(),
                    file: manifest.path.clone(),
                    line,
                    snippet: format!("\"{package}\" dideklarasikan di {}", manifest.path),
                });
            }
        }
    }

    for rule in RULES {
        for config in rule.config_files {
            if let Some(found) = all_paths.iter().find(|path| {
                path.eq_ignore_ascii_case(config)
                    || path
                        .rsplit('/')
                        .next()
                        .map(|name| name.eq_ignore_ascii_case(config))
                        .unwrap_or(false)
            }) {
                findings.push(Finding {
                    capability: rule.capability,
                    matched: (*config).to_string(),
                    file: found.clone(),
                    line: 1,
                    snippet: format!("berkas konfigurasi {found} ada di submission"),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(path: &str, content: &str) -> SourceFile {
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
    fn detects_bundler_from_dev_dependencies() {
        let files = vec![manifest(
            "package.json",
            r#"{"devDependencies":{"vite":"^5.0.0"}}"#,
        )];
        assert!(caps(&analyze(&files, &[])).contains(&"module_bundler"));
    }

    #[test]
    fn detects_bundler_from_config_file_alone() {
        let paths = vec!["vite.config.js".to_string()];
        assert!(caps(&analyze(&[], &paths)).contains(&"module_bundler"));
    }

    #[test]
    fn detects_forbidden_llm_api_and_hub() {
        let files = vec![
            manifest("package.json", r#"{"dependencies":{"openai":"^4.0.0"}}"#),
            manifest(
                "requirements.txt",
                "tensorflow==2.16.1\ntensorflow-hub>=0.16\n",
            ),
        ];
        let found = caps(&analyze(&files, &[]));
        assert!(found.contains(&"llm_api_client"));
        assert!(found.contains(&"pretrained_model_hub"));
        assert!(found.contains(&"ml_in_app"));
    }

    #[test]
    fn requirements_comments_ignored() {
        let files = vec![manifest(
            "requirements.txt",
            "# openai adalah contoh yang dilarang\npandas==2.2.0\n",
        )];
        assert!(
            !caps(&analyze(&files, &[])).contains(&"llm_api_client"),
            "komentar di requirements.txt tidak boleh dihitung"
        );
    }

    #[test]
    fn detects_streamlit_and_server() {
        let files = vec![manifest(
            "requirements.txt",
            "streamlit==1.36.0\nflask==3.0.0\n",
        )];
        let found = caps(&analyze(&files, &[]));
        assert!(found.contains(&"streamlit_dashboard"));
        assert!(found.contains(&"rest_api_server"));
    }
}

#[cfg(test)]
mod polyglot {
    use super::*;

    fn manifest(path: &str, content: &str) -> SourceFile {
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
    fn reads_go_mod_require_block() {
        let files = vec![manifest(
            "go.mod",
            "module capstone/backend\n\ngo 1.22\n\nrequire (\n\tgithub.com/gorilla/mux v1.8.1\n\tgithub.com/lib/pq v1.10.9\n)\n",
        )];
        let found = caps(&analyze(&files, &[]));
        assert!(found.contains(&"rest_api_server"), "dapat: {found:?}");
        assert!(found.contains(&"database_persistence"), "dapat: {found:?}");
    }

    #[test]
    fn reads_go_mod_single_line_require() {
        let files = vec![manifest(
            "go.mod",
            "module x\nrequire github.com/gin-gonic/gin v1.10.0 // indirect comment\n",
        )];
        assert!(caps(&analyze(&files, &[])).contains(&"rest_api_server"));
    }

    #[test]
    fn reads_composer_json() {
        let files = vec![manifest(
            "composer.json",
            r#"{"require":{"slim/slim":"^4.13","guzzlehttp/guzzle":"^7.8","doctrine/orm":"^3.0"}}"#,
        )];
        let found = caps(&analyze(&files, &[]));
        assert!(found.contains(&"rest_api_server"));
        assert!(found.contains(&"http_request"));
        assert!(found.contains(&"database_persistence"));
    }

    #[test]
    fn go_mod_comments_ignored() {
        let files = vec![manifest(
            "go.mod",
            "module x\n// require github.com/sashabaranov/go-openai v1.0.0\nrequire github.com/gorilla/mux v1.8.1\n",
        )];
        assert!(
            !caps(&analyze(&files, &[])).contains(&"llm_api_client"),
            "modul yang dikomentari di go.mod tidak boleh dihitung"
        );
    }
}
