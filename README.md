# doner

A CLI tool to retrieve and summarize issues from a GitHub Project board column.

## Installation

```bash
cargo install --path .
```

Or build manually:

```bash
cargo build --release
# Binary will be at ./target/release/doner
```

## Authentication

### Interactive Login (Recommended)

```bash
doner auth login
```

This will:
1. Prompt you for a GitHub Personal Access Token
2. Validate the token with GitHub
3. Store it securely in your system keychain (macOS Keychain, Windows Credential Manager, etc.)

### Create a Token

Create a token at https://github.com/settings/tokens with these scopes:
- `read:project` - to read project data
- `repo` - to access issue information

### Other Auth Commands

```bash
# Check authentication status
doner auth status

# Log out (removes token from keychain)
doner auth logout

# Login with token directly (non-interactive, useful for scripts)
doner auth login --with-token ghp_your_token_here
```

### Environment Variable

You can also use an environment variable (takes precedence over stored token):

```bash
export GITHUB_TOKEN=ghp_your_token_here
```

## Usage

```bash
doner summarize <PROJECT_ID> [OPTIONS]
# or use the short alias
doner sum <PROJECT_ID> [OPTIONS]
```

### Project ID Format

The project ID can be specified in two formats:

1. **Owner/number format**: `owner/number` (e.g., `myorg/5` or `myuser/3`)
   - Find the number in your project URL: `https://github.com/orgs/myorg/projects/5`

2. **GraphQL node ID**: `PVT_kwDO...` (starts with "PVT_")
   - Found in the GitHub API or project settings

### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--col` | `-c` | Column name to fetch issues from | `Done` |
| `--since` | `-s` | Filter issues by time | No limit |
| `--format` | `-f` | Output format (`text` or `markdown`) | `text` |
| `--wrap` | `-w` | Group issues by parent issue | Off |
| `--ai` | | Generate AI-powered rich summary | Off |

### Time Filters

The `--since` option supports various formats:

- **Duration**: `7d` (7 days), `24h` (24 hours), `30m` (30 minutes), `2w` (2 weeks)
- **Keywords**: `yesterday`, `today`, `this-week`, `this-month`

### Examples

Get all issues from the "Done" column:

```bash
doner sum myorg/5
```

Get issues completed in the last 7 days:

```bash
doner sum myorg/5 --since 7d
```

Get issues from "In Review" column, grouped by parent:

```bash
doner sum myorg/5 --col "In Review" --wrap
```

Output as markdown:

```bash
doner sum myorg/5 --since yesterday --format markdown
```

Get issues completed this week, grouped by parent, as markdown:

```bash
doner sum myorg/5 -s this-week -w -f markdown
```

## AI-Powered Summaries

Use the `--ai` flag to generate rich, narrative summaries using a local LLM CLI tool:

```bash
doner sum myorg/5 --since 7d --ai
```

### Supported CLI Tools

The `--ai` flag uses locally installed CLI tools (auto-detected in this order):

1. **gemini-cli** - Google's Gemini CLI ([install](https://github.com/google-gemini/gemini-cli))
2. **cursor** - Cursor's CLI

### Custom LLM Command

Use any CLI tool that accepts a prompt as an argument:

```bash
# Set a custom command (prompt is appended as the last argument)
export DONER_LLM_CMD="llm -m gpt-4"
export DONER_LLM_CMD="ollama run llama3"
export DONER_LLM_CMD="sgpt"

doner sum myorg/5 --ai
```

### Example AI Output

```
## Work Summary

### Infrastructure & Performance
The team focused heavily on system reliability this week:
- Implemented connection pooling for the database layer, reducing latency by 40%
- Fixed memory leak in the background job processor
- Added health check endpoints for all microservices

### User Experience
Several UI improvements shipped:
- Dark mode support across all pages
- Improved mobile responsiveness for the dashboard
- Fixed accessibility issues with form inputs

### Key Themes
The work this week centered on technical debt reduction and polish,
with a strong emphasis on performance optimization and user experience
refinements ahead of the upcoming release.
```

## Output Examples

### Text format (default)

```
Found 3 issue(s):

• [myorg/repo#42] Fix login button alignment
  https://github.com/myorg/repo/issues/42
  Closed: 2024-01-15 14:30

• [myorg/repo#45] Add dark mode support
  https://github.com/myorg/repo/issues/45
  Parent: UI Improvements (https://github.com/myorg/repo/issues/40)
  Closed: 2024-01-15 16:00
```

### Markdown format (`--format markdown`)

```markdown
## Summary (3 issues)

- **[myorg/repo#42](https://github.com/myorg/repo/issues/42)**: Fix login button alignment
  - Closed: 2024-01-15 14:30

- **[myorg/repo#45](https://github.com/myorg/repo/issues/45)**: Add dark mode support
  - Parent: [UI Improvements](https://github.com/myorg/repo/issues/40)
  - Closed: 2024-01-15 16:00
```

### Grouped output (`--wrap`)

```
Found 3 issue(s):

▶ UI Improvements
  https://github.com/myorg/repo/issues/40
  Completed:
    • [myorg/repo#45] Add dark mode support
    • [myorg/repo#46] Update color palette

▶ Standalone Issues
  • [myorg/repo#42] Fix login button alignment
    https://github.com/myorg/repo/issues/42
```

## License

MIT
