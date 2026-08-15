use oxc_allocator::Allocator;
use oxc_ast::visit::walk;
use oxc_ast::{Visit, ast::*};
use oxc_parser::Parser;
use oxc_span::{SourceType, Span};
use oxc_syntax::scope::ScopeFlags;

#[derive(Debug, Clone)]
pub struct Hit {
    pub capability: &'static str,
    pub matched: String,
    pub span: Span,
}

pub struct FileHits {
    pub hits: Vec<Hit>,
    pub parse_error: Option<String>,
}

pub fn analyze(path: &str, source: &str) -> FileHits {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let ret = Parser::new(&allocator, source, source_type).parse();

    let mut collector = Collector { hits: Vec::new() };
    collector.visit_program(&ret.program);

    let parse_error = ret.errors.first().map(|e| e.to_string());

    FileHits {
        hits: collector.hits,
        parse_error,
    }
}

struct Collector {
    hits: Vec<Hit>,
}

impl Collector {
    fn push(&mut self, capability: &'static str, matched: impl Into<String>, span: Span) {
        self.hits.push(Hit {
            capability,
            matched: matched.into(),
            span,
        });
    }
}

fn callee_path(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::ThisExpression(_) => Some("this".to_string()),
        Expression::StaticMemberExpression(m) => {
            let object = callee_path(&m.object)?;
            Some(format!("{}.{}", object, m.property.name))
        }
        Expression::CallExpression(c) => callee_path(&c.callee).map(|p| format!("{}()", p)),
        Expression::ParenthesizedExpression(p) => callee_path(&p.expression),
        _ => None,
    }
}

fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn root_segment(path: &str) -> &str {
    path.split('.').next().unwrap_or(path)
}

const HTTP_ROOTS: &[&str] = &["axios", "ky", "superagent", "got"];
const HTTP_BARE: &[&str] = &["fetch", "$", "jQuery"];
const HTTP_JQUERY_METHODS: &[&str] = &["ajax", "get", "post", "getJSON"];
const ITERATION_METHODS: &[&str] = &[
    "map", "filter", "reduce", "forEach", "flatMap", "some", "every", "find",
];
const DOM_QUERY_METHODS: &[&str] = &[
    "getElementById",
    "querySelector",
    "querySelectorAll",
    "getElementsByClassName",
    "getElementsByTagName",
    "createElement",
    "createTextNode",
    "appendChild",
    "removeChild",
    "insertAdjacentHTML",
    "setAttribute",
    "getAttribute",
];
const DOM_PROPERTIES: &[&str] = &[
    "innerHTML",
    "textContent",
    "innerText",
    "outerHTML",
    "value",
    "classList",
    "style",
];
const STORAGE_ROOTS: &[&str] = &["localStorage", "sessionStorage"];
const ROUTE_ROOTS: &[&str] = &["app", "router", "server", "api", "route", "routes"];
const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "options", "head",
];

fn first_string_argument<'a>(call: &'a CallExpression<'_>) -> Option<&'a str> {
    match call.arguments.first()?.as_expression()? {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        Expression::TemplateLiteral(template) => template
            .quasis
            .first()
            .map(|quasi| quasi.value.raw.as_str()),
        _ => None,
    }
}

fn route_definition(path: &str, call: &CallExpression<'_>) -> Option<String> {
    let root = root_segment(path);
    let last = last_segment(path);

    if last == "route" && ROUTE_ROOTS.contains(&root) {
        return Some(format!("{path}()"));
    }
    if !ROUTE_ROOTS.contains(&root) || !ROUTE_METHODS.contains(&last) {
        return None;
    }
    let route = first_string_argument(call)?;
    if route.starts_with('/') {
        Some(format!("{} {}", last.to_ascii_uppercase(), route))
    } else {
        None
    }
}

