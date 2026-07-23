/**
 * Dependency-free syntax tokenizer for transcript code blocks and tool
 * output. Pure data in, data out: rendering happens in the Transcript
 * component, so provider-supplied text can never become markup.
 */

export type TokenType =
  | "kw"
  | "str"
  | "num"
  | "com"
  | "fn"
  | "prop"
  | "op"
  | "add"
  | "del"
  | "meta"

export interface CodeToken {
  /** null carries plain text between highlighted tokens. */
  type: TokenType | null
  text: string
}

interface Rule {
  type: TokenType
  pattern: RegExp
}

const MAX_HIGHLIGHT_CHARS = 60_000

function keywords(words: string) {
  return new RegExp(`(?:${words.split(" ").join("|")})\\b`, "y")
}

const C_LIKE_RULES: Rule[] = [
  { type: "com", pattern: /\/\/[^\n]*|\/\*[^]*?(?:\*\/|$)|#[^\n]*/y },
  { type: "str", pattern: /`(?:\\.|[^`\\])*`?|"(?:\\.|[^"\n\\])*"?|'(?:\\.|[^'\n\\])*'?/y },
  {
    type: "kw",
    pattern: keywords(
      "abstract as async await break case catch class const continue debugger default delete do else enum export extends finally for from function if implements import in instanceof interface let new of private protected public return satisfies static super switch this throw try type typeof var void while with yield fn impl mut match loop mod move pub ref struct trait unsafe use where crate dyn self Self def elif except global lambda nonlocal not or pass raise and is None True False null undefined true false",
    ),
  },
  { type: "num", pattern: /0[xXbBoO][\da-fA-F_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?/y },
  { type: "fn", pattern: /[A-Za-z_$][\w$]*(?=\s*\()/y },
  { type: "prop", pattern: /[A-Za-z_$][\w$]*(?=\s*:)/y },
  { type: "op", pattern: /=>|::|[+\-*/%=!<>&|^~?]+/y },
]

const BASH_RULES: Rule[] = [
  { type: "com", pattern: /#[^\n]*/y },
  { type: "str", pattern: /"(?:\\.|[^"\\])*"?|'[^']*'?/y },
  {
    type: "kw",
    pattern: keywords(
      "if then else elif fi for while until do done case esac function in return exit export local set unset source alias cd echo printf read shift trap",
    ),
  },
  { type: "meta", pattern: /^\s*\$(?=\s)/my },
  { type: "prop", pattern: /--?[\w-]+/y },
  { type: "num", pattern: /\d+(?:\.\d+)?/y },
  { type: "op", pattern: /&&|\|\||[|&;<>()]/y },
  { type: "fn", pattern: /\$\{?[\w@#?*]+\}?/y },
]

const JSON_RULES: Rule[] = [
  { type: "prop", pattern: /"(?:\\.|[^"\\])*"(?=\s*:)/y },
  { type: "str", pattern: /"(?:\\.|[^"\\])*"?/y },
  { type: "num", pattern: /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y },
  { type: "kw", pattern: /(?:true|false|null)\b/y },
  { type: "op", pattern: /[{}[\]:,]/y },
]

const DIFF_RULES: Rule[] = [
  { type: "meta", pattern: /^(?:diff |index |--- |\+\+\+ |@@)[^\n]*/my },
  { type: "add", pattern: /^\+[^\n]*/my },
  { type: "del", pattern: /^-[^\n]*/my },
]

const LANGUAGE_RULES: Record<string, Rule[]> = {
  bash: BASH_RULES,
  json: JSON_RULES,
  diff: DIFF_RULES,
  clike: C_LIKE_RULES,
}

const ALIASES: Record<string, string> = {
  sh: "bash", shell: "bash", zsh: "bash", nu: "bash", nushell: "bash", console: "bash", fish: "bash", powershell: "bash", ps1: "bash",
  json5: "json", jsonc: "json",
  patch: "diff",
  js: "clike", jsx: "clike", ts: "clike", tsx: "clike", javascript: "clike", typescript: "clike",
  rust: "clike", rs: "clike", go: "clike", c: "clike", cpp: "clike", java: "clike", kotlin: "clike",
  swift: "clike", python: "clike", py: "clike", ruby: "clike", php: "clike", css: "clike", scss: "clike",
  yaml: "clike", yml: "clike", toml: "clike", sql: "clike",
}

/** Resolves a fence hint, or inspects the content when no hint exists. */
export function guessLanguage(code: string, hint?: string): string | undefined {
  const named = hint?.trim().toLowerCase()
  if (named) {
    if (LANGUAGE_RULES[named]) return named
    if (ALIASES[named]) return ALIASES[named]
  }
  const sample = code.slice(0, 2_000)
  if (/^diff --git|^--- .+\n\+\+\+ |^@@ -\d/m.test(sample)) return "diff"
  const trimmed = sample.trimStart()
  if (/^[{[]/.test(trimmed)) {
    try {
      JSON.parse(code)
      return "json"
    } catch { /* fall through */ }
    if (/"[^"\n]*"\s*:/.test(trimmed)) return "json"
  }
  if (/^(?:\$ |#!\/|(?:sudo|git|npm|npx|cargo|python3?|node|cd|ls|mkdir|curl|brew|rm|cp|mv|grep|cat|echo) )/m.test(sample)) return "bash"
  if (named) return "clike"
  return undefined
}

/**
 * Tokenizes code for the detected language. Returns null when the language is
 * unknown or the content is too large, so callers can render plain text.
 */
export function tokenizeCode(code: string, hint?: string): CodeToken[] | null {
  if (code.length > MAX_HIGHLIGHT_CHARS) return null
  const language = guessLanguage(code, hint)
  const rules = language ? LANGUAGE_RULES[language] : undefined
  if (!rules) return null

  const tokens: CodeToken[] = []
  let plainStart = 0
  let index = 0

  const flushPlain = (end: number) => {
    if (end > plainStart) tokens.push({ type: null, text: code.slice(plainStart, end) })
  }

  while (index < code.length) {
    let matched = false
    // Only match at token-ish boundaries so identifiers stay whole.
    const previous = index === 0 ? "" : code[index - 1]
    const boundary = !/[\w$]/.test(previous)
    for (const rule of rules) {
      if (!boundary && (rule.type === "kw" || rule.type === "num" || rule.type === "fn" || rule.type === "prop")) continue
      rule.pattern.lastIndex = index
      const match = rule.pattern.exec(code)
      if (!match || match.index !== index || !match[0]) continue
      flushPlain(index)
      tokens.push({ type: rule.type, text: match[0] })
      index += match[0].length
      plainStart = index
      matched = true
      break
    }
    if (!matched) index += 1
  }
  flushPlain(code.length)
  return tokens
}
