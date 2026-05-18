use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config = Config::parse(env::args_os().skip(1))?;

    if config.help {
        print_help();
        return Ok(());
    }

    if config.version {
        println!("cargo-typegraph {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let json_path = match &config.json {
        Some(path) => path.clone(),
        None => build_rustdoc_json(&config)?,
    };

    let input = fs::read_to_string(&json_path)?;
    let json: Value = serde_json::from_str(&input)?;
    let graph = TypeGraph::from_rustdoc_json(&json, config.include_external)?;
    let dot = graph.to_dot();

    match &config.output {
        Some(path) => fs::write(path, dot)?,
        None => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(dot.as_bytes())?;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct Config {
    json: Option<PathBuf>,
    output: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
    package: Option<String>,
    toolchain: Option<String>,
    target: Option<String>,
    bin: Option<String>,
    features: Option<String>,
    all_features: bool,
    no_default_features: bool,
    document_private_items: bool,
    include_external: bool,
    cargo_args: Vec<OsString>,
    help: bool,
    version: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            json: None,
            output: None,
            manifest_path: None,
            package: None,
            toolchain: Some("nightly".to_string()),
            target: None,
            bin: None,
            features: None,
            all_features: false,
            no_default_features: false,
            document_private_items: true,
            include_external: false,
            cargo_args: Vec::new(),
            help: false,
            version: false,
        }
    }
}

impl Config {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Self> {
        let mut config = Config::default();
        let mut args: Vec<OsString> = args.collect();

        if args.first().and_then(|arg| arg.to_str()) == Some("typegraph") {
            args.remove(0);
        }

        let mut index = 0;
        while index < args.len() {
            let arg = args[index].to_string_lossy();
            if arg == "--" {
                config.cargo_args.extend(args[index + 1..].iter().cloned());
                break;
            }

            if let Some((name, value)) = split_arg(&arg) {
                config.set_option(name, Some(OsString::from(value)))?;
                index += 1;
                continue;
            }

            match arg.as_ref() {
                "-h" | "--help" => config.help = true,
                "-V" | "--version" => config.version = true,
                "--private" | "--document-private-items" => config.document_private_items = true,
                "--no-private" | "--no-document-private-items" => {
                    config.document_private_items = false
                }
                "--include-external" => config.include_external = true,
                "--all-features" => config.all_features = true,
                "--no-default-features" => config.no_default_features = true,
                "-o" | "--output" | "--json" | "--manifest-path" | "-p" | "--package"
                | "--toolchain" | "--target" | "--bin" | "--features" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .cloned()
                        .ok_or_else(|| format!("missing value for {arg}"))?;
                    config.set_option(&arg, Some(value))?;
                }
                _ => return Err(format!("unknown argument: {arg}").into()),
            }

            index += 1;
        }

        Ok(config)
    }

    fn set_option(&mut self, name: &str, value: Option<OsString>) -> Result<()> {
        let value = value.ok_or_else(|| format!("missing value for {name}"))?;
        match name {
            "-o" | "--output" => self.output = Some(PathBuf::from(value)),
            "--json" => self.json = Some(PathBuf::from(value)),
            "--manifest-path" => self.manifest_path = Some(PathBuf::from(value)),
            "-p" | "--package" => self.package = Some(os_to_string(value, name)?),
            "--toolchain" => self.toolchain = Some(os_to_string(value, name)?),
            "--target" => self.target = Some(os_to_string(value, name)?),
            "--bin" => self.bin = Some(os_to_string(value, name)?),
            "--features" => self.features = Some(os_to_string(value, name)?),
            _ => return Err(format!("unknown option: {name}").into()),
        }
        Ok(())
    }
}

fn os_to_string(value: OsString, name: &str) -> Result<String> {
    value
        .into_string()
        .map_err(|_| format!("{name} must be valid UTF-8").into())
}

fn split_arg(arg: &str) -> Option<(&str, &str)> {
    let (name, value) = arg.split_once('=')?;
    if name.starts_with("--") {
        Some((name, value))
    } else {
        None
    }
}