fn classify_call(path: &str) -> Option<(&'static str, String)> {
    let root = root_segment(path);
    let last = last_segment(path);

    if path == "fetch"
        || path == "window.fetch"
        || path == "globalThis.fetch"
        || path == "self.fetch"
    {
        return Some(("http_request", path.to_string()));
    }
    if HTTP_ROOTS.contains(&root) {
        return Some(("http_request", path.to_string()));
    }
    if HTTP_BARE.contains(&root) && HTTP_JQUERY_METHODS.contains(&last) {
        return Some(("http_request", path.to_string()));
    }
    if matches!(last, "then" | "catch" | "finally") {
        let cap = if last == "catch" {
            "error_handling"
        } else {
            "promise_chain"
        };
        return Some((cap, format!(".{}()", last)));
    }
    if last == "addEventListener" || last == "removeEventListener" {
        return Some(("event_listener", path.to_string()));
    }
    if DOM_QUERY_METHODS.contains(&last) {
        return Some(("dom_manipulation", path.to_string()));
    }
    if STORAGE_ROOTS.contains(&root) {
        return None;
    }
    if path == "JSON.parse" || path == "JSON.stringify" {
        return Some(("json_handling", path.to_string()));
    }
    if ITERATION_METHODS.contains(&last) && path.contains('.') {
        return Some(("array_iteration", format!(".{}()", last)));
    }
    None
}

