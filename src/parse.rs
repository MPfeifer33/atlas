use std::sync::LazyLock;
use regex::Regex;
use crate::graph::Language;

/// Extract import/use statements from source code.
pub fn extract_imports(content: &str, language: Language) -> Vec<String> {
    match language {
        Language::Rust => extract_rust_imports(content),
        Language::TypeScript | Language::JavaScript => extract_js_imports(content),
        Language::Python => extract_python_imports(content),
        Language::Go => extract_go_imports(content),
        Language::Unknown => Vec::new(),
    }
}

/// Extract exported symbols (function/struct/class names).
pub fn extract_exports(content: &str, language: Language) -> Vec<String> {
    match language {
        Language::Rust => extract_rust_exports(content),
        Language::TypeScript | Language::JavaScript => extract_js_exports(content),
        Language::Python => extract_python_exports(content),
        Language::Go => extract_go_exports(content),
        Language::Unknown => Vec::new(),
    }
}

// --- Rust ---

static RUST_USE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(?:\s*)use\s+([\w:]+(?:::\{[^}]+\})?)").unwrap());
static RUST_MOD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(?:\s*)(?:pub\s+)?mod\s+(\w+)\s*;").unwrap());
static RUST_EXPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^pub\s+(?:async\s+)?(?:fn|struct|enum|trait|type|const)\s+(\w+)").unwrap());

fn extract_rust_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for cap in RUST_USE_RE.captures_iter(content) {
        let path = cap[1].to_string();
        // Skip std/external crates, keep crate:: and relative
        if path.starts_with("crate::") || path.starts_with("super::") {
            imports.push(path);
        }
    }

    for cap in RUST_MOD_RE.captures_iter(content) {
        imports.push(format!("crate::{}", &cap[1]));
    }

    imports
}

fn extract_rust_exports(content: &str) -> Vec<String> {
    RUST_EXPORT_RE.captures_iter(content)
        .map(|cap| cap[1].to_string())
        .collect()
}

// --- TypeScript / JavaScript ---

static JS_IMPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?m)import\s+.*?\s+from\s+['"]([^'"]+)['"]"#).unwrap());
static JS_REQUIRE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?m)require\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap());
static JS_EXPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)export\s+(?:default\s+)?(?:async\s+)?(?:function|class|const|let|var|interface|type|enum)\s+(\w+)").unwrap());

fn extract_js_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for cap in JS_IMPORT_RE.captures_iter(content) {
        imports.push(cap[1].to_string());
    }
    for cap in JS_REQUIRE_RE.captures_iter(content) {
        imports.push(cap[1].to_string());
    }

    imports
}

fn extract_js_exports(content: &str) -> Vec<String> {
    JS_EXPORT_RE.captures_iter(content)
        .map(|cap| cap[1].to_string())
        .collect()
}

// --- Python ---

static PY_IMPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^import\s+([\w.]+)").unwrap());
static PY_FROM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^from\s+([\w.]+)\s+import").unwrap());
static PY_EXPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(?:async\s+)?(?:def|class)\s+(\w+)").unwrap());

fn extract_python_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for cap in PY_IMPORT_RE.captures_iter(content) {
        imports.push(cap[1].to_string());
    }
    for cap in PY_FROM_RE.captures_iter(content) {
        imports.push(cap[1].to_string());
    }

    imports
}

fn extract_python_exports(content: &str) -> Vec<String> {
    PY_EXPORT_RE.captures_iter(content)
        .map(|cap| cap[1].to_string())
        .filter(|name| !name.starts_with('_'))
        .collect()
}

// --- Go ---

static GO_SINGLE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?m)^import\s+"([^"]+)""#).unwrap());
static GO_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?ms)import\s*\((.*?)\)"#).unwrap());
static GO_PATH_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());
static GO_EXPORT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^func\s+(?:\([^)]+\)\s+)?([A-Z]\w*)").unwrap());

fn extract_go_imports(content: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for cap in GO_SINGLE_RE.captures_iter(content) {
        imports.push(cap[1].to_string());
    }

    for cap in GO_BLOCK_RE.captures_iter(content) {
        let block = &cap[1];
        for path_cap in GO_PATH_RE.captures_iter(block) {
            imports.push(path_cap[1].to_string());
        }
    }

    imports
}

fn extract_go_exports(content: &str) -> Vec<String> {
    GO_EXPORT_RE.captures_iter(content)
        .map(|cap| cap[1].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_use_crate() {
        let code = "use crate::graph::CodeGraph;\nuse crate::parse;\nuse std::path::Path;\n";
        let imports = extract_rust_imports(code);
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"crate::graph::CodeGraph".to_string()));
        assert!(imports.contains(&"crate::parse".to_string()));
    }

    #[test]
    fn rust_mod_declarations() {
        let code = "mod cli;\npub mod graph;\nmod parse;\n";
        let imports = extract_rust_imports(code);
        assert_eq!(imports.len(), 3);
        assert!(imports.contains(&"crate::cli".to_string()));
    }

    #[test]
    fn rust_pub_exports() {
        let code = "pub fn scan() {}\npub struct Graph {}\nfn private() {}\npub async fn run() {}\n";
        let exports = extract_rust_exports(code);
        assert_eq!(exports.len(), 3);
        assert!(exports.contains(&"scan".to_string()));
        assert!(exports.contains(&"Graph".to_string()));
        assert!(exports.contains(&"run".to_string()));
    }

    #[test]
    fn js_imports() {
        let code = r#"import { foo } from './bar';
import baz from "../qux";
const x = require('./lib');
"#;
        let imports = extract_js_imports(code);
        assert_eq!(imports.len(), 3);
        assert!(imports.contains(&"./bar".to_string()));
        assert!(imports.contains(&"../qux".to_string()));
        assert!(imports.contains(&"./lib".to_string()));
    }

    #[test]
    fn python_imports() {
        let code = "import os\nfrom pathlib import Path\nimport mymodule.sub\n";
        let imports = extract_python_imports(code);
        assert_eq!(imports.len(), 3);
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"pathlib".to_string()));
        assert!(imports.contains(&"mymodule.sub".to_string()));
    }

    #[test]
    fn go_imports() {
        let code = r#"
import "fmt"

import (
    "os"
    "github.com/foo/bar"
)
"#;
        let imports = extract_go_imports(code);
        assert_eq!(imports.len(), 3);
        assert!(imports.contains(&"fmt".to_string()));
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"github.com/foo/bar".to_string()));
    }
}
