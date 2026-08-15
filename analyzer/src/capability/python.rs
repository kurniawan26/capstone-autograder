use tree_sitter::{Node, Parser};

#[derive(Debug, Clone)]
pub struct Hit {
    pub capability: &'static str,
    pub matched: String,
    pub line: usize,
    pub column: usize,
    pub snippet: String,
}

pub fn analyze(source: &str) -> Vec<Hit> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => return Vec::new(),
    };

    let mut collector = Collector {
        source,
        hits: Vec::new(),
    };
    collector.walk(tree.root_node());
    collector.hits
}

struct Collector<'a> {
    source: &'a str,
    hits: Vec<Hit>,
}

fn text_of<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn line_snippet(source: &str, node: Node<'_>) -> String {
    let start = node.start_byte();
    let line_start = source[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = source[start..]
        .find('\n')
        .map(|p| start + p)
        .unwrap_or(source.len());
    let raw = source[line_start..line_end].trim();
    if raw.chars().count() > 160 {
        let cut: String = raw.chars().take(160).collect();
        format!("{cut} …")
    } else {
        raw.to_string()
    }
}

const HTTP_CALLS: &[&str] = &[
    "requests.get",
    "requests.post",
    "requests.put",
    "requests.patch",
    "requests.delete",
    "requests.request",
    "requests.head",
    "httpx.get",
    "httpx.post",
    "httpx.put",
    "httpx.delete",
    "httpx.request",
    "urllib.request.urlopen",
    "urlopen",
    "aiohttp.request",
];

const ROUTE_DECORATORS: &[&str] = &["route", "get", "post", "put", "patch", "delete"];

const DB_CALLS: &[&str] = &[
    "psycopg2.connect",
    "sqlite3.connect",
    "pymysql.connect",
    "mysql.connector.connect",
    "create_engine",
    "MongoClient",
    "SessionLocal",
];

const LLM_CALLS: &[&str] = &[
    "openai.chat.completions.create",
    "openai.ChatCompletion.create",
    "OpenAI",
    "AsyncOpenAI",
    "genai.GenerativeModel",
    "genai.configure",
    "anthropic.Anthropic",
    "Anthropic",
    "cohere.Client",
];

const HUB_CALLS: &[&str] = &[
    "hub.KerasLayer",
    "hub.load",
    "hub.Module",
    "from_pretrained",
    "timm.create_model",
    "easyocr.Reader",
    "PaddleOCR",
    "pytesseract.image_to_string",
    "YOLO",
    "mediapipe.solutions",
    "DeepFace.analyze",
    "pipeline",
];

const AUTOML_CALLS: &[&str] = &[
    "autokeras.ImageClassifier",
    "ak.ImageClassifier",
    "ak.StructuredDataClassifier",
    "TPOTClassifier",
    "TPOTRegressor",
    "AutoSklearnClassifier",
    "compare_models",
    "AutoMlClient",
];

const SAVE_CALLS: &[&str] = &[
    "model.save",
    "tf.saved_model.save",
    "save_model",
    "model.export",
    "model.save_weights",
];

const INFERENCE_CALLS: &[&str] = &[
    "model.predict",
    "model.predict_classes",
    "predict",
    "model.evaluate",
];

const EDA_CALLS: &[&str] = &[
    "df.describe",
    "df.info",
    "df.head",
    "value_counts",
    "describe",
    "corr",
    "groupby",
];

const CLEANING_CALLS: &[&str] = &[
    "dropna",
    "fillna",
    "drop_duplicates",
    "isnull",
    "isna",
    "duplicated",
    "astype",
    "replace",
];

const VISUALISATION_CALLS: &[&str] = &[
    "plt.plot",
    "plt.bar",
    "plt.hist",
    "plt.scatter",
    "plt.show",
    "sns.barplot",
    "sns.heatmap",
    "sns.histplot",
    "sns.scatterplot",
    "px.bar",
    "px.line",
    "px.scatter",
];

const FEATURE_ENGINEERING_CALLS: &[&str] = &[
    "StandardScaler",
    "MinMaxScaler",
    "RobustScaler",
    "OneHotEncoder",
    "LabelEncoder",
    "OrdinalEncoder",
    "PolynomialFeatures",
    "TfidfVectorizer",
    "CountVectorizer",
    "get_dummies",
    "PCA",
    "SelectKBest",
];

const AB_TEST_CALLS: &[&str] = &[
    "ttest_ind",
    "ttest_rel",
    "chi2_contingency",
    "mannwhitneyu",
    "proportions_ztest",
    "ztest",
    "f_oneway",
];

const STREAMLIT_CALLS: &[&str] = &[
    "st.title",
    "st.write",
    "st.dataframe",
    "st.plotly_chart",
    "st.pyplot",
    "st.sidebar",
    "st.metric",
    "st.selectbox",
];

const TENSORBOARD_CALLS: &[&str] = &[
    "tf.keras.callbacks.TensorBoard",
    "keras.callbacks.TensorBoard",
    "TensorBoard",
    "SummaryWriter",
    "tf.summary.create_file_writer",
];

fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn matches_any(path: &str, table: &[&str]) -> bool {
    table
        .iter()
        .any(|candidate| *candidate == path || *candidate == last_segment(path))
}

fn classify_call(path: &str) -> Option<&'static str> {
    if matches_any(path, HTTP_CALLS) {
        return Some("http_request");
    }
    if matches_any(path, LLM_CALLS) {
        return Some("llm_api_client");
    }
    if matches_any(path, HUB_CALLS) {
        return Some("pretrained_model_hub");
    }
    if matches_any(path, AUTOML_CALLS) {
        return Some("automl_service");
    }
    if matches_any(path, TENSORBOARD_CALLS) {
        return Some("tensorboard_integration");
    }
    if matches_any(path, DB_CALLS) {
        return Some("database_persistence");
    }
    if matches_any(path, SAVE_CALLS) {
        return Some("model_export_code");
    }
    if matches_any(path, INFERENCE_CALLS) {
        return Some("model_inference_code");
    }
    if matches_any(path, AB_TEST_CALLS) {
        return Some("ab_testing");
    }
    if matches_any(path, FEATURE_ENGINEERING_CALLS) {
        return Some("feature_engineering");
    }
    if matches_any(path, VISUALISATION_CALLS) {
        return Some("data_visualisation");
    }
    if matches_any(path, CLEANING_CALLS) {
        return Some("data_cleaning");
    }
    if matches_any(path, EDA_CALLS) {
        return Some("exploratory_analysis");
    }
    if matches_any(path, STREAMLIT_CALLS) {
        return Some("streamlit_dashboard");
    }
    if path == "tf.GradientTape" || path == "GradientTape" {
        return Some("custom_training_loop");
    }
    if path == "pd.read_csv" || path == "read_csv" || path == "pd.read_excel" {
        return Some("data_gathering");
    }
    None
}

