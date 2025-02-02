# Tish Templating Guide

This document describes the templating capabilities in Tish, with examples and best practices.

## Basic Syntax

### Variables

Variables are referenced using curly braces:

```
{user}         # Current username
{host}         # System hostname
{path-folder}  # Current directory name
```

### Assignment

```
{let name = 'value'}    # Variable assignment
{const MAX = '100'}     # Constant declaration
```

### Conditionals

Basic if statement:

```
{if status equals 'success' {
    Output successful
} else {
    Output failed
}}
```

Multiple conditions:

```
{if level greater 80 {
    High
} else if level greater 60 {
    Medium
} else {
    Low
}}
```

### String Operations

```
{let path = '/home/user/file.txt'}
{let filename = path | split('/', -1)}
{let ext = path | match('\.(\w+)$', 1)}
```

## Advanced Features

### Nested Conditions with Styling

```
{let severity = 'high'}
{let status = if severity equals 'high' {
    critical
} else {
    normal
}}

<s.{if status equals 'critical' {red} else {green}}>
    System Status: {status}
</s>
```

### Command Output Integration

```
{const docker_conn = cmd('docker context show')}
{const version = cmd('docker --version') | match('Docker version (\d+\.\d+\.\d+)', 1)}

<s.b>Running Docker {version} on {docker_conn}</s>
```

### Git Status Integration

```
{if git.in-repo {
    <s.{if git.working.changed {
        yellow
    } else if git.ahead > 0 {
        purple
    } else {
        green
    }}>
        Branch: {git.branch}{git.status}
    </s>
}}
```

### Environment Variables

```
{let env_mode = $MODE}
{let config = if env_mode equals 'production' {
    prod.config
} else {
    dev.config
}}
```

## Styling Tags

Style tags can be used to format output:

```
<s.green>Success message</s>
<s.red>Error message</s>
<s.b>Bold text</s>
<s.i>Italic text</s>
<s.u>Underlined text</s>
```

### Color Codes

- Named colors: `red`, `green`, `blue`, `yellow`, etc.
- Hex colors: `<s.#FF5733>Custom color</s>`
- Style combinations: `<s.b><s.green>Bold green text</s></s>`

## Best Practices

1. **Use Meaningful Names**

   ```
   {let connection_status = 'active'}  # Good
   {let cs = 'active'}                # Less clear
   ```

2. **Structured Conditionals**

   ```
   {if status equals 'error' && priority equals 'high' {
       <s.red>Critical Error</s>
   } else if status equals 'warning' {
       <s.yellow>Warning</s>
   } else {
       <s.green>Normal</s>
   }}
   ```

3. **Modular Templates**
   Break complex templates into smaller, reusable parts using variables:
   ```
   {let status_color = if status equals 'error' {red} else {green}}
   <s.{status_color}>{message}</s>
   ```

## Common Use Cases

### Custom Prompt

```
{const docker_conn = cmd('docker context show')}
{const version = cmd('docker --version') | match('Docker version (\d+\.\d+\.\d+)', 1)}

<s.b><s.cyan>{path-folder}</s>
{if git.in-repo {
    on <s.magenta>{git.branch}{git.status}</s>
}}
<s.green>➜</s>{' '}
```

### Status Indicators

```
{let status = 'running'}
{let progress = '75'}

<s.{if status equals 'running' {
    if progress greater 50 {green} else {yellow}
}}>
    Status: {status} ({progress}%)
</s>
```

## Debugging Tips

1. Use simple strings first, then add complexity
2. Test conditions separately before combining
3. Verify variable values with direct output
4. Check syntax nesting with matching braces
5. Use consistent spacing for readability
