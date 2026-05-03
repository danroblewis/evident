# Evident Language Support for VS Code

Syntax highlighting for the [Evident](https://github.com/danroblewis/evident)
constraint programming language (`.ev` files).

## Features

- Syntax highlighting for all Evident constructs:
  - Schema and claim declarations (`schema Foo`, `claim Bar`)
  - Type declarations (`type Color = Red | Green | Blue`)
  - Quantifiers: `all`, `some`, `none`, `one` and their Unicode forms `∀` `∃` `∃!` `¬∃`
  - Membership operators: `∈` `∉` `∋` `∌` `⊆` `⊇` and word forms `in`, `subset`, `superset`
  - Implication arrow `⇒` and `=>`
  - Logic operators `∧` `∨` `¬` and `and`, `or`, `not`
  - Set algebra `∩` `∪` `×`
  - String operators `⊑` `⊒` `++`, `starts_with`, `ends_with`, `contains`, `matches`
  - Cardinality `#seq`
  - Regex literals `/pattern/` in membership context
  - Sequence literals `⟨ ... ⟩`
  - Passthrough `..SchemaName`
  - Import statements
  - `-- @plot`, `-- @test` annotation comments (highlighted distinctly)
  - Numbers, strings, booleans
- **Evident Dark** color theme (Catppuccin Mocha-inspired)
- Bracket matching: `{}` `[]` `()` `⟨⟩`
- Comment toggling with `--`
- File association for `.ev` extension

## Installation

### From VSIX (recommended for local use)

```bash
cd /path/to/evident/vscode-evident
./install.sh
```

### Manual (no vsce required)

```bash
mkdir -p ~/.vscode/extensions/evident-lang
cp -r . ~/.vscode/extensions/evident-lang/
```

Then restart VS Code.

## Usage

Open any `.ev` file — the language is detected automatically.

To use the Evident Dark theme: open the Command Palette (`Cmd+Shift+P`),
run **Preferences: Color Theme**, and select **Evident Dark**.