fn print_help() {
    println!(
        "\
cargo-typegraph {}

Generate a Graphviz DOT type dependency graph from rustdoc JSON.

USAGE:
    cargo typegraph [OPTIONS]
    cargo-typegraph [OPTIONS]

OPTIONS:
        --json <PATH>                 Read an existing rustdoc JSON file
    -o, --output <PATH>               Write DOT to a file instead of stdout
        --include-external            Include referenced external types as graph nodes
        --no-private                  Do not pass --document-private-items to cargo doc
        --manifest-path <PATH>        Cargo manifest path for cargo doc
    -p, --package <PACKAGE>           Package to document
        --bin <NAME>                  Generate docs for a binary target
        --target <TRIPLE>             Cargo target triple
        --features <FEATURES>         Space or comma separated Cargo features
        --all-features                Activate all Cargo features
        --no-default-features         Disable default Cargo features
        --toolchain <NAME>            Use cargo +<NAME> doc (default: nightly)
    -h, --help                        Print help
    -V, --version                     Print version

DOT is written to stdout by default. Use -o/--output to write it to a file.

Private items are included in generated rustdoc JSON by default. Use --no-private to exclude them.

Any arguments after -- are passed through to cargo doc.

NOTE:
    Building rustdoc JSON currently requires nightly rustdoc:
    cargo typegraph
",
        env!("CARGO_PKG_VERSION")
    );
}

fn build_rustdoc_json(config: &Config) -> Result<PathBuf> {
    let target_dir = PathBuf::from("target/typegraph-rustdoc");
    fs::create_dir_all(&target_dir)?;

    let mut command = Command::new("cargo");
    if let Some(toolchain) = &config.toolchain {
        command.arg(format!("+{toolchain}"));
    }
    command.arg("doc");
    command.env("RUSTDOCFLAGS", rustdoc_json_flags());

    if let Some(manifest_path) = &config.manifest_path {
        command.arg("--manifest-path").arg(manifest_path);
    }
    if let Some(package) = &config.package {
        command.arg("-p").arg(package);
    }
    if let Some(target) = &config.target {
        command.arg("--target").arg(target);
    }
    if let Some(bin) = &config.bin {
        command.arg("--bin").arg(bin);
    }
    if let Some(features) = &config.features {
        command.arg("--features").arg(features);
    }
    if config.all_features {
        command.arg("--all-features");
    }
    if config.no_default_features {
        command.arg("--no-default-features");
    }
    command.arg("--no-deps");
    command.arg("--target-dir").arg(&target_dir);
    if config.document_private_items {
        command.arg("--document-private-items");
    }
    command.args(&config.cargo_args);

    let status = command.status()?;
    if !status.success() {
        return Err(format!(
            "cargo doc failed with status {status}; rustdoc JSON requires a nightly toolchain, so try --toolchain nightly"
        )
        .into());
    }

    best_rustdoc_json_file(&target_dir)?.ok_or_else(|| {
        format!(
            "cargo doc succeeded but no rustdoc JSON file was found under {}",
            target_dir.display()
        )
        .into()
    })
}

fn rustdoc_json_flags() -> OsString {
    let mut flags = env::var_os("RUSTDOCFLAGS").unwrap_or_default();
    if !flags.as_os_str().is_empty() {
        flags.push(" ");
    }
    flags.push("-Z unstable-options --output-format json");
    flags
}

fn best_rustdoc_json_file(root: &Path) -> Result<Option<PathBuf>> {
    let mut best: Option<(usize, std::time::SystemTime, PathBuf)> = None;
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let Some(score) = rustdoc_json_score(&path)? else {
                continue;
            };
            let modified = entry.metadata()?.modified()?;
            match &best {
                Some((current_score, current_modified, _))
                    if (*current_score, *current_modified) >= (score, modified) => {}
                _ => best = Some((score, modified, path)),
            }
        }
    }

    Ok(best.map(|(_, _, path)| path))
}

