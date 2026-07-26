//! Dependency-free syntax tokenizer for transcript code blocks and tool
//! output. Provider text is escaped as it is emitted, so highlighted output can
//! never introduce markup of its own.

/// Highlighting is a readability aid, not a viewer. Past this size the cost of
/// tokenizing on the UI thread outweighs the benefit, so callers fall back to
/// plain text.
const MAX_HIGHLIGHT_CHARS: usize = 60_000;
/// Language detection only inspects the head of a block.
const DETECT_SAMPLE_CHARS: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Kw,
    Str,
    Num,
    Com,
    Fn,
    Prop,
    Op,
    Add,
    Del,
    Meta,
}

impl TokenKind {
    const fn class(self) -> &'static str {
        match self {
            Self::Kw => "syn-kw",
            Self::Str => "syn-str",
            Self::Num => "syn-num",
            Self::Com => "syn-com",
            Self::Fn => "syn-fn",
            Self::Prop => "syn-prop",
            Self::Op => "syn-op",
            Self::Add => "syn-add",
            Self::Del => "syn-del",
            Self::Meta => "syn-meta",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Bash,
    Json,
    Diff,
    CLike,
}

impl Language {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Json => "json",
            Self::Diff => "diff",
            Self::CLike => "clike",
        }
    }
}

const CLIKE_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "of",
    "private",
    "protected",
    "public",
    "return",
    "satisfies",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "type",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "fn",
    "impl",
    "mut",
    "match",
    "loop",
    "mod",
    "move",
    "pub",
    "ref",
    "struct",
    "trait",
    "unsafe",
    "use",
    "where",
    "crate",
    "dyn",
    "self",
    "Self",
    "def",
    "elif",
    "except",
    "global",
    "lambda",
    "nonlocal",
    "not",
    "or",
    "pass",
    "raise",
    "and",
    "is",
    "None",
    "True",
    "False",
    "null",
    "undefined",
    "true",
    "false",
];

const BASH_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "function", "in", "return", "exit", "export", "local", "set", "unset", "source", "alias", "cd",
    "echo", "printf", "read", "shift", "trap",
];

/// Commands common enough at the start of a line to identify shell output.
const BASH_COMMANDS: &[&str] = &[
    "sudo", "git", "npm", "npx", "cargo", "python3", "python", "node", "cd", "ls", "mkdir", "curl",
    "brew", "rm", "cp", "mv", "grep", "cat", "echo",
];

const DIFF_PREFIXES: &[&str] = &["diff ", "index ", "--- ", "+++ ", "@@"];

fn alias(name: &str) -> Option<Language> {
    Some(match name {
        "bash" | "sh" | "shell" | "zsh" | "nu" | "nushell" | "console" | "fish" | "powershell"
        | "ps1" => Language::Bash,
        "json" | "json5" | "jsonc" => Language::Json,
        "diff" | "patch" => Language::Diff,
        "clike" | "js" | "jsx" | "ts" | "tsx" | "javascript" | "typescript" | "rust" | "rs"
        | "go" | "c" | "cpp" | "java" | "kotlin" | "swift" | "python" | "py" | "ruby" | "php"
        | "css" | "scss" | "yaml" | "yml" | "toml" | "sql" => Language::CLike,
        _ => return None,
    })
}

fn is_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '$'
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_' || character == '$'
}

