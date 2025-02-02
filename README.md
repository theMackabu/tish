# TISH: The Tiny Shell

TISH is a small and fast Unix shell written in Rust. It aims to provide a powerful and customizable shell experience while keeping the codebase concise and efficient.

## Features

- **Rich Templating System**: Support for conditional logic, variables, and complex string operations
- **Git Integration**: Built-in git status information in prompts
- **Lua Scripting**: Extensible through Lua scripts
- **Modern CLI Features**:
  - Syntax highlighting
  - Command completion
  - History management
  - Auto-cd navigation
- **Custom Commands**: Enhanced `ls` command with icons and color coding
- **Environment Management**: Smart environment variable handling and expansion

## Installation

```bash
cargo install tish
```

## Usage

### Basic Commands

```bash
tish                 # Start shell
tish -c "command"    # Execute command and exit
tish -n             # Start without loading environment
tish -H             # Run in headless mode
tish -L             # Login shell (loads .tish_profile)
```

### Prompt Customization

Tish uses a powerful templating system for prompt customization. The default prompt template is:

```
{user}@{host} {path} {prompt}
```

Available template variables include:

- `{user}`: Current username
- `{host}`: Hostname
- `{path}`: Current path (with variants like path-pretty, path-folder)
- `{git.*}`: Git status information
- `{prompt}`: Shell prompt character (# for root, % for users)

### Git Integration

Git information is automatically available in templates:

- `{git.branch}`: Current branch name
- `{git.status}`: Status indicators (+, ~, -)
- `{git.ahead}`, `{git.behind}`: Commit difference with remote
- `{git.working.changed}`: Working directory status
- `{git.staging.changed}`: Staging area status

### Configuration

Configuration is done through `.tishrc` in your home directory:

```lua
-- Example configuration
config.history_size = 500
config.auto_cd = true
config.use_tish_ls = true
config.show_hidden = false
config.prompt = "{user}@{host} {path} {prompt} "
```

## Development

### Building from Source

```bash
git clone https://github.com/themackabu/tish
cd tish
cargo install --path .
```

### Requirements

Unix-like operating system (Linux, macOS, BSD)