fn rustdoc_json_score(path: &Path) -> Result<Option<usize>> {
    let input = fs::read_to_string(path)?;
    let Ok(json) = serde_json::from_str::<Value>(&input) else {
        return Ok(None);
    };

    let Some(index) = json.get("index").and_then(Value::as_object) else {
        return Ok(None);
    };

    let local_crate_id = json
        .get("root")
        .and_then(id_value_to_string)
        .and_then(|root| index.get(&root))
        .and_then(item_crate_id);

    let score = index
        .values()
        .filter(|item| {
            local_crate_id
                .as_deref()
                .is_none_or(|crate_id| item_crate_id(item).as_deref() == Some(crate_id))
        })
        .filter(|item| item.get("name").and_then(Value::as_str).is_some())
        .filter_map(item_kind)
        .filter(|kind| is_type_definition_kind(kind))
        .count();

    Ok(Some(score))
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    label: String,
    kind: String,
    visibility: Visibility,
    external: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Visibility {
    Public,
    Crate,
    Super,
    Restricted,
    Private,
}

impl Visibility {
    fn label(self) -> &'static str {
        match self {
            Self::Public => "pub",
            Self::Crate => "pub(crate)",
            Self::Super => "pub(super)",
            Self::Restricted => "pub(in ...)",
            Self::Private => "private",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Public => "#2563eb",
            Self::Crate => "#16a34a",
            Self::Super => "#d97706",
            Self::Restricted => "#7c3aed",
            Self::Private => "#dc2626",
        }
    }

    fn fillcolor(self) -> &'static str {
        match self {
            Self::Public => "#dbeafe",
            Self::Crate => "#dcfce7",
            Self::Super => "#fef3c7",
            Self::Restricted => "#ede9fe",
            Self::Private => "#fee2e2",
        }
    }
}

#[derive(Debug, Default)]
struct TypeGraph {
    nodes: BTreeMap<String, Node>,
    edges: BTreeSet<(String, String)>,
}

impl TypeGraph {
    fn from_rustdoc_json(json: &Value, include_external: bool) -> Result<Self> {
        let document = RustdocDocument::new(json)?;
        let local_crate_id = document.local_crate_id();
        let mut graph = TypeGraph::default();

        for (id, item) in document.index {
            if local_crate_id
                .as_deref()
                .is_some_and(|crate_id| item_crate_id(item).as_deref() != Some(crate_id))
            {
                continue;
            }

            let Some(kind) = item_kind(item) else {
                continue;
            };
            if !is_type_definition_kind(kind) {
                continue;
            }
            if item.get("name").and_then(Value::as_str).is_none() {
                continue;
            }

            graph.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    label: document.path_label(id).unwrap_or_else(|| id.clone()),
                    kind: kind.to_string(),
                    visibility: item_visibility(item, id, &document),
                    external: false,
                },
            );
        }

        let local_type_ids: BTreeSet<String> = graph.nodes.keys().cloned().collect();
        for source_id in &local_type_ids {
            let Some(item) = document.index.get(source_id) else {
                continue;
            };

            let mut deps = BTreeSet::new();
            let mut seen_items = BTreeSet::new();
            if let Some(inner) = item.get("inner") {
                collect_type_refs(
                    inner,
                    source_id,
                    &document,
                    &local_type_ids,
                    include_external,
                    &mut deps,
                    &mut seen_items,
                );
            }

            for target_id in deps {
                if target_id == *source_id {
                    continue;
                }

                if !graph.nodes.contains_key(&target_id) {
                    if include_external {
                        if let Some(node) = document.external_node(&target_id) {
                            graph.nodes.insert(target_id.clone(), node);
                        }
                    }
                }

                if graph.nodes.contains_key(&target_id) {
                    graph.edges.insert((source_id.clone(), target_id));
                }
            }
        }

        Ok(graph)
    }

    fn to_dot(&self) -> String {
        let mut dot = String::from("digraph typegraph {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  graph [fontname=\"monospace\"];\n");
        dot.push_str("  node [shape=plain, fontname=\"monospace\"];\n");
        dot.push_str("  edge [fontname=\"monospace\"];\n\n");

        for node in self.nodes.values() {
            let declaration = if node.external {
                format!(
                    "{} {}<BR/>external",
                    escape_html_label(node.visibility.label()),
                    escape_html_label(&node.kind)
                )
            } else {
                format!(
                    "{} {}",
                    escape_html_label(node.visibility.label()),
                    escape_html_label(&node.kind)
                )
            };
            let color = node.visibility.color();
            let fillcolor = node.visibility.fillcolor();
            dot.push_str(&format!(
                "  \"{}\" [label=<<TABLE BORDER=\"1\" CELLBORDER=\"1\" CELLSPACING=\"0\" CELLPADDING=\"6\" COLOR=\"{}\" BGCOLOR=\"{}\"><TR><TD><FONT FACE=\"monospace\">{}</FONT></TD></TR><TR><TD><FONT FACE=\"monospace\">{}</FONT></TD></TR></TABLE>>];\n",
                escape_dot(&node.id),
                color,
                fillcolor,
                declaration,
                escape_html_label(&node.label)
            ));
        }

        if !self.edges.is_empty() {
            dot.push('\n');
        }

        for (source, target) in &self.edges {
            dot.push_str(&format!(
                "  \"{}\" -> \"{}\";\n",
                escape_dot(source),
                escape_dot(target)
            ));
        }

        dot.push_str("}\n");
        dot
    }
}