/// Resolves a fence hint, or inspects the content when no usable hint exists.
pub fn guess_language(code: &str, hint: Option<&str>) -> Option<Language> {
    let named = hint.map(|hint| hint.trim().to_ascii_lowercase());
    let named = named.filter(|named| !named.is_empty());
    if let Some(named) = named.as_deref()
        && let Some(language) = alias(named)
    {
        return Some(language);
    }

    let sample = code
        .char_indices()
        .nth(DETECT_SAMPLE_CHARS)
        .map_or(code, |(index, _)| &code[..index]);

    if sample.lines().any(|line| {
        line.starts_with("diff --git")
            || line.starts_with("@@ -")
            || (line.starts_with("+++ ") || line.starts_with("--- "))
    }) {
        return Some(Language::Diff);
    }

    let trimmed = sample.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if serde_json::from_str::<serde_json::Value>(code).is_ok() {
            return Some(Language::Json);
        }
        if looks_like_json_object(trimmed) {
            return Some(Language::Json);
        }
    }

    if sample.lines().any(is_shell_line) {
        return Some(Language::Bash);
    }

    named.map(|_| Language::CLike)
}

/// True when a line carries a `"key":` pair, the cheapest signal that partial
/// or truncated output is still JSON.
fn looks_like_json_object(sample: &str) -> bool {
    let characters = sample.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '"' {
            index += 1;
            continue;
        }
        let Some(length) = match_quoted(&characters, index, '"', false, true) else {
            index += 1;
            continue;
        };
        let mut after = index + length;
        while characters.get(after).is_some_and(|item| *item == ' ') {
            after += 1;
        }
        if characters.get(after) == Some(&':') {
            return true;
        }
        index += length.max(1);
    }
    false
}

fn is_shell_line(line: &str) -> bool {
    if line.starts_with("$ ") || line.starts_with("#!/") {
        return true;
    }
    BASH_COMMANDS
        .iter()
        .any(|command| line.starts_with(command) && line[command.len()..].starts_with(' '))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    /// Length in `char`s, not bytes.
    length: usize,
}

const fn token(kind: TokenKind, length: usize) -> Option<Token> {
    if length == 0 {
        None
    } else {
        Some(Token { kind, length })
    }
}

fn match_line(source: &[char], index: usize) -> usize {
    let mut length = 0;
    while let Some(character) = source.get(index + length)
        && *character != '\n'
    {
        length += 1;
    }
    length
}

/// Matches a quoted run. Unterminated quotes still highlight to the end of the
/// line (or block) so streaming output does not flicker between styles.
fn match_quoted(
    source: &[char],
    index: usize,
    quote: char,
    stop_at_newline: bool,
    escapes: bool,
) -> Option<usize> {
    if source.get(index) != Some(&quote) {
        return None;
    }
    let mut cursor = index + 1;
    while let Some(character) = source.get(cursor) {
        match character {
            '\\' if escapes => cursor += 2,
            '\n' if stop_at_newline => return Some(cursor - index),
            character if *character == quote => return Some(cursor + 1 - index),
            _ => cursor += 1,
        }
    }
    Some(source.len() - index)
}

fn match_identifier(source: &[char], index: usize) -> usize {
    if !source.get(index).copied().is_some_and(is_identifier_start) {
        return 0;
    }
    let mut length = 1;
    while source.get(index + length).copied().is_some_and(is_word) {
        length += 1;
    }
    length
}

fn match_keyword(source: &[char], index: usize, keywords: &[&str]) -> Option<usize> {
    let length = match_identifier(source, index);
    if length == 0 {
        return None;
    }
    let word = source[index..index + length].iter().collect::<String>();
    keywords.contains(&word.as_str()).then_some(length)
}

