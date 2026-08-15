pub mod artifact;
pub mod js;
pub mod manifest;
pub mod markup;
pub mod python;

use crate::types::CapabilityInfo;

pub const JS: &[&str] = &["javascript", "typescript"];
pub const MANIFEST: &[&str] = &["package.json", "requirements.txt", "pyproject.toml"];
pub const MARKUP: &[&str] = &["html", "css"];
pub const FILES: &[&str] = &["berkas di dalam arsip"];
pub const PY: &[&str] = &["python", "notebook"];

pub const CATALOG: &[CapabilityInfo] = &[
    CapabilityInfo {
        id: "http_request",
        description: "Mengambil data lewat HTTP: fetch, axios, XMLHttpRequest, jQuery.ajax, ky, superagent",
        languages: JS,
    },
    CapabilityInfo {
        id: "async_await",
        description: "Menggunakan async/await",
        languages: JS,
    },
    CapabilityInfo {
        id: "promise_chain",
        description: "Merangkai Promise dengan .then/.catch/.finally",
        languages: JS,
    },
    CapabilityInfo {
        id: "error_handling",
        description: "Menangani kegagalan dengan try/catch atau .catch()",
        languages: JS,
    },
    CapabilityInfo {
        id: "dom_manipulation",
        description: "Membaca atau mengubah DOM lewat document/element API",
        languages: JS,
    },
    CapabilityInfo {
        id: "event_listener",
        description: "Merespons interaksi lewat addEventListener atau on-handler",
        languages: JS,
    },
    CapabilityInfo {
        id: "web_storage",
        description: "Menyimpan state di localStorage atau sessionStorage",
        languages: JS,
    },
    CapabilityInfo {
        id: "es_module",
        description: "Memecah kode dengan import/export ES module",
        languages: JS,
    },
    CapabilityInfo {
        id: "class_usage",
        description: "Mendefinisikan class",
        languages: JS,
    },
    CapabilityInfo {
        id: "array_iteration",
        description: "Mengolah koleksi dengan map/filter/reduce/forEach",
        languages: JS,
    },
    CapabilityInfo {
        id: "json_handling",
        description: "Mengurai atau menyusun JSON",
        languages: JS,
    },
    CapabilityInfo {
        id: "websocket",
        description: "Membuka koneksi WebSocket",
        languages: JS,
    },
    CapabilityInfo {
        id: "rest_route_definition",
        description: "Mendefinisikan endpoint HTTP di sisi server (app.get('/path'), router.post, server.route)",
        languages: JS,
    },
    CapabilityInfo {
        id: "module_bundler",
        description: "Membangun proyek dengan module bundler: Vite, webpack, Parcel, Rollup, esbuild",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "rest_api_server",
        description: "Memakai framework server HTTP: Express, Hapi, Fastify, Koa, NestJS, Flask, FastAPI",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "database_persistence",
        description: "Menyimpan data lewat database atau ORM",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "ml_in_app",
        description: "Menyertakan pustaka AI/ML: TensorFlow, tfjs, ONNX Runtime, transformers",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "llm_api_client",
        description: "Memanggil model lewat API pihak ketiga: OpenAI, Gemini, Anthropic, Cohere. Umumnya dipakai sebagai larangan (expect: absent)",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "automl_service",
        description: "Memakai AutoML: Vertex AutoML, AutoKeras, auto-sklearn, PyCaret. Umumnya dipakai sebagai larangan (expect: absent)",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "pretrained_model_hub",
        description: "Memakai model siap pakai dari TensorFlow Hub atau serupa. Umumnya dipakai sebagai larangan (expect: absent)",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "responsive_layout",
        description: "Layout menyesuaikan ukuran layar: @media query, meta viewport, atau CSS grid",
        languages: MARKUP,
    },
    CapabilityInfo {
        id: "css_framework",
        description: "Memakai Bootstrap atau Tailwind CSS",
        languages: MARKUP,
    },
    CapabilityInfo {
        id: "semantic_html",
        description: "Memakai elemen semantik: main, header, nav, section, article, footer",
        languages: MARKUP,
    },
    CapabilityInfo {
        id: "express_framework",
        description: "RESTful API dibangun dengan framework Express",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "ml_serving_api",
        description: "REST API mandiri untuk melayani model: FastAPI atau Flask",
        languages: MANIFEST,
    },
    CapabilityInfo {
        id: "tensorboard_integration",
        description: "TensorBoard terintegrasi dan log pelatihannya disertakan di repository",
        languages: FILES,
    },
    CapabilityInfo {
        id: "saved_model_artifact",
        description: "Model terlatih diekspor: .keras, SavedModel, .h5, .tflite",
        languages: FILES,
    },
    CapabilityInfo {
        id: "notebook_present",
        description: "Notebook .ipynb disertakan",
        languages: FILES,
    },
    CapabilityInfo {
        id: "tf_functional_api",
        description: "Membangun model dengan TensorFlow Functional API: tf.keras.Model(inputs=…, outputs=…)",
        languages: PY,
    },
    CapabilityInfo {
        id: "tf_model_subclassing",
        description: "Membangun model dengan Model Subclassing: class X(tf.keras.Model)",
        languages: PY,
    },
    CapabilityInfo {
        id: "custom_keras_component",
        description: "Komponen kustom Keras: custom Layer, Loss, Callback, atau Metric",
        languages: PY,
    },
    CapabilityInfo {
        id: "custom_training_loop",
        description: "Training/evaluation loop kustom dengan tf.GradientTape",
        languages: PY,
    },
    CapabilityInfo {
        id: "model_export_code",
        description: "Kode menyimpan model: model.save() atau tf.saved_model.save()",
        languages: PY,
    },
    CapabilityInfo {
        id: "model_inference_code",
        description: "Kode inference model: model.predict() atau model.evaluate()",
        languages: PY,
    },
    CapabilityInfo {
        id: "data_gathering",
        description: "Mengumpulkan data: pd.read_csv, read_excel, scraping",
        languages: PY,
    },
    CapabilityInfo {
        id: "data_cleaning",
        description: "Membersihkan data: dropna, fillna, drop_duplicates, isnull",
        languages: PY,
    },
    CapabilityInfo {
        id: "exploratory_analysis",
        description: "EDA: describe, info, value_counts, corr, groupby",
        languages: PY,
    },
    CapabilityInfo {
        id: "data_visualisation",
        description: "Visualisasi data dengan matplotlib, seaborn, atau plotly",
        languages: PY,
    },
    CapabilityInfo {
        id: "feature_engineering",
        description: "Feature engineering: scaler, encoder, PCA, vectorizer, get_dummies",
        languages: PY,
    },
    CapabilityInfo {
        id: "ab_testing",
        description: "A/B testing dengan uji statistik: t-test, chi-square, z-test, ANOVA",
        languages: PY,
    },
    CapabilityInfo {
        id: "notebook_narrative",
        description: "Notebook menyertakan penjelasan markdown, bukan hanya sel kode",
        languages: PY,
    },
    CapabilityInfo {
        id: "streamlit_dashboard",
        description: "Membangun dashboard interaktif dengan Streamlit",
        languages: MANIFEST,
    },
];