#[derive(Debug)]
struct RustdocDocument<'a> {
    index: &'a Map<String, Value>,
    paths: Option<&'a Map<String, Value>>,
    root: Option<String>,
}

impl<'a> RustdocDocument<'a> {
    fn new(json: &'a Value) -> Result<Self> {
        let index = json
            .get("index")
            .and_then(Value::as_object)
            .ok_or("rustdoc JSON is missing an object-valued index")?;
        let paths = json.get("paths").and_then(Value::as_object);
        let root = json.get("root").and_then(id_value_to_string);

        Ok(Self { index, paths, root })
    }

    fn local_crate_id(&self) -> Option<String> {
        self.root
            .as_deref()
            .and_then(|root| self.index.get(root))
            .and_then(item_crate_id)
    }

    fn path_label(&self, id: &str) -> Option<String> {
        self.paths
            .and_then(|paths| paths.get(id))
            .and_then(|path| path.get("path"))
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .filter(|path| !path.is_empty())
            .or_else(|| {
                self.index
                    .get(id)
                    .and_then(|item| item.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }

    fn external_node(&self, id: &str) -> Option<Node> {
        let path = self.paths.and_then(|paths| paths.get(id))?;
        let kind = path.get("kind").and_then(Value::as_str)?;
        if !is_type_definition_kind(kind) {
            return None;
        }

        Some(Node {
            id: id.to_string(),
            label: self.path_label(id).unwrap_or_else(|| id.to_string()),
            kind: kind.to_string(),
            visibility: Visibility::Public,
            external: true,
        })
    }

    fn containing_module_path(&self, id: &str) -> Option<String> {
        let mut parts = self
            .paths
            .and_then(|paths| paths.get(id))
            .and_then(|path| path.get("path"))
            .and_then(Value::as_array)?
            .iter()
            .filter_map(Value::as_str);

        parts.next()?;
        let mut module_parts: Vec<&str> = parts.collect();
        module_parts.pop()?;

        if module_parts.is_empty() {
            Some("::".to_string())
        } else {
            Some(format!("::{}", module_parts.join("::")))
        }
    }
}

fn collect_type_refs(
    value: &Value,
    source_id: &str,
    document: &RustdocDocument<'_>,
    local_type_ids: &BTreeSet<String>,
    include_external: bool,
    deps: &mut BTreeSet<String>,
    seen_items: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            if let Some(id) = object.get("id").and_then(id_value_to_string) {
                consider_reference(
                    &id,
                    source_id,
                    document,
                    local_type_ids,
                    include_external,
                    deps,
                    seen_items,
                );
            }

            for (key, nested) in object {
                if is_rustdoc_backreference_key(key) {
                    continue;
                }

                collect_type_refs(
                    nested,
                    source_id,
                    document,
                    local_type_ids,
                    include_external,
                    deps,
                    seen_items,
                );
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect_type_refs(
                    nested,
                    source_id,
                    document,
                    local_type_ids,
                    include_external,
                    deps,
                    seen_items,
                );
            }
        }
        Value::String(id) => consider_reference(
            id,
            source_id,
            document,
            local_type_ids,
            include_external,
            deps,
            seen_items,
        ),
        Value::Number(number) => consider_reference(
            &number.to_string(),
            source_id,
            document,
            local_type_ids,
            include_external,
            deps,
            seen_items,
        ),
        _ => {}
    }
}

fn is_rustdoc_backreference_key(key: &str) -> bool {
    key == "implementations"
}

fn consider_reference(
    id: &str,
    source_id: &str,
    document: &RustdocDocument<'_>,
    local_type_ids: &BTreeSet<String>,
    include_external: bool,
    deps: &mut BTreeSet<String>,
    seen_items: &mut BTreeSet<String>,
) {
    if id == source_id {
        return;
    }

    if local_type_ids.contains(id) {
        deps.insert(id.to_string());
        return;
    }

    if include_external && document.external_node(id).is_some() {
        deps.insert(id.to_string());
        return;
    }

    let Some(item) = document.index.get(id) else {
        return;
    };

    if !seen_items.insert(id.to_string()) {
        return;
    }

    if let Some(inner) = item.get("inner") {
        collect_type_refs(
            inner,
            source_id,
            document,
            local_type_ids,
            include_external,
            deps,
            seen_items,
        );
    }
}

fn item_crate_id(item: &Value) -> Option<String> {
    id_value_to_string(item.get("crate_id")?)
}

fn id_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => Some(number.to_string()),
        Value::String(string) => Some(string.clone()),
        _ => None,
    }
}