const BASE_CAPABILITIES: &[(&str, &str)] = &[
    ("tf.keras.layers.Layer", "custom_keras_component"),
    ("keras.layers.Layer", "custom_keras_component"),
    ("layers.Layer", "custom_keras_component"),
    ("tf.keras.losses.Loss", "custom_keras_component"),
    ("keras.losses.Loss", "custom_keras_component"),
    ("losses.Loss", "custom_keras_component"),
    ("tf.keras.callbacks.Callback", "custom_keras_component"),
    ("keras.callbacks.Callback", "custom_keras_component"),
    ("callbacks.Callback", "custom_keras_component"),
    ("tf.keras.metrics.Metric", "custom_keras_component"),
    ("tf.keras.Model", "tf_model_subclassing"),
    ("keras.Model", "tf_model_subclassing"),
    ("Model", "tf_model_subclassing"),
];

impl<'a> Collector<'a> {
    fn push(&mut self, capability: &'static str, matched: String, node: Node<'_>) {
        let position = node.start_position();
        self.hits.push(Hit {
            capability,
            matched,
            line: position.row + 1,
            column: position.column + 1,
            snippet: line_snippet(self.source, node),
        });
    }

    fn dotted_name(&self, node: Node<'_>) -> Option<String> {
        match node.kind() {
            "identifier" => Some(text_of(node, self.source).to_string()),
            "attribute" => {
                let object = node.child_by_field_name("object")?;
                let attribute = node.child_by_field_name("attribute")?;
                let base = self.dotted_name(object)?;
                Some(format!("{}.{}", base, text_of(attribute, self.source)))
            }
            "call" => {
                let function = node.child_by_field_name("function")?;
                self.dotted_name(function)
            }
            _ => None,
        }
    }

