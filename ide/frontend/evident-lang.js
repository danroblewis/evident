/**
 * Evident language support for Monaco Editor.
 *
 * Adapted from the highlight.js grammar in ~/md/static/index.html.
 * Uses Monaco's Monarch tokenizer format.
 */

'use strict';

const EVIDENT_LANGUAGE_ID = 'evident';

/**
 * Monarch tokenizer definition.
 * Rules are tested top-to-bottom; first match wins.
 */
const EVIDENT_TOKENS = {
    // ── Token classifications ────────────────────────────────────────────
    keywords: [
        'schema', 'claim', 'type', 'assert', 'evident', 'when',
        'with', 'as', 'then', 'else', 'find', 'minimizing', 'where',
        'not', 'and', 'or', 'true', 'false', 'otherwise',
    ],

    quantifiers: ['all', 'some', 'none', 'one', 'forall', 'exists'],

    wordOperators: ['in', 'not_in', 'subset', 'superset', 'mapsto', 'bowtie', 'starts_with'],

    typeBuiltins: [
        'Prop', 'det', 'semidet', 'nondet',
        'Nat', 'Int', 'Real', 'Bool', 'String',
        'List', 'Maybe', 'Set', 'Type', 'Arrow', 'ForAll',
    ],

    // ── Tokenizer ────────────────────────────────────────────────────────
    tokenizer: {
        root: [
            // Whitespace
            [/\s+/, ''],

            // Line comments:  -- ...
            [/--.*$/, 'comment'],

            // Strings
            [/"([^"\\]|\\.)*"/, 'string'],

            // ── Claim/evident head: highlight keyword + following identifier ──
            // "claim foo_bar" → keyword + title.function
            [/\b(claim|evident)\b/, { token: 'keyword', next: '@claimName' }],

            // type declaration keyword (distinct colour from other keywords)
            [/\btype\b/, 'keyword.type'],

            // Core keywords
            [/\b(schema|assert|when|with|as|then|else|find|minimizing|where|otherwise)\b/, 'keyword'],
            [/\b(not|and|or|true|false)\b/, 'keyword.operator'],

            // Quantifier keywords
            [/\b(all|some|none|one|forall|exists)\b/, 'keyword.control'],

            // Word operators (in, subset, etc.)
            [/\b(in|not_in|subset|superset|mapsto|bowtie|starts_with)\b/, 'operator.word'],

            // Built-in type names
            [/\b(Prop|det|semidet|nondet|Nat|Int|Real|Bool|String|List|Maybe|Set|Type|Arrow|ForAll)\b/, 'type.builtin'],

            // Any other CapitalisedIdentifier → type constructor
            [/\b[A-Z][A-Za-z0-9_]*\b/, 'type.identifier'],

            // Numbers (integer or decimal)
            [/\b\d+(\.\d+)?\b/, 'number'],

            // Unicode quantifiers — order matters: ∃! and ¬∃ before ∃ and ¬
            [/∃!|¬∃/, 'keyword.control'],
            [/[∀∃¬∄]/, 'keyword.control'],

            // Unicode membership / relational operators
            [/[∈∉⊆⊇]/, 'operator'],

            // Unicode set operators
            [/[∩∪×\\]/, 'operator'],

            // Unicode logical connectives
            [/[∧∨]/, 'operator'],

            // Unicode comparison operators
            [/[≤≥≠]/, 'operator'],

            // Unicode arrows / composition
            [/⇒|↦|·|⋈/, 'operator'],

            // ASCII implication arrow (=> before =)
            [/=>|->/, 'operator'],

            // ASCII comparison / assignment operators (multi-char before single)
            [/<=|>=|!=|==/, 'operator'],
            [/\.\./, 'operator'],

            // Query variable: ?Ident  (e.g. ?X, ?result)
            [/\?[A-Za-z_][A-Za-z0-9_]*/, 'variable.query'],

            // Standalone ? (query line opener)
            [/\?/, 'keyword.control'],

            // Internal variables: _prefix
            [/_[a-zA-Z_][a-zA-Z0-9_]*/, 'variable'],

            // Field access: identifier after a dot
            [/(?<=\.)[a-z_][A-Za-z0-9_]*/, 'attribute'],

            // Field name before ':' or '='
            [/[a-z_][A-Za-z0-9_]*(?=\s*[:=])/, 'attribute'],

            // Body predicate: indented lowercase identifier at start of indented line
            // (handled by the fallback identifier rule with context)

            // Regular lowercase identifiers
            [/[a-z_][A-Za-z0-9_]*/, 'identifier'],

            // Single-char ASCII operators
            [/[=<>!+\-*/%|&^~]/, 'operator'],

            // Brackets
            [/[\[\]]/, 'delimiter.bracket'],

            // Braces
            [/[{}]/, 'delimiter.brace'],

            // Parentheses and other delimiters
            [/[(),.:;]/, 'delimiter'],
        ],

        // After seeing 'claim' or 'evident', consume whitespace then the next identifier
        claimName: [
            [/\s+/, ''],
            [/[a-z_][A-Za-z0-9_]*/, { token: 'title.function', next: '@pop' }],
            // If no identifier follows, pop back
            [/./, { token: '', next: '@pop' }],
        ],
    },
};

