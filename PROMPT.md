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

### Arrays and Maps

Arrays:

```
{let colors = ['red', 'green', 'blue']}
{colors[1]}  # Access second element (green)
```

Maps:

```
{let users = [
    {'name': 'Alice', 'age': 25},
    {'name': 'Bob', 'age': 30}
]}
{users[0].name}  # Access first user's name (Alice)
```

### Loops

Basic array loop:

```
{let colors = ['red', 'green', 'blue']}
{for color in colors {
    Color: {color}{' '}
}}
```

Loop with index:

```
{let colors = ['red', 'green', 'blue']}
{for color, i in colors {
    {i}: {color}{' '}
}}
```

Range-based loop:

```
{for i in 1..5 {
    Count: {i}{' '}
}}
```

Loop over maps:

```
{let users = [
    {'name': 'Alice', 'age': 25},
    {'name': 'Bob', 'age': 30}
]}
{for user in users {
    Name: {user.name}, Age: {user.age}{' '}
}}
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

4. **Efficient Loops**
   When working with loops, consider collecting data first:
   ```
   {let filtered_users = users | filter('status', 'active')}
   {for user in filtered_users {
       {user.name}{' '}
   }}
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

### Data Processing

```
{let items = [
    {'name': 'Task 1', 'status': 'done'},
    {'name': 'Task 2', 'status': 'pending'}
]}

Completed Tasks:
{for item in items {
    {if item.status equals 'done' {
        • {item.name}{'\n'}
    }}
}}
```

# Additional Tish Features

## Default Values

Variables can be assigned default values using the colon syntax. This is useful when you want to provide a fallback value for undefined variables:

```
{undefined_var:'default'}  # Shows 'default' if undefined_var is not set
{status:'pending'}        # Shows value of status if defined, otherwise 'pending'
```

Multiple default values can be used in a single line:

```
{first:'one'} and {second:'two'}
```

Default values can be used in various contexts:

```
# In conditionals
{if status:'pending' equals 'pending' {
    <s.yellow>Waiting</s>
} else {
    <s.red>Error</s>
}}

# In style tags
<s.{color:'blue'}>Default blue text</s>

# With string operations
{input:'test.txt' | split('.', 1)}
```

Default values are also triggered by empty strings:

```
{let empty = ''}
{empty:'was empty'}  # Shows 'was empty'
```

## Boolean Context and Operations

### Implicit Boolean Handling

Empty strings are treated as falsy in boolean contexts, while non-empty strings are truthy:

```
{let empty_str = ''}

# Empty string is falsy
{if empty_str {
    This won't show
} else {
    This will show
}}

# Negation of empty string
{if !empty_str {
    This will show
}}
```

### Boolean Operations

Boolean operations can be combined using standard logical operators:

```
{let connected = true}
{let authorized = true}

# Using AND (&&)
{if connected && authorized {
    Full access granted
}}

# Using OR (||)
{if cpu greater 90 || ram greater 95 {
    Critical state
}}

# Complex combinations
{if (connected && authorized) || admin_override {
    Access granted
}}
```

## Advanced String Operations

### Operation Chaining

Multiple string operations can be chained together using the pipe operator:

```
# Multiple transformations
{let text = 'Hello, World! 123'}
{let result = text | match('\w+', 0) | replace('Hello', 'Hi')}

# Complex regex with capture groups
{let log = '[ERROR] Failed to connect (port 8080)'}
{let error_code = log | match('\[(\w+)\].*port (\d+)', 2)}

# Multiple replacements
{let text = 'a,b,c'}
{let formatted = text | replace(',', ' | ') | replace('a', 'A') | replace('c', 'C')}
```

### Conditional String Operations

String operations can be used within conditional statements:

```
{let path = '/home/user/file.txt'}
{let filename = path | split('/', -1)}
{let ext = if filename | match('\.(\w+)$', 1) equals 'txt' {
    <s.green>text file</s>
} else {
    <s.yellow>other file</s>
}}
```

## Partial File Inclusion

You can include content from external files using the partial inclusion syntax:

```
# Include entire file
{>template.txt}

# Usage in larger template
Header content here
{>header.partial}
Main content here
{>footer.partial}
```

## Conditional Variable Assignment

Variables can be assigned conditionally using if-else expressions:

### Basic Conditional Assignment

```
{let count = 75}
{let status = if count greater 50 {high} else {low}}
```

### Nested Conditional Assignment

```
{let temp = 85}
{let status = if temp greater 90 {
    critical
} else if temp greater 80 {
    warning
} else if temp greater 70 {
    normal
} else {
    low
}}
```

### Multiple Variable Dependencies

```
{let cpu = 80}
{let ram = 90}
{let system_status = if cpu greater 90 || ram greater 95 {
    critical
} else if cpu greater 80 || ram greater 85 {
    warning
} else {
    normal
}}
```

### Style Integration

Conditional assignments can be used to determine styles:

```
{let value = 75}
{let status_color = if value greater 80 {
    red
} else if value greater 60 {
    yellow
} else {
    green
}}
<s.{status_color}>Value: {value}%</s>
```

### Environment-Based Configuration

```
{let env_mode = $MODE}
{let config = if env_mode equals 'production' {
    prod.config
} else if env_mode equals 'staging' {
    staging.config
} else {
    dev.config
}}
```

## Best Practices for Advanced Features

1. **Default Values**

   - Use meaningful default values that make sense in the context
   - Consider using default values for optional configurations
   - Document expected default behaviors in your templates

2. **Boolean Operations**

   - Keep boolean expressions simple and readable
   - Break complex conditions into smaller, assigned variables
   - Use parentheses to make operation precedence clear

3. **String Operations**

   - Chain operations in a logical order
   - Use meaningful variable names for intermediate results
   - Consider breaking very long chains into multiple steps

4. **Conditional Assignments**

   - Use clear, descriptive variable names
   - Break complex conditions into smaller parts
   - Consider using constants for important thresholds

5. **Partial Inclusion**
   - Keep partials focused and single-purpose
   - Use consistent naming conventions for partial files
   - Document dependencies between partials

## Debugging Tips

1. Use simple strings first, then add complexity
2. Test conditions separately before combining
3. Verify variable values with direct output
4. Check syntax nesting with matching braces
5. Use consistent spacing for readability
6. Test loops with small datasets first
7. Verify map/array access with single elements before looping
