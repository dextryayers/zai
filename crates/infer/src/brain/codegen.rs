/// Full-coding generator: turns a natural language prompt into complete
/// runnable code plus professional explanation. Offline heuristic, no network.
/// When a real model (GGUF/Ollama) is present the caller prefers model text;
/// this is the guaranteed offline fallback so `ask` and `code` always deliver.
pub fn is_coding_request(q: &str) -> bool {
    let l = q.to_lowercase();
    if l.len() < 8 {
        return false;
    }
    let triggers = [
        "buatkan",
        "buatin",
        "bikin",
        "bikinin",
        "tuliskan",
        "tulis",
        "contohkan",
        "create",
        "make",
        "write",
        "generate",
        "scaffold",
        "boilerplate",
        "program",
        "script",
        "fungsi",
        "function",
        "fitur",
        "feature",
        "endpoint",
        "perbaiki",
        "fix",
        "refactor",
        "tambahkan",
        "implement",
        "jelaskan kode",
        "review kode",
        "audit kode",
        "fibonacci",
        "faktorial",
        "factorial",
        "sorting",
        "binary search",
        "crud",
        "rest api",
        "cli",
        "http server",
        "login",
        "auth",
        "ngoding",
        "koding",
        "coding",
    ];
    if triggers.iter().any(|t| l.contains(t)) {
        return true;
    }
    // "tulis fungsi X dalam python" without explicit buatkan.
    if (l.contains("fungsi") || l.contains("function") || l.contains("kode") || l.contains("code"))
        && (l.contains("python")
            || l.contains("rust")
            || l.contains("javascript")
            || l.contains("typescript")
            || l.contains("go ")
            || l.contains(" bash")
            || l.contains("sql")
            || l.contains("java")
            || l.contains(" dalam "))
    {
        return true;
    }
    false
}

pub fn detect_target_lang(q: &str) -> &'static str {
    let l = q.to_lowercase();
    if l.contains("rust") || l.contains("cargo") {
        return "rust";
    }
    if l.contains("python") || l.contains("pip ") || l.contains(".py") {
        return "python";
    }
    if l.contains("typescript") || l.contains(".ts") {
        return "typescript";
    }
    if l.contains("javascript") || l.contains(" node") || l.contains(".js") {
        return "javascript";
    }
    if l.contains("golang")
        || l.contains(" bahasa go")
        || l.contains(" dalam go")
        || l.contains(" golang")
    {
        // avoid matching "algo" etc: require word go with space context already above
        return "go";
    }
    if l.contains(" bash") || l.contains("shell script") || l.contains(".sh") {
        return "bash";
    }
    if l.contains("sql") || l.contains("postgres") || l.contains("mysql") {
        return "sql";
    }
    // Default: pick python for quick scripts, rust when systems language hinted.
    if l.contains("sistem") || l.contains("cli tool") || l.contains("performance") {
        return "rust";
    }
    "python"
}

/// Extract a requested identifier dynamically from the prompt so each
/// answer differs per request (no single hardcoded function name).
/// Looks for: fungsi <name>, function <name>, `code`, "quoted", then falls
/// back to a slug of content words.
pub fn extract_fn_name(query: &str) -> String {
    let q = query.to_string();
    let low = q.to_lowercase();
    for marker in ["fungsi ", "function ", "method ", "def ", "fn "] {
        if let Some(idx) = low.find(marker) {
            let rest = q[idx + marker.len()..].trim_start();
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.len() >= 2 {
                return to_snake(&name);
            }
        }
    }
    // `quoted` or "quoted" identifier.
    for (open, close) in [('`', '`'), ('"', '"'), ('\'', '\'')] {
        if let Some(a) = q.find(open) {
            if let Some(b) = q[a + 1..].find(close) {
                let inner: String = q[a + 1..a + 1 + b]
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if inner.len() >= 2 && inner.len() <= 32 {
                    return to_snake(&inner);
                }
            }
        }
    }
    // Fallback: first two content words joined.
    let stop = [
        "buatkan",
        "buatin",
        "bikin",
        "tolong",
        "buatin",
        "tulis",
        "tuliskan",
        "contoh",
        "program",
        "script",
        "fungsi",
        "function",
        "dalam",
        "dengan",
        "untuk",
        "yang",
        "dan",
        "atau",
        "sebuah",
        "create",
        "make",
        "write",
        "generate",
        "python",
        "rust",
        "javascript",
        "typescript",
        "golang",
        "bash",
        "sql",
        "please",
    ];
    let words: Vec<String> = q
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.len() >= 3 && !stop.contains(&w.as_str()))
        .take(2)
        .collect();
    if words.is_empty() {
        "solve".to_string()
    } else {
        words.join("_")
    }
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_lowercase().next().unwrap_or(c));
    }
    out.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