fn item_kind(item: &Value) -> Option<&str> {
    item.get("inner")
        .and_then(Value::as_object)
        .and_then(|inner| inner.keys().next())
        .map(String::as_str)
}

fn item_visibility(item: &Value, id: &str, document: &RustdocDocument<'_>) -> Visibility {
    match item.get("visibility") {
        Some(Value::String(visibility)) if visibility == "public" => Visibility::Public,
        Some(Value::String(visibility)) if visibility == "crate" => Visibility::Crate,
        Some(Value::Object(visibility)) => visibility
            .get("restricted")
            .and_then(Value::as_object)
            .and_then(|restricted| restricted.get("path"))
            .and_then(Value::as_str)
            .map(|path| restricted_visibility(path, id, document))
            .unwrap_or(Visibility::Restricted),
        _ => Visibility::Private,
    }
}

fn restricted_visibility(path: &str, id: &str, document: &RustdocDocument<'_>) -> Visibility {
    let Some(containing_module) = document.containing_module_path(id) else {
        return Visibility::Restricted;
    };

    if path == containing_module {
        return Visibility::Private;
    }

    if parent_module_path(&containing_module).as_deref() == Some(path) {
        return Visibility::Super;
    }

    if path == "::" {
        return Visibility::Crate;
    }

    Visibility::Restricted
}

fn parent_module_path(path: &str) -> Option<String> {
    if path == "::" {
        return None;
    }

    let trimmed = path.strip_prefix("::").unwrap_or(path);
    let Some((parent, _)) = trimmed.rsplit_once("::") else {
        return Some("::".to_string());
    };

    if parent.is_empty() {
        Some("::".to_string())
    } else {
        Some(format!("::{parent}"))
    }
}

fn is_type_definition_kind(kind: &str) -> bool {
    matches!(
        kind,
        "struct"
            | "enum"
            | "union"
            | "trait"
            | "type_alias"
            | "typedef"
            | "foreign_type"
            | "primitive"
            | "opaque_ty"
    )
}