    fn handle_call(&mut self, node: Node<'_>) {
        let function = match node.child_by_field_name("function") {
            Some(function) => function,
            None => return,
        };
        let path = match self.dotted_name(function) {
            Some(path) => path,
            None => return,
        };

        if let Some(capability) = classify_call(&path) {
            self.push(capability, path.clone(), node);
        }

        if last_segment(&path) == "Model" {
            let arguments = node
                .child_by_field_name("arguments")
                .map(|args| text_of(args, self.source).to_string())
                .unwrap_or_default();
            if arguments.contains("inputs") && arguments.contains("outputs") {
                self.push(
                    "tf_functional_api",
                    format!("{path}(inputs=…, outputs=…)"),
                    node,
                );
            }
        }
    }

    fn handle_class(&mut self, node: Node<'_>) {
        let name = node
            .child_by_field_name("name")
            .map(|n| text_of(n, self.source).to_string())
            .unwrap_or_default();

        let bases = match node.child_by_field_name("superclasses") {
            Some(bases) => bases,
            None => return,
        };

        let mut cursor = bases.walk();
        let children: Vec<Node> = bases.named_children(&mut cursor).collect();
        for base in children {
            if let Some(path) = self.dotted_name(base) {
                for (candidate, capability) in BASE_CAPABILITIES {
                    if *candidate == path || last_segment(candidate) == last_segment(&path) {
                        self.push(capability, format!("class {name}({path})"), node);
                        break;
                    }
                }
            }
        }
    }

    fn handle_decorator(&mut self, node: Node<'_>) {
        let text = text_of(node, self.source);
        let inner = text.trim_start_matches('@').trim();
        let path = inner.split('(').next().unwrap_or(inner);
        let last = last_segment(path);

        if ROUTE_DECORATORS.contains(&last) && path.contains('.') {
            let route = text
                .find('(')
                .and_then(|start| {
                    let rest = &text[start + 1..];
                    rest.find(')').map(|end| rest[..end].to_string())
                })
                .unwrap_or_default();
            let route = route.trim().trim_matches(['"', '\'']).to_string();
            if route.starts_with('/') {
                self.push(
                    "rest_route_definition",
                    format!("{} {}", last.to_ascii_uppercase(), route),
                    node,
                );
            }
        }
    }

    fn handle_import(&mut self, node: Node<'_>) {
        let text = text_of(node, self.source);
        let capability = if text.contains("tensorflow_hub") {
            Some("pretrained_model_hub")
        } else if text.contains("openai")
            || text.contains("google.generativeai")
            || text.contains("anthropic")
        {
            Some("llm_api_client")
        } else if text.contains("easyocr")
            || text.contains("paddleocr")
            || text.contains("pytesseract")
            || text.contains("ultralytics")
            || text.contains("mediapipe")
        {
            Some("pretrained_model_hub")
        } else if text.contains("autokeras") || text.contains("tpot") || text.contains("pycaret") {
            Some("automl_service")
        } else if text.contains("streamlit") {
            Some("streamlit_dashboard")
        } else if text.contains("tensorflow") || text.contains("keras") || text.contains("torch") {
            Some("ml_in_app")
        } else {
            None
        };

        if let Some(capability) = capability {
            self.push(capability, text.trim().to_string(), node);
        }
    }