fn match_number(source: &[char], index: usize) -> Option<usize> {
    let mut length = 0;
    if source.get(index) == Some(&'0')
        && source
            .get(index + 1)
            .is_some_and(|item| matches!(item, 'x' | 'X' | 'b' | 'B' | 'o' | 'O'))
    {
        length = 2;
        while source
            .get(index + length)
            .is_some_and(|item| item.is_ascii_hexdigit() || *item == '_')
        {
            length += 1;
        }
        return (length > 2).then_some(length);
    }
    while source
        .get(index + length)
        .is_some_and(|item| item.is_ascii_digit() || (length > 0 && *item == '_'))
    {
        length += 1;
    }
    if length == 0 {
        return None;
    }
    if source.get(index + length) == Some(&'.')
        && source
            .get(index + length + 1)
            .is_some_and(char::is_ascii_digit)
    {
        length += 1;
        while source
            .get(index + length)
            .is_some_and(|item| item.is_ascii_digit() || *item == '_')
        {
            length += 1;
        }
    }
    if source
        .get(index + length)
        .is_some_and(|item| matches!(item, 'e' | 'E'))
    {
        let mut exponent = length + 1;
        if source
            .get(index + exponent)
            .is_some_and(|item| matches!(item, '+' | '-'))
        {
            exponent += 1;
        }
        if source
            .get(index + exponent)
            .is_some_and(char::is_ascii_digit)
        {
            length = exponent;
            while source.get(index + length).is_some_and(char::is_ascii_digit) {
                length += 1;
            }
        }
    }
    Some(length)
}

/// Matches an identifier that is followed by `terminator`, which is how call
/// sites and object keys are told apart from plain words.
fn match_identifier_before(source: &[char], index: usize, terminator: char) -> Option<usize> {
    let length = match_identifier(source, index);
    if length == 0 {
        return None;
    }
    let mut after = index + length;
    while source
        .get(after)
        .is_some_and(|item| *item == ' ' || *item == '\t')
    {
        after += 1;
    }
    (source.get(after) == Some(&terminator)).then_some(length)
}

fn match_run(source: &[char], index: usize, allowed: &str) -> usize {
    let mut length = 0;
    while source
        .get(index + length)
        .is_some_and(|item| allowed.contains(*item))
    {
        length += 1;
    }
    length
}

fn match_clike(source: &[char], index: usize, boundary: bool) -> Option<Token> {
    let current = *source.get(index)?;
    if current == '/' && source.get(index + 1) == Some(&'/') {
        return token(TokenKind::Com, match_line(source, index));
    }
    if current == '#' {
        return token(TokenKind::Com, match_line(source, index));
    }
    if current == '/' && source.get(index + 1) == Some(&'*') {
        let mut cursor = index + 2;
        while cursor < source.len() {
            if source[cursor] == '*' && source.get(cursor + 1) == Some(&'/') {
                return token(TokenKind::Com, cursor + 2 - index);
            }
            cursor += 1;
        }
        return token(TokenKind::Com, source.len() - index);
    }
    if current == '`' {
        return match_quoted(source, index, '`', false, true)
            .and_then(|l| token(TokenKind::Str, l));
    }
    if current == '"' || current == '\'' {
        return match_quoted(source, index, current, true, true)
            .and_then(|l| token(TokenKind::Str, l));
    }
    if boundary {
        if let Some(length) = match_keyword(source, index, CLIKE_KEYWORDS) {
            return token(TokenKind::Kw, length);
        }
        if let Some(length) = match_number(source, index) {
            return token(TokenKind::Num, length);
        }
        if let Some(length) = match_identifier_before(source, index, '(') {
            return token(TokenKind::Fn, length);
        }
        if let Some(length) = match_identifier_before(source, index, ':') {
            return token(TokenKind::Prop, length);
        }
    }
    if current == '=' && source.get(index + 1) == Some(&'>') {
        return token(TokenKind::Op, 2);
    }
    if current == ':' && source.get(index + 1) == Some(&':') {
        return token(TokenKind::Op, 2);
    }
    token(TokenKind::Op, match_run(source, index, "+-*/%=!<>&|^~?"))
}

