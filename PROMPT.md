# Tish Templating Guide

This document describes the templating capabilities in Tish, with examples and best practices.

## Table of Contents

1. [Basic Syntax](#basic-syntax)

   - [Variables](#variables)
   - [Assignment](#assignment)
   - [Arrays and Maps](#arrays-and-maps)
   - [Loops](#loops)
   - [Conditionals](#conditionals)
   - [String Operations](#string-operations)

2. [Advanced Features](#advanced-features)

   - [Default Values](#default-values)
   - [Boolean Operations](#boolean-operations)
   - [Advanced String Operations](#advanced-string-operations)
   - [Partial File Inclusion](#partial-file-inclusion)
   - [Conditional Variable Assignment](#conditional-variable-assignment)

3. [Styling](#styling)

   - [Style Tags](#style-tags)
   - [Color Codes](#color-codes)
   - [Nested Styles](#nested-styles)

4. [Integration Features](#integration-features)

   - [Command Output](#command-output)
   - [Git Status](#git-status)
   - [Environment Variables](#environment-variables)

5. [Best Practices](#best-practices)

   - [Naming Conventions](#naming-conventions)
   - [Code Structure](#code-structure)
   - [Performance Optimization](#performance-optimization)
   - [Debugging Guidelines](#debugging-guidelines)

6. [Common Use Cases](#common-use-cases)
   - [Custom Prompts](#custom-prompts)
   - [Status Indicators](#status-indicators)
   - [Data Processing](#data-processing)

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

Basic string manipulation:

```
{let path = '/home/user/file.txt'}
{let filename = path | split('/', -1)}
{let ext = path | match('\.(\w+)$', 1)}
```

## Advanced Features

### Default Values

Variables can have default values using the colon syntax:

```
{undefined_var:'default'}  # Shows 'default' if undefined_var is not set
{status:'pending'}        # Shows value of status if defined, otherwise 'pending'
```

Multiple default values:

```
{first:'one'} and {second:'two'}
```

Default values in context:

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

### Boolean Operations

Implicit boolean handling:

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

Complex boolean operations:

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

### Advanced String Operations

Operation chaining:

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

### Partial File Inclusion

Include external files:

```
# Include entire file
{>template.txt}

# Usage in larger template
Header content here
{>header.partial}
Main content here
{>footer.partial}
```

### Conditional Variable Assignment

Basic assignment:

```
{let count = 75}
{let status = if count greater 50 {high} else {low}}
```

Nested conditions:

```
{let temp = 85}
{let status = if temp greater 90 {
    critical
} else if temp greater 80 {
    warning
} else if temp greater 70 {
    normal
} else {
    normal
}}
```

## Styling

### Style Tags

Basic styling:

```
<s.green>Success message</s>
<s.red>Error message</s>
<s.b>Bold text</s>
<s.i>Italic text</s>
<s.u>Underlined text</s>
```

### Color Codes

Available colors:

- Named colors: `red`, `green`, `blue`, `yellow`, etc.
- Hex colors: `<s.#FF5733>Custom color</s>`

### Nested Styles

Combining styles:

```
<s.b><s.green>Bold green text</s></s>
```

## Integration Features

### Command Output

```
{const docker_conn = cmd('docker context show')}
{const version = cmd('docker --version') | match('Docker version (\d+\.\d+\.\d+)', 1)}

<s.b>Running Docker {version} on {docker_conn}</s>
```

### Git Status

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
<s.{status_color}>Value: {value}%</s>
```

### Environment Variables

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

## Best Practices

### Naming Conventions

- Use descriptive, meaningful names
- Follow consistent naming patterns
- Avoid abbreviations unless commonly understood

### Code Structure

- Break complex templates into smaller parts
- Use consistent indentation
- Group related functionality

### Performance Optimization

- Minimize redundant operations
- Pre-process data when possible
- Use efficient loops and conditions

### Debugging Guidelines

1. Start Simple

   - Begin with basic templates
   - Add complexity incrementally
   - Test each feature in isolation

2. Common Issues

   - Verify bracket matching
   - Check variable scope
   - Validate conditional logic
   - Test loop behavior with small datasets

3. Testing Strategy
   - Use echo statements for debugging
   - Verify variable values
   - Test edge cases
   - Validate template syntax

## Common Use Cases

### Custom Prompts

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