    fn walk(&mut self, node: Node<'_>) {
        match node.kind() {
            "call" => self.handle_call(node),
            "class_definition" => self.handle_class(node),
            "decorator" => self.handle_decorator(node),
            "import_statement" | "import_from_statement" => self.handle_import(node),
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(source: &str) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> =
            analyze(source).into_iter().map(|h| h.capability).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    #[test]
    fn detects_functional_api() {
        let found = caps(
            "import tensorflow as tf\n\
             inputs = tf.keras.Input(shape=(32,))\n\
             x = tf.keras.layers.Dense(64)(inputs)\n\
             model = tf.keras.Model(inputs=inputs, outputs=x)\n",
        );
        assert!(found.contains(&"tf_functional_api"), "dapat: {found:?}");
    }

    #[test]
    fn detects_model_subclassing() {
        let found = caps(
            "import tensorflow as tf\n\
             class MyModel(tf.keras.Model):\n    def call(self, x):\n        return x\n",
        );
        assert!(found.contains(&"tf_model_subclassing"), "dapat: {found:?}");
    }

    #[test]
    fn detects_custom_keras_components() {
        for (source, label) in [
            ("class Attn(tf.keras.layers.Layer):\n    pass\n", "layer"),
            ("class Focal(tf.keras.losses.Loss):\n    pass\n", "loss"),
            (
                "class Early(tf.keras.callbacks.Callback):\n    pass\n",
                "callback",
            ),
        ] {
            assert!(
                caps(source).contains(&"custom_keras_component"),
                "gagal mendeteksi custom {label}"
            );
        }
    }

    #[test]
    fn detects_gradient_tape() {
        let found = caps("with tf.GradientTape() as tape:\n    loss = loss_fn(y, model(x))\n");
        assert!(found.contains(&"custom_training_loop"), "dapat: {found:?}");
    }

    #[test]
    fn detects_forbidden_hub_and_llm_api() {
        assert!(caps("import tensorflow_hub as hub\n").contains(&"pretrained_model_hub"));
        assert!(
            caps("layer = hub.KerasLayer('https://tfhub.dev/x')\n")
                .contains(&"pretrained_model_hub")
        );
        assert!(caps("from openai import OpenAI\n").contains(&"llm_api_client"));
        assert!(caps("import google.generativeai as genai\n").contains(&"llm_api_client"));
    }

    #[test]
    fn detects_automl() {
        assert!(caps("import autokeras as ak\n").contains(&"automl_service"));
        assert!(caps("from tpot import TPOTClassifier\n").contains(&"automl_service"));
    }

    #[test]
    fn ignores_comments_and_strings() {
        let source = "# nanti pakai tf.GradientTape() untuk custom loop\n\
                      catatan = 'import tensorflow_hub as hub'\n\
                      petunjuk = \"openai.ChatCompletion.create()\"\n\
                      print(catatan, petunjuk)\n";
        let found = caps(source);
        assert!(
            !found.contains(&"custom_training_loop"),
            "komentar tidak boleh dihitung"
        );
        assert!(
            !found.contains(&"pretrained_model_hub"),
            "string tidak boleh dihitung"
        );
        assert!(
            !found.contains(&"llm_api_client"),
            "string tidak boleh dihitung"
        );
    }

    #[test]
    fn detects_fastapi_and_flask_routes() {
        let fastapi = caps(
            "from fastapi import FastAPI\n\
             app = FastAPI()\n\
             @app.get('/api/predictions')\n\
             def predict():\n    return {}\n",
        );
        assert!(
            fastapi.contains(&"rest_route_definition"),
            "dapat: {fastapi:?}"
        );

        let flask = caps(
            "@app.route('/api/products')\n\
             def products():\n    return []\n",
        );
        assert!(flask.contains(&"rest_route_definition"), "dapat: {flask:?}");
    }

    #[test]
    fn detects_data_science_pipeline() {
        let found = caps(
            "import pandas as pd\n\
             from sklearn.preprocessing import StandardScaler\n\
             from scipy.stats import ttest_ind\n\
             import matplotlib.pyplot as plt\n\
             df = pd.read_csv('data.csv')\n\
             df = df.dropna()\n\
             df.describe()\n\
             scaler = StandardScaler()\n\
             plt.hist(df['harga'])\n\
             ttest_ind(a, b)\n",
        );
        for expected in [
            "data_gathering",
            "data_cleaning",
            "exploratory_analysis",
            "feature_engineering",
            "data_visualisation",
            "ab_testing",
        ] {
            assert!(
                found.contains(&expected),
                "hilang: {expected} (dapat: {found:?})"
            );
        }
    }

    #[test]
    fn detects_model_lifecycle() {
        let found = caps(
            "model.save('model.keras')\n\
             preds = model.predict(x_test)\n\
             import requests\n\
             requests.get('https://api.example.com')\n",
        );
        assert!(found.contains(&"model_export_code"));
        assert!(found.contains(&"model_inference_code"));
        assert!(found.contains(&"http_request"));
    }

    #[test]
    fn survives_broken_python() {
        let hits = analyze("def broken(:\n    x = = 1\n");
        assert!(hits.is_empty() || !hits.is_empty());
    }

    #[test]
    fn reports_accurate_line_numbers() {
        let hits = analyze("import os\n\nimport tensorflow_hub as hub\n");
        let hit = hits
            .iter()
            .find(|h| h.capability == "pretrained_model_hub")
            .expect("harus terdeteksi");
        assert_eq!(hit.line, 3);
    }
}

#[cfg(test)]
mod precision {
    use super::*;

    fn caps(source: &str) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> =
            analyze(source).into_iter().map(|h| h.capability).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    #[test]
    fn plain_setup_function_is_not_automl() {
        let found = caps("def setup():\n    pass\n\nsetup()\n");
        assert!(
            !found.contains(&"automl_service"),
            "fungsi setup() biasa tidak boleh dianggap pelanggaran AutoML: {found:?}"
        );
    }

    #[test]
    fn functional_api_counted_once() {
        let hits = analyze("model = tf.keras.Model(inputs=inputs, outputs=outputs)\n");
        let count = hits
            .iter()
            .filter(|h| h.capability == "tf_functional_api")
            .count();
        assert_eq!(count, 1, "satu pemanggilan tidak boleh dihitung dua kali");
    }

    #[test]
    fn subclassing_is_not_functional_api() {
        let found = caps("class MyModel(tf.keras.Model):\n    pass\n");
        assert!(found.contains(&"tf_model_subclassing"));
        assert!(
            !found.contains(&"tf_functional_api"),
            "subclassing bukan Functional API: {found:?}"
        );
    }

    #[test]
    fn model_call_without_inputs_is_not_functional_api() {
        let found = caps("m = SomeModel(config)\n");
        assert!(!found.contains(&"tf_functional_api"), "dapat: {found:?}");
    }
}

#[cfg(test)]
mod pretrained {
    use super::*;

    fn caps(source: &str) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> =
            analyze(source).into_iter().map(|h| h.capability).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    #[test]
    fn ocr_and_detection_models_count_as_pretrained() {
        for source in [
            "import easyocr\nreader = easyocr.Reader(['id','en'])\n",
            "from paddleocr import PaddleOCR\nocr = PaddleOCR()\n",
            "import pytesseract\ntext = pytesseract.image_to_string(img)\n",
            "from ultralytics import YOLO\nm = YOLO('yolov8n.pt')\n",
            "import mediapipe as mp\n",
        ] {
            assert!(
                caps(source).contains(&"pretrained_model_hub"),
                "model siap pakai harus terdeteksi: {source}"
            );
        }
    }

    #[test]
    fn own_model_loading_is_not_pretrained_hub() {
        let found = caps("model = tf.keras.models.load_model('glucognito_model_v2.keras')\n");
        assert!(
            !found.contains(&"pretrained_model_hub"),
            "memuat model sendiri bukan model siap pakai pihak ketiga: {found:?}"
        );
    }
}