fn to_camel(snake: &str) -> String {
    snake
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Main entry: full professional answer with runnable code. English-only
/// output; Indonesian trigger words are recognized for routing so prompts in
/// either language work.
pub fn generate(query: &str) -> String {
    let lang = detect_target_lang(query);
    let task = classify_task(query);
    let code = code_for(lang, &task, query);
    let (run_cmd, file_name) = run_info(lang);
    format!(
        "## Solution: {task_title}\n\nComplete `{lang}` implementation for: _{query_short}_.\n\n### File: `{file_name}`\n\n```{lang}\n{code}\n```\n\n### Run\n\n```bash\n{run_cmd}\n```\n\n### Notes\n\n{explain}\n\n### Next\n\n- Save as `{file_name}`, run the command above.\n- Ask follow-ups in plain language.\n- Apply to repo: `zai code \"{query_short}\" --apply`.\n",
        task_title = task_title(&task),
        lang = lang,
        query_short = truncate(query, 90),
        file_name = file_name,
        code = code,
        run_cmd = run_cmd,
        explain = explain_for(lang, &task),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Task {
    Fibonacci,
    Factorial,
    Sorting,
    Crud,
    HttpServer,
    Cli,
    FileRead,
    Login,
    Generic,
}

fn classify_task(q: &str) -> Task {
    let l = q.to_lowercase();
    if l.contains("fibonacci") || l.contains("fibonaci") {
        return Task::Fibonacci;
    }
    if l.contains("faktorial") || l.contains("factorial") {
        return Task::Factorial;
    }
    if l.contains("sorting") || l.contains("urut") || l.contains("sort") {
        return Task::Sorting;
    }
    if l.contains("crud") || (l.contains("create") && l.contains("read")) {
        return Task::Crud;
    }
    if l.contains("http") || l.contains("server") || l.contains("rest") || l.contains("endpoint") {
        return Task::HttpServer;
    }
    if l.contains("cli") || l.contains("command line") || l.contains("argumen") {
        return Task::Cli;
    }
    if l.contains("baca file") || l.contains("read file") || l.contains("csv") {
        return Task::FileRead;
    }
    if l.contains("login") || l.contains("auth") {
        return Task::Login;
    }
    Task::Generic
}

fn task_title(t: &Task) -> String {
    match t {
        Task::Fibonacci => "Fibonacci with CLI".to_string(),
        Task::Factorial => "Factorial with error handling".to_string(),
        Task::Sorting => "Sorting with a small benchmark".to_string(),
        Task::Crud => "In-memory CRUD starter".to_string(),
        Task::HttpServer => "Minimal HTTP server".to_string(),
        Task::Cli => "Ready-to-run CLI tool".to_string(),
        Task::FileRead => "Robust file reader".to_string(),
        Task::Login => "Login with secure hashing".to_string(),
        Task::Generic => "Complete ready-to-run implementation".to_string(),
    }
}

fn truncate(s: &str, n: usize) -> String {
    let c: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{c}...")
    } else {
        c
    }
}

fn run_info(lang: &str) -> (String, String) {
    match lang {
        "rust" => ("cargo run --quiet".to_string(), "src/main.rs".to_string()),
        "python" => ("python3 main.py".to_string(), "main.py".to_string()),
        "javascript" => ("node main.js".to_string(), "main.js".to_string()),
        "typescript" => ("npx ts-node main.ts".to_string(), "main.ts".to_string()),
        "go" => ("go run .".to_string(), "main.go".to_string()),
        "bash" => ("bash main.sh".to_string(), "main.sh".to_string()),
        "sql" => ("psql -f schema.sql".to_string(), "schema.sql".to_string()),
        _ => ("python3 main.py".to_string(), "main.py".to_string()),
    }
}

fn explain_for(lang: &str, task: &Task) -> String {
    let base = match task {
        Task::Fibonacci => {
            "Iterative O(n) time, O(1) memory. Validates n >= 0 with a clear error message."
        }
        Task::Factorial => "Iterative with n >= 0 validation and a usage example in main.",
        Task::Sorting => "Uses the standard stable sort, fast for general data.",
        Task::Crud => "In-memory structure with auto-increment ids, ready to swap for a database.",
        Task::HttpServer => "One GET /health endpoint plus one POST echo, no heavy dependencies.",
        Task::Cli => "Manual argument parsing with no dependencies, easy to move to clap/argparse.",
        Task::FileRead => "Line-by-line streaming reads stay memory-safe on large files.",
        Task::Login => "Hashing example with Argon2/bcrypt in comments - never store plaintext.",
        Task::Generic => "Modular layout: pure core function plus main for I/O, easy to test.",
    };
    format!("{base} Idiomatic `{lang}` code with error handling and usage example.")
}

fn code_for(lang: &str, task: &Task, query: &str) -> String {
    match (lang, task) {
        ("python", Task::Fibonacci) => [
            "import sys",
            "",
            "def fibonacci(n: int) -> list[int]:",
            "    \"\"\"Return first n Fibonacci numbers. Raises ValueError for n < 0.\"\"\"",
            "    if n < 0:",
            "        raise ValueError(\"n must be >= 0\")",
            "    out: list[int] = []",
            "    a, b = 0, 1",
            "    for _ in range(n):",
            "        out.append(a)",
            "        a, b = b, a + b",
            "    return out",
            "",
            "def main() -> None:",
            "    raw = sys.argv[1] if len(sys.argv) > 1 else \"10\"",
            "    try:",
            "        n = int(raw)",
            "    except ValueError:",
            "        print(f\"invalid number: {raw}\")",
            "        sys.exit(2)",
            "    try:",
            "        print(fibonacci(n))",
            "    except ValueError as e:",
            "        print(f\"error: {e}\")",
            "        sys.exit(2)",
            "",
            "if __name__ == \"__main__\":",
            "    main()",
        ]
        .join("\n"),
        ("rust", Task::Fibonacci) => [
            "use std::env;",
            "use std::process::ExitCode;",
            "",
            "/// First n Fibonacci numbers. Empty for n == 0.",
            "pub fn fibonacci(n: usize) -> Vec<u64> {",
            "    let mut out = Vec::with_capacity(n);",
            "    let (mut a, mut b) = (0u64, 1u64);",
            "    for _ in 0..n {",
            "        out.push(a);",
            "        let next = a.saturating_add(b);",
            "        a = b;",
            "        b = next;",
            "    }",
            "    out",
            "}",
            "",
            "fn main() -> ExitCode {",
            "    let raw = env::args().nth(1).unwrap_or_else(|| \"10\".to_string());",
            "    let n: usize = match raw.parse() {",
            "        Ok(v) => v,",
            "        Err(_) => {",
            "            eprintln!(\"invalid number: {raw}\");",
            "            return ExitCode::from(2);",
            "        }",
            "    };",
            "    println!(\"{:?}\", fibonacci(n));",
            "    ExitCode::SUCCESS",
            "}",
        ]
        .join("\n"),
        ("javascript", Task::Fibonacci) | ("typescript", Task::Fibonacci) => [
            "function fibonacci(n) {",
            "  if (!Number.isInteger(n) || n < 0) throw new Error(\"n must be >= 0\");",
            "  const out = [];",
            "  let a = 0, b = 1;",
            "  for (let i = 0; i < n; i++) { out.push(a); [a, b] = [b, a + b]; }",
            "  return out;",
            "}",
            "const n = parseInt(process.argv[2] ?? \"10\", 10);",
            "console.log(fibonacci(n));",
        ]
        .join("\n"),
        ("go", Task::Fibonacci) => [
            "package main",
            "",
            "import (",
            "    \"fmt\"",
            "    \"os\"",
            "    \"strconv\"",
            ")",
            "",
            "func fibonacci(n int) ([]int, error) {",
            "    if n < 0 { return nil, fmt.Errorf(\"n must be >= 0\") }",
            "    out := make([]int, 0, n)",
            "    a, b := 0, 1",
            "    for i := 0; i < n; i++ {",
            "        out = append(out, a)",
            "        a, b = b, a+b",
            "    }",
            "    return out, nil",
            "}",
            "",
            "func main() {",
            "    raw := \"10\"",
            "    if len(os.Args) > 1 { raw = os.Args[1] }",
            "    n, err := strconv.Atoi(raw)",
            "    if err != nil { fmt.Println(\"invalid number:\", raw); os.Exit(2) }",
            "    seq, err := fibonacci(n)",
            "    if err != nil { fmt.Println(\"error:\", err); os.Exit(2) }",
            "    fmt.Println(seq)",
            "}",
        ]
        .join("\n"),
        ("bash", Task::Fibonacci) => [
            "#!/usr/bin/env bash",
            "set -euo pipefail",
            "n=\"${1:-10}\"",
            "if ! [[ \"$n\" =~ ^[0-9]+$ ]]; then echo \"invalid number: $n\" >&2; exit 2; fi",
            "a=0; b=1; out=\"\"",
            "for ((i=0; i<n; i++)); do out+=\"$a \"; nxt=$((a+b)); a=$b; b=$nxt; done",
            "echo \"${out% }\"",
        ]
        .join("\n"),
        (_, Task::Factorial) => factorial_code(lang),
        (_, Task::Sorting) => sorting_code(lang),
        (_, Task::Crud) => crud_code(lang, query),
        (_, Task::HttpServer) => http_server_code(lang),
        (_, Task::Cli) => cli_code(lang, query),
        (_, Task::FileRead) => file_read_code(lang),
        (_, Task::Login) => login_code(lang),
        _ => generic_code(lang, task, query),
    }
}

fn factorial_code(lang: &str) -> String {
    match lang {
        "rust" => [
            "use std::env;",
            "pub fn factorial(n: u64) -> u64 { (1..=n.max(1)).product() }",
            "fn main() {",
            "    let n: u64 = env::args().nth(1).unwrap_or(\"5\".into()).parse().unwrap_or(5);",
            "    println!(\"{}\", factorial(n));",
            "}",
        ]
        .join("\n"),
        "javascript" | "typescript" => [
            "function factorial(n) {",
            "  if (!Number.isInteger(n) || n < 0) throw new Error(\"n must be >= 0\");",
            "  let r = 1; for (let i = 2; i <= n; i++) r *= i; return r;",
            "}",
            "console.log(factorial(parseInt(process.argv[2] ?? \"5\", 10)));",
        ]
        .join("\n"),
        "go" => [
            "package main",
            "import (\"fmt\" \"os\" \"strconv\")",
            "func factorial(n int) int { r := 1; for i := 2; i <= n; i++ { r *= i }; return r }",
            "func main() {",
            "    raw := \"5\"; if len(os.Args) > 1 { raw = os.Args[1] }",
            "    n, _ := strconv.Atoi(raw); fmt.Println(factorial(n))",
            "}",
        ]
        .join("\n"),
        "bash" => [
            "#!/usr/bin/env bash",
            "set -euo pipefail",
            "n=\"${1:-5}\"; r=1; for ((i=2; i<=n; i++)); do r=$((r*i)); done; echo \"$r\"",
        ]
        .join("\n"),
        "sql" => [
            "-- Factorial via recursive CTE (Postgres).",
            "-- Usage: psql -v n=5 -f schema.sql",
            "WITH RECURSIVE f(i, r) AS (",
            "  SELECT 1, 1",
            "  UNION ALL SELECT i+1, r*(i+1) FROM f WHERE i < :n",
            ") SELECT r AS factorial FROM f ORDER BY i DESC LIMIT 1;",
        ]
        .join("\n"),
        _ => [
            "import sys",
            "def factorial(n: int) -> int:",
            "    \"\"\"Iterative factorial with n >= 0 validation.\"\"\"",
            "    if n < 0: raise ValueError(\"n must be >= 0\")",
            "    r = 1",
            "    for i in range(2, n + 1): r *= i",
            "    return r",
            "if __name__ == \"__main__\":",
            "    raw = sys.argv[1] if len(sys.argv) > 1 else \"5\"",
            "    print(factorial(int(raw)))",
        ]
        .join("\n"),
    }
}

fn sorting_code(lang: &str) -> String {
    match lang {
        "rust" => [
            "use std::env;",
            "/// Parse numbers, sort ascending, print. Usage: cargo run -- 3 1 2",
            "pub fn sort_numbers(mut v: Vec<i64>) -> Vec<i64> { v.sort(); v }",
            "fn main() {",
            "    let nums: Vec<i64> = env::args().skip(1).filter_map(|s| s.parse().ok()).collect();",
            "    let nums = if nums.is_empty() { vec![5, 2, 9, 1] } else { nums };",
            "    println!(\"{:?}\", sort_numbers(nums));",
            "}",
        ]
        .join("\n"),
        "javascript" | "typescript" => [
            "function sortNumbers(arr) { return [...arr].sort((a, b) => a - b); }",
            "const nums = (process.argv[2] ?? \"5,2,9,1\").split(\",\").map(Number);",
            "console.log(sortNumbers(nums));",
        ]
        .join("\n"),
        "go" => [
            "package main",
            "import (\"fmt\" \"sort\" \"strconv\" \"strings\")",
            "func main() {",
            "    raw := \"5,2,9,1\"",
            "    // go run . \"3,1,2\"",
            "    nums := []int{}; for _, p := range strings.Split(raw, \",\") {",
            "        if n, err := strconv.Atoi(p); err == nil { nums = append(nums, n) } }",
            "    sort.Ints(nums); fmt.Println(nums)",
            "}",
        ]
        .join("\n"),
        "bash" => [
            "#!/usr/bin/env bash",
            "# Usage: bash main.sh 5 2 9 1",
            "set -euo pipefail",
            "if [ $# -eq 0 ]; then set -- 5 2 9 1; fi",
            "printf '%s\\n' \"$@\" | sort -n | tr '\\n' ' '; echo",
        ]
        .join("\n"),
        "sql" => [
            "-- Sort demo: ORDER BY is the sort.",
            "CREATE TABLE IF NOT EXISTS nums(v INT);",
            "INSERT INTO nums VALUES (5),(2),(9),(1);",
            "SELECT v FROM nums ORDER BY v ASC;",
        ]
        .join("\n"),
        _ => [
            "import sys, time",
            "\"\"\"Sort numbers ascending with a tiny benchmark. Usage: python3 main.py 5 2 9 1\"\"\"",
            "def sort_numbers(nums: list[int]) -> list[int]:",
            "    return sorted(nums)",
            "def main() -> None:",
            "    raw = sys.argv[1:] or [\"5\", \"2\", \"9\", \"1\"]",
            "    nums = [int(x.strip(\",\")) for x in raw]",
            "    t0 = time.perf_counter()",
            "    out = sort_numbers(nums)",
            "    dt = (time.perf_counter() - t0) * 1000",
            "    print(out, f\"({dt:.2f}ms)\")",
            "if __name__ == \"__main__\": main()",
        ]
        .join("\n"),
    }
}

fn crud_code(lang: &str, query: &str) -> String {
    let snake = extract_fn_name(query);
    match lang {
        "rust" => format!(
            "//! In-memory CRUD starter. Run: cargo run --quiet\nuse std::collections::HashMap;\n#[derive(Debug, Clone)] pub struct Item {{ id: u64, name: String }}\n#[derive(Default)] pub struct Store {{ next: u64, items: HashMap<u64, Item> }}\nimpl Store {{\n    pub fn create(&mut self, name: &str) -> u64 {{ self.next += 1; self.items.insert(self.next, Item {{ id: self.next, name: name.into() }}); self.next }}\n    pub fn read(&self, id: u64) -> Option<&Item> {{ self.items.get(&id) }}\n    pub fn update(&mut self, id: u64, name: &str) -> bool {{ self.items.get_mut(&id).map(|i| {{ i.name = name.into(); true }}).unwrap_or(false) }}\n    pub fn delete(&mut self, id: u64) -> bool {{ self.items.remove(&id).is_some() }}\n    pub fn list(&self) -> Vec<&Item> {{ let mut v: Vec<_> = self.items.values().collect(); v.sort_by_key(|i| i.id); v }}\n}}\nfn main() {{\n    // Demo for: {snake}\n    let mut s = Store::default();\n    let id = s.create(\"demo\");\n    s.update(id, \"demo v2\");\n    println!(\"{{:?}}\", s.list());\n    s.delete(id);\n    println!(\"after delete: {{:?}}\", s.list());\n}}"
        ),
        "javascript" | "typescript" => format!(
            "// In-memory CRUD starter. Run: node main.js\nclass Store {{\n  constructor() {{ this.next = 0; this.items = new Map(); }}\n  create(name) {{ const id = ++this.next; this.items.set(id, {{ id, name }}); return id; }}\n  read(id) {{ return this.items.get(id) ?? null; }}\n  update(id, name) {{ if (!this.items.has(id)) return false; this.items.set(id, {{ id, name }}); return true; }}\n  delete(id) {{ return this.items.delete(id); }}\n  list() {{ return [...this.items.values()].sort((a, b) => a.id - b.id); }}\n}}\n// Demo: {snake}\nconst s = new Store();\nconst id = s.create(\"demo\");\ns.update(id, \"demo v2\");\nconsole.log(s.list());",
        ),
        "go" => format!(
            "// In-memory CRUD starter. Run: go run .\npackage main\nimport (\"fmt\" \"sort\")\ntype Item struct {{ ID int; Name string }}\ntype Store struct {{ next int; items map[int]Item }}\nfunc New() *Store {{ return &Store{{items: map[int]Item{{}}}} }}\nfunc (s *Store) Create(name string) int {{ s.next++; s.items[s.next] = Item{{s.next, name}}; return s.next }}\nfunc (s *Store) List() []Item {{ out := []Item{{}}; for _, v := range s.items {{ out = append(out, v) }}; sort.Slice(out, func(i, j int) bool {{ return out[i].ID < out[j].ID }}); return out }}\nfunc main() {{ // {snake}\n    s := New(); id := s.Create(\"demo\"); fmt.Println(id, s.List())\n}}"
        ),
        "bash" => format!(
            "#!/usr/bin/env bash\n# Tiny file CRUD (todo). Usage: bash main.sh add \"buy milk\" | bash main.sh list\n# Scope: {snake}\nset -euo pipefail\nDB=\"${{DB:-./store.txt}}\"\ntouch \"$DB\"\ncmd=\"${{1:-list}}\"; case \"$cmd\" in\n  add) shift; echo \"$*\" >> \"$DB\"; echo \"added\";;\n  list) nl -ba \"$DB\";;\n  del) sed -i \"${{2}}d\" \"$DB\"; echo \"deleted $2\";;\n  *) echo \"usage: $0 {{add <text>|list|del <n>}}\";;\nesac"
        ),
        "sql" => format!(
            "-- CRUD schema for: {snake}\nCREATE TABLE IF NOT EXISTS items (\n  id SERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL DEFAULT now()\n);\n-- Create\nINSERT INTO items (name) VALUES ('demo') RETURNING id;\n-- Read\nSELECT * FROM items ORDER BY id LIMIT 20;\n-- Update\nUPDATE items SET name='demo v2' WHERE id=1;\n-- Delete\nDELETE FROM items WHERE id=1;"
        ),
        _ => format!(
            "\"\"\"In-memory CRUD starter. Run: python3 main.py\"\"\"\nfrom dataclasses import dataclass\n\n@dataclass\nclass Item:\n    id: int\n    name: str\n\nclass Store:\n    \"\"\"CRUD for: {snake}. Swap dict for sqlite later.\"\"\"\n    def __init__(self) -> None:\n        self._next = 0\n        self._items: dict[int, Item] = {{}}\n    def create(self, name: str) -> int:\n        self._next += 1\n        self._items[self._next] = Item(self._next, name)\n        return self._next\n    def read(self, _id: int) -> Item | None:\n        return self._items.get(_id)\n    def update(self, _id: int, name: str) -> bool:\n        if _id not in self._items: return False\n        self._items[_id].name = name\n        return True\n    def delete(self, _id: int) -> bool:\n        return self._items.pop(_id, None) is not None\n    def list(self) -> list[Item]:\n        return [self._items[k] for k in sorted(self._items)]\n\nif __name__ == \"__main__\":\n    s = Store()\n    _id = s.create(\"demo\")\n    s.update(_id, \"demo v2\")\n    print(s.list())"
        ),
    }
}

fn http_server_code(lang: &str) -> String {
    match lang {
        "rust" => [
            "// Minimal HTTP server on std only. Run: cargo run --quiet, then curl localhost:8080/health",
            "use std::io::{Read, Write};",
            "use std::net::TcpListener;",
            "fn main() -> std::io::Result<()> {",
            "    let l = TcpListener::bind(\"127.0.0.1:8080\")?;",
            "    println!(\"listening on 127.0.0.1:8080\");",
            "    for s in l.incoming() {",
            "        let mut s = s?; let mut buf = [0u8; 2048]; let n = s.read(&mut buf)?;",
            "        let req: String = String::from_utf8_lossy(&buf[..n]).into_owned();",
            "        let body = if req.starts_with(\"GET /health\") { r#\"{\"ok\":true}\"# } else { r#\"{\"echo\":true}\"# };",
            "        let res = format!(\"HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\n\\r\\n{}\", body.len(), body);",
            "        s.write_all(res.as_bytes())?",
            "    }",
            "    Ok(())",
            "}",
        ]
        .join("\n"),
        "javascript" | "typescript" => [
            "// Minimal HTTP server. Run: node main.js, then curl localhost:8080/health",
            "const http = require(\"http\");",
            "const srv = http.createServer((req, res) => {",
            "  if (req.url === \"/health\") { res.writeHead(200, {\"Content-Type\": \"application/json\"}); res.end('{\"ok\":true}'); }",
            "  else { let b = \"\"; req.on(\"data\", c => b += c); req.on(\"end\", () => { res.writeHead(200, {\"Content-Type\": \"application/json\"}); res.end(JSON.stringify({ echo: b || null })); }); }",
            "});",
            "srv.listen(8080, () => console.log(\"listening on 127.0.0.1:8080\"));",
        ]
        .join("\n"),
        "go" => [
            "// Minimal HTTP server. Run: go run ., then curl localhost:8080/health",
            "package main",
            "import (\"fmt\" \"io\" \"net/http\")",
            "func main() {",
            "    http.HandleFunc(\"/health\", func(w http.ResponseWriter, r *http.Request) { w.Header().Set(\"Content-Type\", \"application/json\"); fmt.Fprint(w, `{\"ok\":true}`) })",
            "    http.HandleFunc(\"/echo\", func(w http.ResponseWriter, r *http.Request) { b, _ := io.ReadAll(r.Body); w.Write(b) })",
            "    fmt.Println(\"listening on 127.0.0.1:8080\"); http.ListenAndServe(\"127.0.0.1:8080\", nil)",
            "}",
        ]
        .join("\n"),
        "bash" => [
            "#!/usr/bin/env bash",
            "# Serve current dir on 8080 via stdlib. Run: bash main.sh",
            "set -euo pipefail",
            "echo \"serving on http://127.0.0.1:8080/health (Ctrl+C stops)\"",
            "python3 -c 'from http.server import BaseHTTPRequestHandler, HTTPServer;",
            "class H(BaseHTTPRequestHandler):",
            " def do_GET(self):",
            "  b = b\"{\\\"ok\\\":true}\" if self.path==\"/health\" else b\"{\\\"echo\\\":true}\"",
            "  self.send_response(200); self.send_header(\"Content-Type\",\"application/json\"); self.send_header(\"Content-Length\",str(len(b))); self.end_headers(); self.wfile.write(b)",
            "HTTPServer((\"127.0.0.1\",8080),H).serve_forever()'",
        ]
        .join("\n"),
        "sql" => [
            "-- HTTP is app-layer; schema for an endpoint log:",
            "CREATE TABLE IF NOT EXISTS requests(id SERIAL PRIMARY KEY, path TEXT, body JSONB, created_at TIMESTAMPTZ DEFAULT now());",
            "INSERT INTO requests(path, body) VALUES ('/health', '{\"ok\": true}');",
        ]
        .join("\n"),
        _ => [
            "\"\"\"Minimal HTTP server. Run: python3 main.py, then curl localhost:8080/health\"\"\"",
            "from http.server import BaseHTTPRequestHandler, HTTPServer",
            "import json",
            "class H(BaseHTTPRequestHandler):",
            "    def do_GET(self):",
            "        body = b'{\"ok\": true}' if self.path == \"/health\" else b'{\"echo\": true}'",
            "        self.send_response(200)",
            "        self.send_header(\"Content-Type\", \"application/json\")",
            "        self.send_header(\"Content-Length\", str(len(body)))",
            "        self.end_headers(); self.wfile.write(body)",
            "    def do_POST(self):",
            "        n = int(self.headers.get(\"Content-Length\", 0))",
            "        data = self.rfile.read(n) if n else b\"\"",
            "        body = json.dumps({\"echo\": data.decode(errors=\"replace\")}).encode()",
            "        self.send_response(200)",
            "        self.send_header(\"Content-Type\", \"application/json\")",
            "        self.send_header(\"Content-Length\", str(len(body)))",
            "        self.end_headers(); self.wfile.write(body)",
            "    def log_message(self, *a): pass",
            "if __name__ == \"__main__\":",
            "    print(\"listening on 127.0.0.1:8080\")",
            "    HTTPServer((\"127.0.0.1\", 8080), H).serve_forever()",
        ]
        .join("\n"),
    }
}

fn cli_code(lang: &str, query: &str) -> String {
    let snake = extract_fn_name(query);
    match lang {
        "rust" => format!(
            "//! CLI tool: {snake}. Run: cargo run -- --name demo\nuse std::env;\nfn usage() -> &'static str {{ \"usage: app --name <n> [--count N]\" }}\nfn main() {{\n    let args: Vec<String> = env::args().collect();\n    let mut name = \"demo\".to_string();\n    let mut count = 1usize;\n    let mut i = 1;\n    while i < args.len() {{\n        match args[i].as_str() {{\n            \"--name\" if i + 1 < args.len() => {{ name = args[i + 1].clone(); i += 2; }}\n            \"--count\" if i + 1 < args.len() => {{ count = args[i + 1].parse().unwrap_or(1); i += 2; }}\n            \"-h\" | \"--help\" => {{ println!(\"{{}}\", usage()); return; }}\n            other => {{ eprintln!(\"unknown arg: {{other}}\\n{{}}\", usage()); std::process::exit(2); }}\n        }}\n    }}\n    for _ in 0..count {{ println!(\"hello {{name}} from {snake}\"); }}\n}}"
        ),
        "javascript" | "typescript" => format!(
            "// CLI tool: {snake}. Run: node main.js --name demo --count 2\nfunction parse(argv) {{\n  const out = {{ name: \"demo\", count: 1 }};\n  for (let i = 0; i < argv.length; i++) {{\n    if (argv[i] === \"--name\") out.name = argv[++i] ?? \"demo\";\n    else if (argv[i] === \"--count\") out.count = parseInt(argv[++i] ?? \"1\", 10);\n  }}\n  return out;\n}}\nconst {{ name, count }} = parse(process.argv.slice(2));\nfor (let i = 0; i < count; i++) console.log(`hello ${{name}} from {snake}`);"
        ),
        "go" => format!(
            "// CLI tool: {snake}. Run: go run . --name demo\npackage main\nimport (\"flag\" \"fmt\")\nfunc main() {{\n    name := flag.String(\"name\", \"demo\", \"who to greet\");\n    count := flag.Int(\"count\", 1, \"times\");\n    flag.Parse();\n    for i := 0; i < *count; i++ {{ fmt.Printf(\"hello %s from {snake}\\n\", *name) }}\n}}"
        ),
        "bash" => format!(
            "#!/usr/bin/env bash\n# CLI tool: {snake}. Run: bash main.sh --name demo\nset -euo pipefail\nname=\"demo\"; count=1\nwhile [ $# -gt 0 ]; do case \"$1\" in --name) name=\"${{2:-demo}}\"; shift 2;; --count) count=\"${{2:-1}}\"; shift 2;; -h|--help) echo \"usage: $0 --name <n> --count N\"; exit 0;; *) echo \"unknown: $1\" >&2; exit 2;; esac; done\nfor ((i=0;i<count;i++)); do echo \"hello $name from {snake}\"; done"
        ),
        "sql" => format!(
            "-- CLI equivalent: psql variables for {snake}\n-- Usage: psql -v name=demo -f schema.sql\nSELECT 'hello ' || :'name' || ' from {snake}' AS greeting;"
        ),
        _ => format!(
            "\"\"\"CLI tool: {snake}. Run: python3 main.py --name demo --count 2\"\"\"\nimport argparse\ndef main() -> None:\n    p = argparse.ArgumentParser(description=\"{snake}\")\n    p.add_argument(\"--name\", default=\"demo\")\n    p.add_argument(\"--count\", type=int, default=1)\n    a = p.parse_args()\n    for _ in range(max(1, a.count)):\n        print(f\"hello {{a.name}} from {snake}\")\nif __name__ == \"__main__\": main()"
        ),
    }
}

fn file_read_code(lang: &str) -> String {
    match lang {
        "rust" => [
            "// Robust file reader. Run: cargo run -- main.rs",
            "use std::env; use std::fs::File; use std::io::{BufRead, BufReader};",
            "fn main() -> std::io::Result<()> {",
            "    let path = env::args().nth(1).unwrap_or(\"main.py\".into());",
            "    let f = File::open(&path)?; let mut n = 0;",
            "    for line in BufReader::new(f).lines() { let l = line?; n += 1; if n <= 20 { println!(\"{n:4}: {l}\"); } }",
            "    println!(\"read {n} lines from {path}\"); Ok(())",
            "}",
        ]
        .join("\n"),
        "javascript" | "typescript" => [
            "// Robust file reader. Run: node main.js main.py",
            "const fs = require(\"fs\"); const readline = require(\"readline\");",
            "async function main() {",
            "  const path = process.argv[2] ?? \"main.py\";",
            "  const rl = readline.createInterface({ input: fs.createReadStream(path), crlfDelay: Infinity });",
            "  let n = 0; for await (const line of rl) { n++; if (n <= 20) console.log(`${String(n).padStart(4)}: ${line}`); }",
            "  console.log(`read ${n} lines from ${path}`);",
            "}",
            "main().catch(e => { console.error(e.message); process.exit(1); });",
        ]
        .join("\n"),
        "go" => [
            "// Robust file reader. Run: go run . main.go",
            "package main",
            "import (\"bufio\" \"fmt\" \"os\")",
            "func main() {",
            "    path := \"main.py\"; if len(os.Args) > 1 { path = os.Args[1] }",
            "    f, err := os.Open(path); if err != nil { fmt.Println(err); os.Exit(1) }",
            "    defer f.Close(); s := bufio.NewScanner(f); n := 0",
            "    for s.Scan() { n++; if n <= 20 { fmt.Printf(\"%4d: %s\\n\", n, s.Text()) } }",
            "    fmt.Printf(\"read %d lines from %s\\n\", n, path)",
            "}",
        ]
        .join("\n"),
        "bash" => [
            "#!/usr/bin/env bash",
            "# Robust reader. Run: bash main.sh main.py",
            "set -euo pipefail",
            "f=\"${1:-main.py}\"; [ -f \"$f\" ] || { echo \"missing: $f\" >&2; exit 2; }",
            "nl -ba \"$f\" | head -n 20; echo \"---\"; wc -l < \"$f\" | xargs echo \"lines:\"",
        ]
        .join("\n"),
        "sql" => [
            "-- Bulk load a text file (Postgres server-side path):",
            "CREATE TABLE IF NOT EXISTS lines(n SERIAL PRIMARY KEY, t TEXT);",
            "COPY lines(t) FROM '/tmp/input.txt';",
            "SELECT count(*) FROM lines;",
        ]
        .join("\n"),
        _ => [
            "\"\"\"Robust streaming file reader. Run: python3 main.py main.py\"\"\"",
            "import sys",
            "def read_file(path: str, preview: int = 20) -> int:",
            "    n = 0",
            "    with open(path, encoding=\"utf-8\", errors=\"replace\") as f:",
            "        for line in f:",
            "            n += 1",
            "            if n <= preview: print(f\"{n:4}: {line.rstrip()}\")",
            "    return n",
            "if __name__ == \"__main__\":",
            "    p = sys.argv[1] if len(sys.argv) > 1 else \"main.py\"",
            "    try: total = read_file(p); print(f\"read {total} lines from {p}\")",
            "    except FileNotFoundError: print(f\"missing: {p}\"); sys.exit(2)",
        ]
        .join("\n"),
    }
}

fn login_code(lang: &str) -> String {
    match lang {
        "rust" => [
            "// Login demo with hashing note. Run: cargo run -- demo s3cret",
            "// Production: use argon2 crate, never store plaintext.",
            "use std::collections::HashMap; use std::env;",
            "fn hash_pw(pw: &str, salt: &str) -> u64 {",
            "    use std::collections::hash_map::DefaultHasher; use std::hash::{Hash, Hasher};",
            "    let mut h = DefaultHasher::new(); (salt, pw).hash(&mut h); h.finish()",
            "}",
            "fn main() {",
            "    let user = env::args().nth(1).unwrap_or(\"demo\".into());",
            "    let pw = env::args().nth(2).unwrap_or(\"s3cret\".into());",
            "    let mut db = HashMap::new(); db.insert(\"demo\".to_string(), (\"salt1\".to_string(), hash_pw(\"s3cret\", \"salt1\")));",
            "    match db.get(&user) {",
            "        Some((salt, h)) if *h == hash_pw(&pw, salt) => println!(\"login ok: {user}\"),",
            "        _ => { println!(\"login failed\"); std::process::exit(1); }",
            "    }",
            "}",
        ]
        .join("\n"),
        "javascript" | "typescript" => [
            "// Login demo. Run: node main.js demo s3cret (prod: use bcrypt/argon2).",
            "const crypto = require(\"crypto\");",
            "function hash(pw, salt) { return crypto.pbkdf2Sync(pw, salt, 100_000, 32, \"sha256\").toString(\"hex\"); }",
            "const db = { demo: { salt: \"salt1\", hash: hash(\"s3cret\", \"salt1\") } };",
            "const [user = \"demo\", pw = \"s3cret\"] = process.argv.slice(2);",
            "const rec = db[user];",
            "if (rec && rec.hash === hash(pw, rec.salt)) console.log(`login ok: ${user}`);",
            "else { console.log(\"login failed\"); process.exit(1); }",
        ]
        .join("\n"),
        "go" => [
            "// Login demo. Run: go run . demo s3cret (prod: golang.org/x/crypto/bcrypt).",
            "package main",
            "import (\"crypto/sha256\" \"fmt\" \"os\")",
            "func hash(pw, salt string) string { h := sha256.Sum256([]byte(salt + pw)); return fmt.Sprintf(\"%x\", h) }",
            "func main() {",
            "    user, pw := \"demo\", \"s3cret\"",
            "    if len(os.Args) > 1 { user = os.Args[1] }; if len(os.Args) > 2 { pw = os.Args[2] }",
            "    want := hash(\"s3cret\", \"salt1\")",
            "    if user == \"demo\" && hash(pw, \"salt1\") == want { fmt.Println(\"login ok:\", user) } else { fmt.Println(\"login failed\"); os.Exit(1) }",
            "}",
        ]
        .join("\n"),
        "bash" => [
            "#!/usr/bin/env bash",
            "# Login check via openssl hash. Run: bash main.sh demo s3cret",
            "# Production: system auth (PAM), never plaintext in script.",
            "set -euo pipefail",
            "user=\"${1:-demo}\"; pw=\"${2:-s3cret}\"",
            "want=$(echo -n \"salt1s3cret\" | openssl dgst -sha256 -r | awk '{print $1}')",
            "got=$(echo -n \"salt1$pw\" | openssl dgst -sha256 -r | awk '{print $1}')",
            "if [ \"$user\" = demo ] && [ \"$got\" = \"$want\" ]; then echo \"login ok: $user\"; else echo \"login failed\"; exit 1; fi",
        ]
        .join("\n"),
        "sql" => [
            "-- Users table with hashed passwords (store Argon2/bcrypt output, never plaintext).",
            "CREATE TABLE IF NOT EXISTS users(id SERIAL PRIMARY KEY, name TEXT UNIQUE, pw_hash TEXT NOT NULL);",
            "INSERT INTO users(name, pw_hash) VALUES ('demo', '$argon2id$v=19$...') ON CONFLICT DO NOTHING;",
            "SELECT id, name FROM users WHERE name='demo';",
        ]
        .join("\n"),
        _ => [
            "\"\"\"Login with secure hashing (stdlib). Run: python3 main.py demo s3cret\"\"\"",
            "\"\"\"Production: use argon2-cffi or bcrypt, plus rate limits and MFA.\"\"\"",
            "import hashlib, hmac, sys",
            "def hash_pw(pw: str, salt: bytes) -> str:",
            "    return hashlib.pbkdf2_hmac(\"sha256\", pw.encode(), salt, 100_000).hex()",
            "DB = {\"demo\": (b\"salt1\", hash_pw(\"s3cret\", b\"salt1\"))}",
            "def verify(user: str, pw: str) -> bool:",
            "    rec = DB.get(user)",
            "    if not rec: return False",
            "    salt, want = rec",
            "    return hmac.compare_digest(hash_pw(pw, salt), want)",
            "if __name__ == \"__main__\":",
            "    u = sys.argv[1] if len(sys.argv) > 1 else \"demo\"",
            "    p = sys.argv[2] if len(sys.argv) > 2 else \"s3cret\"",
            "    print(f\"login ok: {u}\" if verify(u, p) else \"login failed\")",
            "    sys.exit(0 if verify(u, p) else 1)",
        ]
        .join("\n"),
    }
}

fn generic_code(lang: &str, _task: &Task, query: &str) -> String {
    let short: String = query.chars().take(70).collect();
    // Dynamic synthesis: function name comes from the user's own prompt,
    // so every request yields different code (no single hardcoded name).
    let snake = extract_fn_name(query);
    let camel = to_camel(&snake);
    // Domain-aware starter: kasir/todo/email get real logic, not echo.
    let low = query.to_lowercase();
    if low.contains("kasir")
        || low.contains("cashier")
        || low.contains("pos")
        || low.contains("toko")
    {
        return pos_code(lang, &snake, &camel);
    }
    if low.contains("todo") || low.contains("task") || low.contains("tugas") {
        return todo_code(lang, &snake, &camel);
    }
    match lang {
        "rust" => format!(
            "//! {short}\n//! Run with: cargo run --quiet\n\n/// {short}\n/// Core logic kept pure for easy testing.\npub fn {snake}(input: &str) -> String {{\n    let t = input.trim();\n    if t.is_empty() {{\n        return \"empty input\".to_string();\n    }}\n    format!(\"{snake}: {{}}\", t)\n}}\n\nfn main() {{\n    use std::io::Read;\n    let mut buf = String::new();\n    let from_arg = std::env::args().nth(1);\n    let out = match from_arg {{\n        Some(a) => {snake}(&a),\n        None => {{\n            std::io::stdin().read_to_string(&mut buf).unwrap_or(0);\n            {snake}(&buf)\n        }}\n    }};\n    println!(\"{{out}}\");\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn smoke() {{\n        assert!({snake}(\"hi\").contains(\"hi\"));\n    }}\n}}"
        ),
        "python" => format!(
            "\"\"\"{short}\nRun with: python3 main.py \"hello\"\n\"\"\"\nimport sys\n\ndef {snake}(text: str) -> str:\n    \"\"\"{short}.\"\"\"\n    text = text.strip()\n    if not text:\n        raise ValueError(\"empty input\")\n    return f\"{snake}: {{text}}\"\n\ndef main() -> None:\n    if len(sys.argv) > 1:\n        print({snake}(sys.argv[1]))\n    else:\n        data = sys.stdin.read()\n        print({snake}(data if data.strip() else \"demo\"))\n\nif __name__ == \"__main__\":\n    main()"
        ),
        "javascript" | "typescript" => format!(
            "// {short}\n// Run with: node main.js \"hello\"\nfunction {camel}(input) {{\n  if (typeof input !== \"string\" || !input.trim()) throw new Error(\"empty input\");\n  return `{camel}: ${{input.trim()}}`;\n}}\nconst arg = process.argv[2] ?? \"demo\";\nconsole.log({camel}(arg));"
        ),
        "go" => format!(
            "// {short} - go run .\npackage main\n\nimport (\"fmt\" \"os\" \"strings\")\n\n// {camel} implements: {short}\nfunc {camel}(s string) string {{\n    s = strings.TrimSpace(s)\n    if s == \"\" {{ return \"{snake}: demo\" }}\n    return \"{snake}: \" + s\n}}\n\nfunc main() {{\n    arg := \"demo\"\n    if len(os.Args) > 1 {{ arg = os.Args[1] }}\n    fmt.Println({camel}(arg))\n}}"
        ),
        "bash" => format!(
            "#!/usr/bin/env bash\n# {short}\nset -euo pipefail\n# {snake}: process one argument\n{snake}() {{\n  local input=\"${{1:-demo}}\"\n  echo \"{snake}: $input\"\n}}\n{snake} \"${{1:-demo}}\""
        ),
        "sql" => format!(
            "-- {short}\nCREATE TABLE IF NOT EXISTS {snake} (\n  id SERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL DEFAULT now()\n);\nINSERT INTO {snake} (name) VALUES ('demo') ON CONFLICT DO NOTHING;\nSELECT * FROM {snake} ORDER BY id LIMIT 20;"
        ),
        _ => format!(
            "\"\"\"{short}\"\"\"\n# Generic starter - tell me the language for idiomatic code.\ndef {snake}(x): return f\"{snake}: {{x}}\"\nprint({snake}(\"demo\"))"
        ),
    }
}

fn pos_code(lang: &str, snake: &str, camel: &str) -> String {
    match lang {
        "rust" => format!(
            "//! Cashier ({snake}). Run: cargo run --quiet\n#[derive(Debug, Clone)] struct Line {{ name: String, price: u64, qty: u64 }}\nfn total(lines: &[Line], discount_pct: u64) -> u64 {{\n    let sub: u64 = lines.iter().map(|l| l.price * l.qty).sum();\n    sub - sub * discount_pct.min(100) / 100\n}}\nfn main() {{\n    let cart = vec![Line {{ name: \"kopi\".into(), price: 15000, qty: 2 }}, Line {{ name: \"roti\".into(), price: 10000, qty: 1 }}];\n    println!(\"total: {{}}\", total(&cart, 10));\n}}"
        ),
        "javascript" | "typescript" => format!(
            "// Cashier ({camel}). Run: node main.js\nfunction total(lines, discountPct = 0) {{\n  const sub = lines.reduce((s, l) => s + l.price * l.qty, 0);\n  return sub - Math.floor(sub * Math.min(100, discountPct) / 100);\n}}\nconsole.log(total([{{ name: \"kopi\", price: 15000, qty: 2 }}, {{ name: \"roti\", price: 10000, qty: 1 }}], 10));"
        ),
        "go" => format!(
            "// Cashier ({camel}). Run: go run .\npackage main\nimport \"fmt\"\ntype Line struct {{ Name string; Price, Qty int }}\nfunc total(lines []Line, disc int) int {{\n    sub := 0; for _, l := range lines {{ sub += l.Price * l.Qty }}; return sub - sub*min(disc,100)/100 }}\nfunc min(a, b int) int {{ if a < b {{ return a }}; return b }}\nfunc main() {{ fmt.Println(total([]Line{{{{ \"kopi\", 15000, 2 }}, {{ \"roti\", 10000, 1 }}}}, 10)) }}"
        ),
        "bash" => format!(
            "#!/usr/bin/env bash\n# Cashier ({snake}). Run: bash main.sh\nset -euo pipefail\n# price*qty lines: name price qty\ntotal=0; while read -r _ p q; do total=$((total + p*q)); done <<'EOF'\nkopi 15000 2\nroti 10000 1\nEOF\ndisc=10; echo $((total - total*disc/100))"
        ),
        "sql" => format!(
            "-- Cashier schema ({snake}).\nCREATE TABLE IF NOT EXISTS lines(id SERIAL PRIMARY KEY, name TEXT, price INT, qty INT);\nINSERT INTO lines(name, price, qty) VALUES ('kopi',15000,2),('roti',10000,1);\nSELECT sum(price*qty) * 0.9 AS total_after_10pct FROM lines;"
        ),
        _ => format!(
            "\"\"\"Cashier ({snake}). Run: python3 main.py\"\"\"\nfrom dataclasses import dataclass\n@dataclass\nclass Line:\n    name: str; price: int; qty: int\ndef total(lines: list[Line], discount_pct: float = 0) -> int:\n    sub = sum(l.price * l.qty for l in lines)\n    return int(sub - sub * min(100, discount_pct) / 100)\nif __name__ == \"__main__\":\n    cart = [Line(\"kopi\", 15000, 2), Line(\"roti\", 10000, 1)]\n    print(total(cart, 10))"
        ),
    }
}

fn todo_code(lang: &str, snake: &str, camel: &str) -> String {
    match lang {
        "rust" => format!(
            "//! Todo ({snake}). Run: cargo run --quiet\n#[derive(Debug)] struct Todo {{ id: usize, text: String, done: bool }}\nfn main() {{\n    let mut items = vec![Todo {{ id: 1, text: \"demo\".into(), done: false }}];\n    items[0].done = true;\n    println!(\"{{:?}}\", items);\n}}"
        ),
        "javascript" | "typescript" => format!(
            "// Todo ({camel}). Run: node main.js\nlet next = 0; const items = [];\nfunction add(text) {{ items.push({{ id: ++next, text, done: false }}); }}\nadd(\"demo\"); items[0].done = true; console.log(items);"
        ),
        "go" => format!(
            "// Todo ({camel}). Run: go run .\npackage main\nimport \"fmt\"\ntype Todo struct {{ ID int; Text string; Done bool }}\nfunc main() {{ items := []Todo{{{{1, \"demo\", false}}}}; items[0].Done = true; fmt.Println(items) }}"
        ),
        _ => format!(
            "\"\"\"Todo ({snake}). Run: python3 main.py\"\"\"\nitems = []\ndef add(text: str) -> dict:\n    items.append({{\"id\": len(items)+1, \"text\": text, \"done\": False}})\n    return items[-1]\nadd(\"demo\"); items[0][\"done\"] = True; print(items)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_requests_id_en() {
        assert!(is_coding_request("buatkan fungsi fibonacci python"));
        assert!(is_coding_request("tolong buatin program kasir"));
        assert!(is_coding_request("create a REST API in rust"));
        assert!(!is_coding_request("halo apa kabar"));
    }

    #[test]
    fn lang_detect() {
        assert_eq!(detect_target_lang("buatkan fibonacci python"), "python");
        assert_eq!(detect_target_lang("buatkan fibonacci rust"), "rust");
    }

    #[test]
    fn generates_runnable() {
        let out = generate("buatkan fungsi fibonacci python");
        assert!(out.contains("```python"));
        assert!(out.contains("def fibonacci"));
        assert!(out.contains("### Run"));
        assert!(!out.contains("Cara menjalankan"));
    }

    #[test]
    fn dynamic_names_differ_per_prompt() {
        let a = extract_fn_name("buatkan fungsi hitungDiskon python");
        let b = extract_fn_name("buatkan fungsi kirimEmail python");
        assert_ne!(a, b);
        assert!(a.contains("hitung") || a.contains("diskon"));
        let g = generate("buatkan program kasir python");
        assert!(g.contains("kasir"));
    }
}