fn escape_dot(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_html_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\n' => escaped.push_str("<BR/>"),
            '\r' => {}
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_to_stdout_and_private_rustdoc_items() {
        let config = Config::parse(std::iter::empty()).unwrap();

        assert!(config.output.is_none());
        assert!(config.document_private_items);
        assert_eq!(config.toolchain.as_deref(), Some("nightly"));
    }

    #[test]
    fn can_disable_private_rustdoc_items() {
        let config = Config::parse([OsString::from("--no-private")].into_iter()).unwrap();

        assert!(!config.document_private_items);
    }

    #[test]
    fn can_override_default_toolchain() {
        let config =
            Config::parse([OsString::from("--toolchain"), OsString::from("beta")].into_iter())
                .unwrap();

        assert_eq!(config.toolchain.as_deref(), Some("beta"));
    }

    #[test]
    fn can_write_dot_to_output_path() {
        let config =
            Config::parse([OsString::from("--output"), OsString::from("graph.dot")].into_iter())
                .unwrap();

        assert_eq!(config.output, Some(PathBuf::from("graph.dot")));
    }

    #[test]
    fn finds_best_rustdoc_json_and_ignores_other_json_files() {
        let root = temp_test_dir("rustdoc-json-selection");
        fs::create_dir_all(root.join("doc")).unwrap();
        fs::create_dir_all(root.join(".fingerprint")).unwrap();

        let rustdoc_json = root.join("doc").join("crate.json");
        let tiny_rustdoc_json = root.join("doc").join("tiny.json");
        let sidecar_json = root.join(".fingerprint").join("newer.json");
        fs::write(
            &rustdoc_json,
            r#"{
                "root":"0:0",
                "index":{
                    "0:0":{"crate_id":0,"name":"test_crate","inner":{"module":{"items":[]}}},
                    "0:1":{"crate_id":0,"name":"A","inner":{"struct":{"fields":[]}}},
                    "0:2":{"crate_id":0,"name":"B","inner":{"enum":{"variants":[]}}}
                },
                "paths":{},
                "format_version":57
            }"#,
        )
        .unwrap();
        fs::write(
            &tiny_rustdoc_json,
            r#"{
                "root":"0:0",
                "index":{
                    "0:0":{"crate_id":0,"name":"tiny_crate","inner":{"module":{"items":[]}}},
                    "0:1":{"crate_id":0,"name":"Only","inner":{"struct":{"fields":[]}}}
                },
                "paths":{},
                "format_version":57
            }"#,
        )
        .unwrap();
        fs::write(&sidecar_json, r#"{"package":"not rustdoc"}"#).unwrap();

        let found = best_rustdoc_json_file(&root).unwrap();

        fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, Some(rustdoc_json));
    }

    #[test]
    fn extracts_edges_through_struct_fields_and_aliases() {
        let json = json!({
            "root": "0:0",
            "index": {
                "0:0": item(0, "test_crate", json!({"module": {"items": ["0:1", "0:3"]}})),
                "0:1": item(0, "A", json!({"struct": {"fields": ["0:2"]}})),
                "0:2": item(0, "field", json!({"struct_field": {
                    "resolved_path": {"name": "B", "id": "0:4"}
                }})),
                "0:3": item(0, "Alias", json!({"type_alias": {
                    "type": {"resolved_path": {"name": "A", "id": "0:1"}}
                }})),
                "0:4": item(0, "B", json!({"struct": {"fields": []}}))
            },
            "paths": {
                "0:1": {"crate_id": 0, "path": ["test_crate", "A"], "kind": "struct"},
                "0:3": {"crate_id": 0, "path": ["test_crate", "Alias"], "kind": "type_alias"},
                "0:4": {"crate_id": 0, "path": ["test_crate", "B"], "kind": "struct"}
            }
        });

        let graph = TypeGraph::from_rustdoc_json(&json, false).unwrap();

        assert!(
            graph
                .edges
                .contains(&("0:1".to_string(), "0:4".to_string()))
        );
        assert!(
            graph
                .edges
                .contains(&("0:3".to_string(), "0:1".to_string()))
        );
    }

    #[test]
    fn can_include_external_type_nodes() {
        let json = json!({
            "root": "0:0",
            "index": {
                "0:0": item(0, "test_crate", json!({"module": {"items": ["0:1"]}})),
                "0:1": item(0, "A", json!({"struct": {"fields": ["0:2"]}})),
                "0:2": item(0, "field", json!({"struct_field": {
                    "resolved_path": {"name": "String", "id": "1:0"}
                }}))
            },
            "paths": {
                "0:1": {"crate_id": 0, "path": ["test_crate", "A"], "kind": "struct"},
                "1:0": {"crate_id": 1, "path": ["alloc", "string", "String"], "kind": "struct"}
            }
        });

        let graph = TypeGraph::from_rustdoc_json(&json, true).unwrap();

        assert!(graph.nodes.get("1:0").is_some_and(|node| node.external));
        assert!(
            graph
                .edges
                .contains(&("0:1".to_string(), "1:0".to_string()))
        );
    }

    #[test]
    fn supports_numeric_rustdoc_ids() {
        let json = json!({
            "root": 10,
            "index": {
                "10": item(0, "test_crate", json!({"module": {"items": [20, 30]}})),
                "20": item(0, "Cpu", json!({"struct": {
                    "kind": {"plain": {"fields": [21], "has_stripped_fields": false}},
                    "generics": {
                        "params": [{
                            "name": "M",
                            "kind": {"type": {
                                "bounds": [{
                                    "trait_bound": {
                                        "trait": {"path": "CpuMemory", "id": 30, "args": null},
                                        "generic_params": [],
                                        "modifier": "none"
                                    }
                                }],
                                "default": null,
                                "is_synthetic": false
                            }}
                        }],
                        "where_predicates": []
                    },
                    "impls": []
                }})),
                "21": item(0, "bus", json!({"struct_field": {
                    "resolved_path": {"name": "Bus", "id": 40}
                }})),
                "30": item(0, "CpuMemory", json!({"trait": {
                    "is_auto": false,
                    "is_unsafe": false,
                    "items": [],
                    "generics": {"params": [], "where_predicates": []},
                    "bounds": [],
                    "implementations": []
                }})),
                "40": item(0, "Bus", json!({"struct": {
                    "kind": {"plain": {"fields": [], "has_stripped_fields": false}},
                    "generics": {"params": [], "where_predicates": []},
                    "impls": []
                }}))
            },
            "paths": {
                "20": {"crate_id": 0, "path": ["test_crate", "Cpu"], "kind": "struct"},
                "30": {"crate_id": 0, "path": ["test_crate", "CpuMemory"], "kind": "trait"},
                "40": {"crate_id": 0, "path": ["test_crate", "Bus"], "kind": "struct"}
            }
        });

        let graph = TypeGraph::from_rustdoc_json(&json, false).unwrap();

        assert!(graph.edges.contains(&("20".to_string(), "30".to_string())));
        assert!(graph.edges.contains(&("20".to_string(), "40".to_string())));
    }

    #[test]
    fn ignores_trait_implementation_backreferences_but_keeps_type_impls() {
        let json = json!({
            "root": "0:0",
            "index": {
                "0:0": item(0, "test_crate", json!({"module": {"items": ["0:1", "0:2"]}})),
                "0:1": item(0, "Bus", json!({"trait": {
                    "is_auto": false,
                    "is_unsafe": false,
                    "items": [],
                    "generics": {"params": [], "where_predicates": []},
                    "bounds": [],
                    "implementations": ["0:3"]
                }})),
                "0:2": item(0, "Memory", json!({"struct": {
                    "kind": {"plain": {"fields": [], "has_stripped_fields": false}},
                    "generics": {"params": [], "where_predicates": []},
                    "impls": ["0:3"]
                }})),
                "0:3": item(0, "", json!({"impl": {
                    "is_unsafe": false,
                    "generics": {"params": [], "where_predicates": []},
                    "provided_trait_methods": [],
                    "trait": {"path": "Bus", "id": "0:1", "args": null},
                    "for": {"resolved_path": {"path": "Memory", "id": "0:2", "args": null}},
                    "items": [],
                    "is_negative": false,
                    "is_synthetic": false,
                    "blanket_impl": null
                }}))
            },
            "paths": {
                "0:1": {"crate_id": 0, "path": ["test_crate", "Bus"], "kind": "trait"},
                "0:2": {"crate_id": 0, "path": ["test_crate", "Memory"], "kind": "struct"}
            }
        });

        let graph = TypeGraph::from_rustdoc_json(&json, false).unwrap();

        assert!(
            !graph
                .edges
                .contains(&("0:1".to_string(), "0:2".to_string()))
        );
        assert!(
            graph
                .edges
                .contains(&("0:2".to_string(), "0:1".to_string()))
        );
    }

    #[test]
    fn colors_nodes_by_visibility() {
        let json = json!({
            "root": "0:0",
            "index": {
                "0:0": item(0, "test_crate", json!({"module": {"items": ["0:1", "0:2", "0:3", "0:4"]}})),
                "0:1": item(0, "PublicType", json!({"struct": {"fields": []}})),
                "0:2": crate_item(0, "CrateType", json!({"struct": {"fields": []}})),
                "0:3": restricted_item(
                    0,
                    "SuperType",
                    "::parent",
                    json!({"struct": {"fields": []}})
                ),
                "0:4": private_item(0, "PrivateType", json!({"struct": {"fields": []}}))
            },
            "paths": {
                "0:1": {"crate_id": 0, "path": ["test_crate", "PublicType"], "kind": "struct"},
                "0:2": {"crate_id": 0, "path": ["test_crate", "CrateType"], "kind": "struct"},
                "0:3": {"crate_id": 0, "path": ["test_crate", "parent", "child", "SuperType"], "kind": "struct"},
                "0:4": {"crate_id": 0, "path": ["test_crate", "PrivateType"], "kind": "struct"}
            }
        });

        let dot = TypeGraph::from_rustdoc_json(&json, false).unwrap().to_dot();

        assert!(dot.contains("node [shape=plain, fontname=\"monospace\"]"));
        assert!(dot.contains(
            "\"0:1\" [label=<<TABLE BORDER=\"1\" CELLBORDER=\"1\" CELLSPACING=\"0\" CELLPADDING=\"6\" COLOR=\"#2563eb\" BGCOLOR=\"#dbeafe\"><TR><TD><FONT FACE=\"monospace\">pub struct</FONT></TD></TR><TR><TD><FONT FACE=\"monospace\">test_crate::PublicType</FONT></TD></TR></TABLE>>]"
        ));
        assert!(dot.contains(
            "\"0:2\" [label=<<TABLE BORDER=\"1\" CELLBORDER=\"1\" CELLSPACING=\"0\" CELLPADDING=\"6\" COLOR=\"#16a34a\" BGCOLOR=\"#dcfce7\"><TR><TD><FONT FACE=\"monospace\">pub(crate) struct</FONT></TD></TR><TR><TD><FONT FACE=\"monospace\">test_crate::CrateType</FONT></TD></TR></TABLE>>]"
        ));
        assert!(dot.contains(
            "\"0:3\" [label=<<TABLE BORDER=\"1\" CELLBORDER=\"1\" CELLSPACING=\"0\" CELLPADDING=\"6\" COLOR=\"#d97706\" BGCOLOR=\"#fef3c7\"><TR><TD><FONT FACE=\"monospace\">pub(super) struct</FONT></TD></TR><TR><TD><FONT FACE=\"monospace\">test_crate::parent::child::SuperType</FONT></TD></TR></TABLE>>]"
        ));
        assert!(dot.contains(
            "\"0:4\" [label=<<TABLE BORDER=\"1\" CELLBORDER=\"1\" CELLSPACING=\"0\" CELLPADDING=\"6\" COLOR=\"#dc2626\" BGCOLOR=\"#fee2e2\"><TR><TD><FONT FACE=\"monospace\">private struct</FONT></TD></TR><TR><TD><FONT FACE=\"monospace\">test_crate::PrivateType</FONT></TD></TR></TABLE>>]"
        ));
        assert!(!dot.contains("subgraph cluster_legend"));
        assert!(dot.contains("pub(crate)"));
        assert!(dot.contains("pub(super)"));
    }

    fn item(crate_id: u64, name: &str, inner: Value) -> Value {
        json!({
            "crate_id": crate_id,
            "name": name,
            "visibility": "public",
            "inner": inner
        })
    }

    fn crate_item(crate_id: u64, name: &str, inner: Value) -> Value {
        json!({
            "crate_id": crate_id,
            "name": name,
            "visibility": "crate",
            "inner": inner
        })
    }

    fn restricted_item(crate_id: u64, name: &str, path: &str, inner: Value) -> Value {
        json!({
            "crate_id": crate_id,
            "name": name,
            "visibility": {"restricted": {"parent": 0, "path": path}},
            "inner": inner
        })
    }

    fn private_item(crate_id: u64, name: &str, inner: Value) -> Value {
        json!({
            "crate_id": crate_id,
            "name": name,
            "visibility": {"restricted": {"parent": 0, "path": "::"}},
            "inner": inner
        })
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!("cargo-typegraph-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }
}