fn match_bash(source: &[char], index: usize, boundary: bool, line_start: bool) -> Option<Token> {
    let current = *source.get(index)?;
    if current == '#' {
        return token(TokenKind::Com, match_line(source, index));
    }
    if current == '"' {
        return match_quoted(source, index, '"', false, true)
            .and_then(|l| token(TokenKind::Str, l));
    }
    if current == '\'' {
        return match_quoted(source, index, '\'', false, false)
            .and_then(|l| token(TokenKind::Str, l));
    }
    if boundary && let Some(length) = match_keyword(source, index, BASH_KEYWORDS) {
        return token(TokenKind::Kw, length);
    }
    if line_start {
        // A leading "$ " is a prompt, not a variable.
        let indent = match_run(source, index, " \t");
        if source.get(index + indent) == Some(&'$')
            && source
                .get(index + indent + 1)
                .is_some_and(|item| item.is_whitespace())
        {
            return token(TokenKind::Meta, indent + 1);
        }
    }
    if current == '-' {
        let dashes = match_run(source, index, "-").min(2);
        let mut length = dashes;
        while source
            .get(index + length)
            .is_some_and(|item| is_word(*item) || *item == '-')
        {
            length += 1;
        }
        if length > dashes {
            return token(TokenKind::Prop, length);
        }
    }
    if boundary && let Some(length) = match_number(source, index) {
        return token(TokenKind::Num, length);
    }
    if (current == '&' && source.get(index + 1) == Some(&'&'))
        || (current == '|' && source.get(index + 1) == Some(&'|'))
    {
        return token(TokenKind::Op, 2);
    }
    if matches!(current, '|' | '&' | ';' | '<' | '>' | '(' | ')') {
        return token(TokenKind::Op, 1);
    }
    if boundary && current == '$' {
        let braced = source.get(index + 1) == Some(&'{');
        let mut length = 1 + usize::from(braced);
        while source
            .get(index + length)
            .is_some_and(|item| is_word(*item) || matches!(item, '@' | '#' | '?' | '*'))
        {
            length += 1;
        }
        if length > 1 + usize::from(braced) {
            if braced && source.get(index + length) == Some(&'}') {
                length += 1;
            }
            return token(TokenKind::Fn, length);
        }
    }
    None
}

fn match_json(source: &[char], index: usize, boundary: bool) -> Option<Token> {
    let current = *source.get(index)?;
    if current == '"' {
        let length = match_quoted(source, index, '"', false, true)?;
        let mut after = index + length;
        while source
            .get(after)
            .is_some_and(|item| *item == ' ' || *item == '\t')
        {
            after += 1;
        }
        let kind = if source.get(after) == Some(&':') {
            TokenKind::Prop
        } else {
            TokenKind::Str
        };
        return token(kind, length);
    }
    if boundary {
        let negative = usize::from(current == '-');
        if let Some(length) = match_number(source, index + negative) {
            return token(TokenKind::Num, negative + length);
        }
        if let Some(length) = match_keyword(source, index, &["true", "false", "null"]) {
            return token(TokenKind::Kw, length);
        }
    }
    if matches!(current, '{' | '}' | '[' | ']' | ':' | ',') {
        return token(TokenKind::Op, 1);
    }
    None
}

fn match_diff(source: &[char], index: usize, line_start: bool) -> Option<Token> {
    if !line_start {
        return None;
    }
    let length = match_line(source, index);
    let line = source[index..index + length].iter().collect::<String>();
    if DIFF_PREFIXES.iter().any(|prefix| line.starts_with(prefix)) {
        return token(TokenKind::Meta, length);
    }
    if line.starts_with('+') {
        return token(TokenKind::Add, length);
    }
    if line.starts_with('-') {
        return token(TokenKind::Del, length);
    }
    None
}

