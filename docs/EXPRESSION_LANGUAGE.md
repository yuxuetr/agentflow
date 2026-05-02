# AgentFlow Expression Language

AgentFlow uses a small expression language for `run_if` and
`while.parameters.condition`. Expressions may be written directly or wrapped in
template braces for compatibility:

```yaml
run_if: "len(nodes.search.outputs.items) > 0 && nodes.classify.outputs.score > 0.7"

parameters:
  condition: "{{ count < 3 }}"
```

## Values

- Booleans: `true`, `false`
- Null: `null`
- Numbers: `1`, `1.5`
- Strings: `"hello"` or `'hello'`
- Paths:
  - `nodes.<node_id>.outputs.<field>`
  - `nodes.<node_id>.outputs.<field>.<nested_field>`
  - `nodes.<node_id>.outputs.<field>.0`
  - `inputs.<name>`
  - `<name>` shorthand for while-loop inputs, for example `count < 3`

`FlowValue::File` and `FlowValue::Url` are exposed as objects with `type`,
`path`/`url`, and `mime_type` fields.

## Operators

Operators are evaluated in standard precedence order:

| Operators | Meaning |
| --- | --- |
| `!`, unary `-` | not, numeric negation |
| `*`, `/` | multiply, divide |
| `+`, `-` | add/subtract; `+` also concatenates strings |
| `>`, `<`, `>=`, `<=` | numeric or string comparison |
| `==`, `!=` | equality / inequality |
| `&&` | logical and |
| `||` | logical or |

Truthiness matches the legacy runtime behavior: `false`, `0`, `null`, empty
strings, empty arrays, and empty objects are false; other values are true.

## Functions

| Function | Description |
| --- | --- |
| `len(x)` | Length of a string, array, object, or `0` for `null` |
| `contains(s, sub)` | String substring check or array membership by string value |
| `is_null(x)` | True when `x` is `null` |
| `is_empty(x)` | True for `null`, empty strings, arrays, or objects |
| `to_number(x)` | Converts strings, booleans, or null to a number |
| `to_string(x)` | Converts any value to a string |

## Validation

`agentflow workflow validate --strict` compiles every node `run_if` expression
and every `while.parameters.condition`. Syntax errors include a column number:

```text
Error at col 1: unknown function 'lenn', did you mean 'len'?
```