pub fn known(id: &str) -> bool {
    CATALOG.iter().any(|c| c.id == id)
}

const MANIFEST_IS_ENOUGH: &[&str] = &[
    "module_bundler",
    "streamlit_dashboard",
    "ml_in_app",
    "express_framework",
    "ml_serving_api",
];

const FILE_EVIDENCE: &[&str] = &[
    "tensorboard_integration",
    "saved_model_artifact",
    "notebook_present",
];

const MARKUP_BASED: &[&str] = &["responsive_layout", "css_framework", "semantic_html"];

pub fn needs_ast(id: &str) -> bool {
    !MANIFEST_IS_ENOUGH.contains(&id) && !FILE_EVIDENCE.contains(&id) && !MARKUP_BASED.contains(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ids_are_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|c| c.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "ada id capability yang duplikat");
    }

    #[test]
    fn manifest_only_capabilities_have_no_stdlib_equivalent() {
        for id in MANIFEST_IS_ENOUGH {
            assert!(known(id), "'{id}' tidak ada di CATALOG");
            assert!(!needs_ast(id));
        }
    }

    #[test]
    fn capabilities_reachable_via_stdlib_need_ast() {
        for id in [
            "rest_api_server",
            "database_persistence",
            "llm_api_client",
            "automl_service",
            "pretrained_model_hub",
            "http_request",
            "rest_route_definition",
        ] {
            assert!(
                needs_ast(id),
                "'{id}' bisa dipakai lewat stdlib atau HTTP mentah, jadi manifest saja \
                 tidak boleh dianggap konklusif"
            );
        }
    }
}