/// Tokenizes `code` for `language`. Plain runs between tokens are reported with
/// a `None` kind so callers can emit them verbatim.
fn tokenize(code: &str, language: Language) -> Vec<(Option<TokenKind>, String)> {
    let source = code.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut plain_start = 0;
    let mut index = 0;

    while index < source.len() {
        let boundary = index == 0 || !is_word(source[index - 1]);
        let line_start = index == 0 || source[index - 1] == '\n';
        let matched = match language {
            Language::Bash => match_bash(&source, index, boundary, line_start),
            Language::Json => match_json(&source, index, boundary),
            Language::Diff => match_diff(&source, index, line_start),
            Language::CLike => match_clike(&source, index, boundary),
        };
        let Some(matched) = matched else {
            index += 1;
            continue;
        };
        if index > plain_start {
            tokens.push((None, source[plain_start..index].iter().collect()));
        }
        let end = (index + matched.length).min(source.len());
        tokens.push((Some(matched.kind), source[index..end].iter().collect()));
        index = end;
        plain_start = index;
    }
    if plain_start < source.len() {
        tokens.push((None, source[plain_start..].iter().collect()));
    }
    tokens
}

pub fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Renders `code` as escaped HTML with `syn-*` spans, or `None` when the
/// language is unknown or the block is too large to be worth tokenizing.
pub fn highlight_html(code: &str, hint: Option<&str>) -> Option<String> {
    if code.chars().count() > MAX_HIGHLIGHT_CHARS {
        return None;
    }
    let language = guess_language(code, hint)?;
    let mut output = String::with_capacity(code.len() + code.len() / 4);
    for (kind, text) in tokenize(code, language) {
        match kind {
            Some(kind) => {
                output.push_str("<span class=\"");
                output.push_str(kind.class());
                output.push_str("\">");
                output.push_str(&escape_html(&text));
                output.push_str("</span>");
            }
            None => output.push_str(&escape_html(&text)),
        }
    }
    Some(output)
}

/// Renders a `<pre><code>` block, highlighted when the language is recognized
/// and escaped plain text otherwise.
pub fn code_block_html(code: &str, hint: Option<&str>) -> String {
    let language = guess_language(code, hint);
    let class = language.map_or_else(
        || "zai-code".to_owned(),
        |language| format!("zai-code language-{}", language.as_str()),
    );
    let body = highlight_html(code, hint).unwrap_or_else(|| escape_html(code));
    format!("<pre><code class=\"{class}\">{body}</code></pre>")
}

/// Splits a tool summary such as "Running Bash · cargo test" into its label and
/// the command it summarizes, so the command can be shown the way a terminal
/// would show it.
pub fn split_tool_summary(title: &str) -> (String, Option<String>) {
    match title.split_once(" · ") {
        Some((label, command)) if !command.trim().is_empty() => {
            (format!("{label} · "), Some(command.to_owned()))
        }
        _ => (title.to_owned(), None),
    }
}

/// Shell tools carry their command as raw text, so name the language rather
/// than leaving it to detection. Everything else — JSON arguments, diffs, file
/// contents — is detected from the content itself.
pub fn tool_language_hint(title: &str) -> Option<&'static str> {
    // Match on the tool name only: a path or query in the summary must not
    // decide the language.
    let name = title
        .split(" · ")
        .next()
        .unwrap_or(title)
        .to_ascii_lowercase();
    ["bash", "shell", "terminal", "exec", "command"]
        .iter()
        .any(|marker| name.contains(marker))
        .then_some("bash")
}

#[cfg(test)]
mod tests {
    use super::{
        Language, code_block_html, escape_html, guess_language, highlight_html, split_tool_summary,
        tool_language_hint,
    };

    #[test]
    fn fence_hints_and_aliases_resolve_to_a_language() {
        assert_eq!(guess_language("", Some("zsh")), Some(Language::Bash));
        assert_eq!(guess_language("", Some("TSX")), Some(Language::CLike));
        assert_eq!(guess_language("", Some("patch")), Some(Language::Diff));
        assert_eq!(guess_language("", Some("jsonc")), Some(Language::Json));
    }

    #[test]
    fn content_is_detected_without_a_hint() {
        assert_eq!(
            guess_language("git status --short\n", None),
            Some(Language::Bash),
        );
        assert_eq!(guess_language("{\"ok\": true}", None), Some(Language::Json),);
        assert_eq!(
            guess_language("diff --git a/x b/x\n@@ -1 +1 @@\n", None),
            Some(Language::Diff),
        );
        assert_eq!(guess_language("just some prose", None), None);
    }