impl<'a> Visit<'a> for Collector {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(path) = callee_path(&it.callee) {
            if let Some((capability, matched)) = classify_call(&path) {
                self.push(capability, matched, it.span);
            }
            if let Some(matched) = route_definition(&path, it) {
                self.push("rest_route_definition", matched, it.span);
            }
            if last_segment(&path) == "json" {
                self.push("json_handling", path, it.span);
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if let Some(path) = callee_path(&it.callee) {
            match last_segment(&path) {
                "XMLHttpRequest" => self.push("http_request", path, it.span),
                "WebSocket" => self.push("websocket", path, it.span),
                _ => {}
            }
        }
        walk::walk_new_expression(self, it);
    }

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        self.push("async_await", "await", it.span);
        walk::walk_await_expression(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if it.r#async {
            self.push("async_await", "async function", it.span);
        }
        walk::walk_function(self, it, flags);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if it.r#async {
            self.push("async_await", "async arrow function", it.span);
        }
        walk::walk_arrow_function_expression(self, it);
    }

    fn visit_try_statement(&mut self, it: &TryStatement<'a>) {
        self.push("error_handling", "try/catch", it.span);
        walk::walk_try_statement(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.push(
            "es_module",
            format!("import '{}'", it.source.value),
            it.span,
        );
        walk::walk_import_declaration(self, it);
    }

    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        self.push("es_module", "export", it.span);
        walk::walk_export_named_declaration(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        self.push("es_module", "export default", it.span);
        walk::walk_export_default_declaration(self, it);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(id) = &it.id {
            self.push("class_usage", format!("class {}", id.name), it.span);
        } else {
            self.push("class_usage", "class", it.span);
        }
        walk::walk_class(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        let property = it.property.name.as_str();
        if DOM_PROPERTIES.contains(&property) {
            if let Some(path) = callee_path(&it.object) {
                if !STORAGE_ROOTS.contains(&root_segment(&path)) {
                    self.push(
                        "dom_manipulation",
                        format!("{}.{}", path, property),
                        it.span,
                    );
                }
            }
        }
        if let Some(path) = callee_path(&it.object) {
            if STORAGE_ROOTS.contains(&root_segment(&path)) {
                self.push("web_storage", format!("{}.{}", path, property), it.span);
            }
        }
        walk::walk_static_member_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(source: &str) -> Vec<&'static str> {
        let mut found: Vec<&'static str> = analyze("app.js", source)
            .hits
            .into_iter()
            .map(|h| h.capability)
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    #[test]
    fn detects_fetch() {
        assert!(
            caps("async function load(){ const r = await fetch('/api'); }")
                .contains(&"http_request")
        );
    }

    #[test]
    fn detects_axios_methods() {
        assert!(caps("axios.get('/api/products')").contains(&"http_request"));
        assert!(caps("import axios from 'axios'; axios({url:'/x'})").contains(&"http_request"));
    }

    #[test]
    fn detects_xhr_and_jquery() {
        assert!(caps("const x = new XMLHttpRequest();").contains(&"http_request"));
        assert!(caps("$.ajax({url:'/api'})").contains(&"http_request"));
    }

    #[test]
    fn ignores_comments_and_strings() {
        let source = r#"
            // nanti kita pakai fetch('/api') di sini
            /* axios.get('/api') juga belum */
            const petunjuk = "gunakan fetch untuk mengambil data";
            const contoh = 'new XMLHttpRequest()';
            console.log(petunjuk, contoh);
        "#;
        assert!(
            !caps(source).contains(&"http_request"),
            "komentar dan string literal tidak boleh dihitung sebagai HTTP request"
        );
    }

    #[test]
    fn detects_supporting_capabilities() {
        let found = caps(
            "import { render } from './ui.js';
             export class Store {
               async load(){
                 try {
                   const res = await fetch('/api');
                   const data = await res.json();
                   localStorage.setItem('cache', JSON.stringify(data));
                   document.querySelector('#list').innerHTML = data.map(d => d.name).join('');
                 } catch (e) { console.error(e); }
               }
             }
             document.addEventListener('click', () => {});",
        );
        for expected in [
            "http_request",
            "async_await",
            "error_handling",
            "dom_manipulation",
            "event_listener",
            "web_storage",
            "es_module",
            "class_usage",
            "array_iteration",
            "json_handling",
        ] {
            assert!(
                found.contains(&expected),
                "hilang: {expected} (dapat: {found:?})"
            );
        }
    }

    #[test]
    fn survives_syntax_error() {
        let result = analyze("broken.js", "function ( { const = ;");
        assert!(result.parse_error.is_some());
    }

    #[test]
    fn typescript_is_parsed() {
        assert!(
            analyze(
                "api.ts",
                "const load = async (): Promise<void> => { await fetch('/x'); };"
            )
            .hits
            .iter()
            .any(|h| h.capability == "http_request")
        );
    }
}

#[cfg(test)]
mod regression {
    use super::*;

    fn count(source: &str, capability: &str) -> usize {
        analyze("app.js", source)
            .hits
            .iter()
            .filter(|h| h.capability == capability)
            .count()
    }

    #[test]
    fn storage_call_counted_once() {
        assert_eq!(
            count("localStorage.setItem('k', 'v');", "web_storage"),
            1,
            "satu pemanggilan storage tidak boleh dihitung dua kali"
        );
    }

    #[test]
    fn server_route_is_not_confused_with_http_client() {
        let server = analyze(
            "server.js",
            "const app = express();
             app.get('/products', handler);
             app.post('/products', handler);
             router.delete('/products/:id', handler);",
        );
        let routes: Vec<&str> = server
            .hits
            .iter()
            .filter(|h| h.capability == "rest_route_definition")
            .map(|h| h.matched.as_str())
            .collect();
        assert_eq!(
            routes,
            vec!["GET /products", "POST /products", "DELETE /products/:id"]
        );
        assert!(
            !server.hits.iter().any(|h| h.capability == "http_request"),
            "definisi route server bukan panggilan HTTP client"
        );

        let client = analyze("api.js", "axios.get('/products');");
        assert!(client.hits.iter().any(|h| h.capability == "http_request"));
        assert!(
            !client
                .hits
                .iter()
                .any(|h| h.capability == "rest_route_definition"),
            "axios.get adalah client, bukan definisi route"
        );
    }

    #[test]
    fn classlist_counted_once() {
        assert_eq!(count("el.classList.add('aktif');", "dom_manipulation"), 1);
    }
}