// ── Language configuration (brackets, auto-close, comments) ─────────────

const EVIDENT_LANGUAGE_CONFIG = {
    comments: {
        lineComment: '--',
    },
    brackets: [
        ['{', '}'],
        ['[', ']'],
        ['(', ')'],
    ],
    autoClosingPairs: [
        { open: '{',  close: '}' },
        { open: '[',  close: ']' },
        { open: '(',  close: ')' },
        { open: '"',  close: '"', notIn: ['string', 'comment'] },
    ],
    surroundingPairs: [
        { open: '{',  close: '}' },
        { open: '[',  close: ']' },
        { open: '(',  close: ')' },
        { open: '"',  close: '"' },
    ],
    indentationRules: {
        // Indent after schema/claim/evident/when blocks
        increaseIndentPattern: /^\s*(schema|claim|evident|when)\b.*$/,
        decreaseIndentPattern: /^\s*$/,
    },
    folding: {
        // Fold schema/claim blocks based on indentation
        offSide: true,
    },
};

// ── Catppuccin Mocha-inspired theme ─────────────────────────────────────

const EVIDENT_THEME = {
    base: 'vs-dark',
    inherit: true,
    rules: [
        // Core language tokens
        { token: 'keyword',           foreground: '89b4fa', fontStyle: 'bold' },
        { token: 'keyword.type',      foreground: 'cba6f7', fontStyle: 'bold' },
        { token: 'keyword.control',   foreground: 'cba6f7' },
        { token: 'keyword.operator',  foreground: '89b4fa' },
        { token: 'keyword.other',     foreground: '89b4fa' },

        // Identifiers and functions
        { token: 'title.function',    foreground: 'a6e3a1', fontStyle: 'bold' },
        { token: 'identifier',        foreground: 'cdd6f4' },
        { token: 'attribute',         foreground: 'f9e2af' },   // field names: warm yellow

        // Types
        { token: 'type.builtin',      foreground: '89dceb', fontStyle: 'bold' },
        { token: 'type.identifier',   foreground: '89dceb' },

        // Operators
        { token: 'operator',          foreground: 'f5c2e7' },
        { token: 'operator.word',     foreground: 'f5c2e7' },

        // Literals
        { token: 'number',            foreground: 'fab387' },
        { token: 'string',            foreground: 'a6e3a1' },

        // Comments
        { token: 'comment',           foreground: '6c7086', fontStyle: 'italic' },

        // Variables
        { token: 'variable',          foreground: '9399b2' },
        { token: 'variable.query',    foreground: 'f9e2af', fontStyle: 'bold' },

        // Delimiters
        { token: 'delimiter',         foreground: '6c7086' },
        { token: 'delimiter.bracket', foreground: '89b4fa' },
        { token: 'delimiter.brace',   foreground: 'cba6f7' },
    ],
    colors: {
        'editor.background':               '#1e1e2e',
        'editor.foreground':               '#cdd6f4',
        'editorLineNumber.foreground':     '#6c7086',
        'editorLineNumber.activeForeground': '#cdd6f4',
        'editorGutter.background':         '#1e1e2e',
        'editorCursor.foreground':         '#f5c2e7',
        'editor.selectionBackground':      '#313244',
        'editor.inactiveSelectionBackground': '#2a2a3d',
        'editor.lineHighlightBackground':  '#24243a',
        'editorIndentGuide.background1':   '#313244',
        'editorIndentGuide.activeBackground1': '#45475a',
        'editor.findMatchBackground':      '#f9e2af40',
        'editor.findMatchHighlightBackground': '#f9e2af20',
        'editorWidget.background':         '#181825',
        'editorWidget.border':             '#313244',
        'editorSuggestWidget.background':  '#181825',
        'editorSuggestWidget.border':      '#313244',
        'editorSuggestWidget.selectedBackground': '#313244',
        'list.hoverBackground':            '#313244',
        'list.activeSelectionBackground':  '#45475a',
        'scrollbar.shadow':                '#00000040',
        'scrollbarSlider.background':      '#45475a80',
        'scrollbarSlider.hoverBackground': '#6c708680',
        'scrollbarSlider.activeBackground': '#9399b280',
    },
};

/**
 * Register the Evident language and theme with a Monaco instance.
 * Call this before creating any editor with language: 'evident'.
 *
 * @param {typeof import('monaco-editor')} monacoInstance
 */
function registerEvidentLanguage(monacoInstance) {
    monacoInstance.languages.register({ id: EVIDENT_LANGUAGE_ID });

    monacoInstance.languages.setMonarchTokensProvider(
        EVIDENT_LANGUAGE_ID,
        EVIDENT_TOKENS,
    );

    monacoInstance.languages.setLanguageConfiguration(
        EVIDENT_LANGUAGE_ID,
        EVIDENT_LANGUAGE_CONFIG,
    );

    monacoInstance.editor.defineTheme('evident-dark', EVIDENT_THEME);
}