    #[test]
    fn shell_commands_highlight_flags_and_strings() {
        let html = highlight_html("cargo test --workspace \"one two\"", Some("bash")).unwrap();
        assert!(html.contains("<span class=\"syn-prop\">--workspace</span>"));
        assert!(html.contains("<span class=\"syn-str\">&quot;one two&quot;</span>"));
    }

    #[test]
    fn keywords_only_match_whole_words() {
        let html = highlight_html("input = information", Some("bash")).unwrap();
        assert!(!html.contains("syn-kw"));
        let html = highlight_html("if true", Some("bash")).unwrap();
        assert!(html.contains("<span class=\"syn-kw\">if</span>"));
    }

    #[test]
    fn json_keys_and_values_are_distinguished() {
        let html = highlight_html("{\"name\": \"onyx\"}", Some("json")).unwrap();
        assert!(html.contains("<span class=\"syn-prop\">&quot;name&quot;</span>"));
        assert!(html.contains("<span class=\"syn-str\">&quot;onyx&quot;</span>"));
    }

    #[test]
    fn diff_lines_are_classified() {
        let html = highlight_html("@@ -1 +1 @@\n-old\n+new\n", Some("diff")).unwrap();
        assert!(html.contains("<span class=\"syn-meta\">@@ -1 +1 @@</span>"));
        assert!(html.contains("<span class=\"syn-del\">-old</span>"));
        assert!(html.contains("<span class=\"syn-add\">+new</span>"));
    }

    #[test]
    fn markup_in_source_is_escaped_everywhere() {
        let html = highlight_html("echo \"<script>alert(1)</script>\"", Some("bash")).unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        let block = code_block_html("<img onerror=x>", None);
        assert!(!block.contains("<img"));
        assert!(block.contains("&lt;img"));
    }

    #[test]
    fn oversized_blocks_fall_back_to_plain_text() {
        let huge = "a\n".repeat(40_000);
        assert!(highlight_html(&huge, Some("bash")).is_none());
        assert!(code_block_html(&huge, Some("bash")).contains("<code class=\"zai-code"));
    }

    #[test]
    fn tokenizing_preserves_the_original_text() {
        let source = "grep -rn 'needle' . | wc -l # count\n";
        let html = highlight_html(source, Some("bash")).unwrap();
        let stripped = html
            .replace("<span class=\"syn-kw\">", "")
            .replace("<span class=\"syn-str\">", "")
            .replace("<span class=\"syn-num\">", "")
            .replace("<span class=\"syn-com\">", "")
            .replace("<span class=\"syn-fn\">", "")
            .replace("<span class=\"syn-prop\">", "")
            .replace("<span class=\"syn-op\">", "")
            .replace("<span class=\"syn-meta\">", "")
            .replace("</span>", "");
        assert_eq!(stripped, escape_html(source));
    }

    #[test]
    fn unterminated_quotes_do_not_hang_or_drop_text() {
        let html = highlight_html("echo \"unfinished", Some("bash")).unwrap();
        assert!(html.contains("&quot;unfinished"));
    }

    #[test]
    fn only_the_tool_name_decides_the_highlight_language() {
        assert_eq!(tool_language_hint("Running Bash · ls -la"), Some("bash"));
        assert_eq!(tool_language_hint("Running Read · shell.rs"), None);
        assert_eq!(tool_language_hint("Running Read"), None);
    }

    #[test]
    fn tool_summaries_split_into_a_label_and_a_command() {
        assert_eq!(
            split_tool_summary("Running Bash · cargo test"),
            ("Running Bash · ".to_owned(), Some("cargo test".to_owned())),
        );
        assert_eq!(
            split_tool_summary("Running Bash"),
            ("Running Bash".to_owned(), None),
        );
    }
}
