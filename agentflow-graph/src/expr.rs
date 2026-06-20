use std::collections::HashMap;

use serde_json::{Number, Value};

use crate::{AgentFlowError, FlowValue, async_node::AsyncNodeResult};

#[derive(Debug, Clone, PartialEq)]
pub struct ExprError {
  pub col: usize,
  pub message: String,
}

impl ExprError {
  fn new(col: usize, message: impl Into<String>) -> Self {
    Self {
      col,
      message: message.into(),
    }
  }
}

impl std::fmt::Display for ExprError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Error at col {}: {}", self.col, self.message)
  }
}

impl std::error::Error for ExprError {}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprValue {
  Null,
  Bool(bool),
  Number(f64),
  String(String),
  Array(Vec<ExprValue>),
  Object(serde_json::Map<String, Value>),
}

impl ExprValue {
  fn truthy(&self) -> bool {
    match self {
      Self::Null => false,
      Self::Bool(value) => *value,
      Self::Number(value) => *value != 0.0,
      Self::String(value) => {
        let lower = value.to_ascii_lowercase();
        !value.is_empty() && lower != "false" && lower != "0"
      }
      Self::Array(value) => !value.is_empty(),
      Self::Object(value) => !value.is_empty(),
    }
  }

  fn to_number(&self, col: usize) -> Result<f64, ExprError> {
    match self {
      Self::Number(value) => Ok(*value),
      Self::String(value) => value
        .parse::<f64>()
        .map_err(|_| ExprError::new(col, format!("expected number, got '{value}'"))),
      Self::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
      Self::Null => Ok(0.0),
      _ => Err(ExprError::new(col, "expected number")),
    }
  }

  fn to_expr_string(&self) -> String {
    match self {
      Self::Null => "null".to_string(),
      Self::Bool(value) => value.to_string(),
      Self::Number(value) => {
        if value.fract() == 0.0 {
          (*value as i64).to_string()
        } else {
          value.to_string()
        }
      }
      Self::String(value) => value.clone(),
      Self::Array(value) => {
        Value::Array(value.iter().map(ExprValue::to_json).collect()).to_string()
      }
      Self::Object(value) => Value::Object(value.clone()).to_string(),
    }
  }

  fn to_json(&self) -> Value {
    match self {
      Self::Null => Value::Null,
      Self::Bool(value) => Value::Bool(*value),
      Self::Number(value) => Number::from_f64(*value).map_or(Value::Null, Value::Number),
      Self::String(value) => Value::String(value.clone()),
      Self::Array(value) => Value::Array(value.iter().map(ExprValue::to_json).collect()),
      Self::Object(value) => Value::Object(value.clone()),
    }
  }
}

impl From<&FlowValue> for ExprValue {
  fn from(value: &FlowValue) -> Self {
    match value {
      FlowValue::Json(value) => Self::from(value),
      FlowValue::File { path, mime_type } => {
        let mut object = serde_json::Map::new();
        object.insert("type".to_string(), Value::String("file".to_string()));
        object.insert(
          "path".to_string(),
          Value::String(path.to_string_lossy().to_string()),
        );
        object.insert(
          "mime_type".to_string(),
          mime_type.clone().map_or(Value::Null, Value::String),
        );
        Self::Object(object)
      }
      FlowValue::Url { url, mime_type } => {
        let mut object = serde_json::Map::new();
        object.insert("type".to_string(), Value::String("url".to_string()));
        object.insert("url".to_string(), Value::String(url.clone()));
        object.insert(
          "mime_type".to_string(),
          mime_type.clone().map_or(Value::Null, Value::String),
        );
        Self::Object(object)
      }
    }
  }
}

impl From<&Value> for ExprValue {
  fn from(value: &Value) -> Self {
    match value {
      Value::Null => Self::Null,
      Value::Bool(value) => Self::Bool(*value),
      Value::Number(value) => Self::Number(value.as_f64().unwrap_or(0.0)),
      Value::String(value) => Self::String(value.clone()),
      Value::Array(value) => Self::Array(value.iter().map(Self::from).collect()),
      Value::Object(value) => Self::Object(value.clone()),
    }
  }
}

#[derive(Debug, Clone)]
pub struct ExprContext<'a> {
  pub nodes: &'a HashMap<String, AsyncNodeResult>,
  pub inputs: &'a HashMap<String, FlowValue>,
}

impl<'a> ExprContext<'a> {
  pub fn new(
    nodes: &'a HashMap<String, AsyncNodeResult>,
    inputs: &'a HashMap<String, FlowValue>,
  ) -> Self {
    Self { nodes, inputs }
  }

  fn resolve_path(&self, path: &[String], col: usize) -> Result<ExprValue, ExprError> {
    match path.first().map(String::as_str) {
      Some("nodes") => self.resolve_node_path(path, col),
      Some("inputs") => self.resolve_input_path(path, col),
      Some(name) => self.resolve_input_shorthand(name, &path[1..], col),
      None => Err(ExprError::new(col, "empty path")),
    }
  }

  fn resolve_node_path(&self, path: &[String], col: usize) -> Result<ExprValue, ExprError> {
    if path.len() < 4 || path[2] != "outputs" {
      return Err(ExprError::new(
        col,
        "node paths must use nodes.<node_id>.outputs.<field>",
      ));
    }

    let node_id = &path[1];
    let output_name = &path[3];
    let result = self
      .nodes
      .get(node_id)
      .ok_or_else(|| ExprError::new(col, format!("unknown node '{node_id}'")))?;

    let outputs = match result {
      Ok(outputs) => outputs,
      Err(AgentFlowError::NodeSkipped) => return Ok(ExprValue::Null),
      Err(err) => return Err(ExprError::new(col, err.to_string())),
    };
    let value = outputs
      .get(output_name)
      .ok_or_else(|| ExprError::new(col, format!("unknown output '{output_name}'")))?;

    access_path(ExprValue::from(value), &path[4..], col)
  }

  fn resolve_input_path(&self, path: &[String], col: usize) -> Result<ExprValue, ExprError> {
    if path.len() < 2 {
      return Err(ExprError::new(col, "input paths must use inputs.<name>"));
    }
    self.resolve_input_shorthand(&path[1], &path[2..], col)
  }

  fn resolve_input_shorthand(
    &self,
    name: &str,
    rest: &[String],
    col: usize,
  ) -> Result<ExprValue, ExprError> {
    let value = self
      .inputs
      .get(name)
      .ok_or_else(|| ExprError::new(col, format!("unknown input '{name}'")))?;
    access_path(ExprValue::from(value), rest, col)
  }
}

pub fn compile(expr: &str) -> Result<(), ExprError> {
  Parser::new(expr).parse()?.validate()
}

pub fn evaluate(
  expr: &str,
  nodes: &HashMap<String, AsyncNodeResult>,
  inputs: &HashMap<String, FlowValue>,
) -> Result<ExprValue, ExprError> {
  let ast = Parser::new(expr).parse()?;
  ast.eval(&ExprContext::new(nodes, inputs))
}

pub fn evaluate_bool(
  expr: &str,
  nodes: &HashMap<String, AsyncNodeResult>,
  inputs: &HashMap<String, FlowValue>,
) -> Result<bool, ExprError> {
  Ok(evaluate(expr, nodes, inputs)?.truthy())
}

pub fn normalize_expression(expr: &str) -> &str {
  let trimmed = expr.trim();
  if let Some(inner) = trimmed
    .strip_prefix("{{")
    .and_then(|value| value.strip_suffix("}}"))
  {
    inner.trim()
  } else {
    trimmed
  }
}

fn access_path(mut value: ExprValue, path: &[String], col: usize) -> Result<ExprValue, ExprError> {
  for part in path {
    value = match value {
      ExprValue::Array(items) => {
        let index = part
          .parse::<usize>()
          .map_err(|_| ExprError::new(col, format!("array index must be numeric: {part}")))?;
        items
          .get(index)
          .cloned()
          .ok_or_else(|| ExprError::new(col, format!("array index out of bounds: {index}")))?
      }
      ExprValue::Object(object) => object
        .get(part)
        .map(ExprValue::from)
        .ok_or_else(|| ExprError::new(col, format!("unknown field '{part}'")))?,
      _ => {
        return Err(ExprError::new(
          col,
          format!("cannot access field '{part}' on scalar value"),
        ));
      }
    };
  }
  Ok(value)
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
  Literal(ExprValue),
  Path(Vec<String>, usize),
  Unary {
    op: UnaryOp,
    expr: Box<Expr>,
    col: usize,
  },
  Binary {
    op: BinaryOp,
    left: Box<Expr>,
    right: Box<Expr>,
    col: usize,
  },
  Function {
    name: String,
    args: Vec<Expr>,
    col: usize,
  },
}

impl Expr {
  fn validate(&self) -> Result<(), ExprError> {
    match self {
      Self::Literal(_) | Self::Path(_, _) => Ok(()),
      Self::Unary { expr, .. } => expr.validate(),
      Self::Binary { left, right, .. } => {
        left.validate()?;
        right.validate()
      }
      Self::Function { name, args, col } => {
        match name.as_str() {
          "len" | "is_null" | "is_empty" | "to_number" | "to_string" => {
            expect_arity(name, args, 1, *col)?
          }
          "contains" => expect_arity(name, args, 2, *col)?,
          _ => return unknown_function(name, *col),
        }
        for arg in args {
          arg.validate()?;
        }
        Ok(())
      }
    }
  }

  fn eval(&self, ctx: &ExprContext<'_>) -> Result<ExprValue, ExprError> {
    match self {
      Self::Literal(value) => Ok(value.clone()),
      Self::Path(path, col) => ctx.resolve_path(path, *col),
      Self::Unary { op, expr, col } => {
        let value = expr.eval(ctx)?;
        match op {
          UnaryOp::Not => Ok(ExprValue::Bool(!value.truthy())),
          UnaryOp::Negate => Ok(ExprValue::Number(-value.to_number(*col)?)),
        }
      }
      Self::Binary {
        op,
        left,
        right,
        col,
      } => eval_binary(*op, left, right, ctx, *col),
      Self::Function { name, args, col } => eval_function(name, args, ctx, *col),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnaryOp {
  Not,
  Negate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOp {
  Or,
  And,
  Eq,
  Ne,
  Gt,
  Lt,
  Ge,
  Le,
  Add,
  Sub,
  Mul,
  Div,
}

fn eval_binary(
  op: BinaryOp,
  left: &Expr,
  right: &Expr,
  ctx: &ExprContext<'_>,
  col: usize,
) -> Result<ExprValue, ExprError> {
  match op {
    BinaryOp::Or => {
      let left = left.eval(ctx)?;
      if left.truthy() {
        return Ok(ExprValue::Bool(true));
      }
      Ok(ExprValue::Bool(right.eval(ctx)?.truthy()))
    }
    BinaryOp::And => {
      let left = left.eval(ctx)?;
      if !left.truthy() {
        return Ok(ExprValue::Bool(false));
      }
      Ok(ExprValue::Bool(right.eval(ctx)?.truthy()))
    }
    BinaryOp::Eq => Ok(ExprValue::Bool(compare_values(
      &left.eval(ctx)?,
      &right.eval(ctx)?,
      |ordering| ordering == std::cmp::Ordering::Equal,
    ))),
    BinaryOp::Ne => Ok(ExprValue::Bool(!compare_values(
      &left.eval(ctx)?,
      &right.eval(ctx)?,
      |ordering| ordering == std::cmp::Ordering::Equal,
    ))),
    BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
      let left = left.eval(ctx)?;
      let right = right.eval(ctx)?;
      Ok(ExprValue::Bool(compare_ordered(op, &left, &right, col)?))
    }
    BinaryOp::Add => {
      let left = left.eval(ctx)?;
      let right = right.eval(ctx)?;
      if matches!(left, ExprValue::String(_)) || matches!(right, ExprValue::String(_)) {
        Ok(ExprValue::String(format!(
          "{}{}",
          left.to_expr_string(),
          right.to_expr_string()
        )))
      } else {
        Ok(ExprValue::Number(
          left.to_number(col)? + right.to_number(col)?,
        ))
      }
    }
    BinaryOp::Sub => Ok(ExprValue::Number(
      left.eval(ctx)?.to_number(col)? - right.eval(ctx)?.to_number(col)?,
    )),
    BinaryOp::Mul => Ok(ExprValue::Number(
      left.eval(ctx)?.to_number(col)? * right.eval(ctx)?.to_number(col)?,
    )),
    BinaryOp::Div => {
      let divisor = right.eval(ctx)?.to_number(col)?;
      if divisor == 0.0 {
        return Err(ExprError::new(col, "division by zero"));
      }
      Ok(ExprValue::Number(left.eval(ctx)?.to_number(col)? / divisor))
    }
  }
}

fn compare_values(
  left: &ExprValue,
  right: &ExprValue,
  predicate: impl FnOnce(std::cmp::Ordering) -> bool,
) -> bool {
  let numeric_pair = match (left, right) {
    (ExprValue::Number(left), ExprValue::Number(right)) => Some((*left, *right)),
    (ExprValue::Number(left), ExprValue::String(right)) => {
      right.parse::<f64>().ok().map(|right| (*left, right))
    }
    (ExprValue::String(left), ExprValue::Number(right)) => {
      left.parse::<f64>().ok().map(|left| (left, *right))
    }
    _ => None,
  };
  if let Some((left, right)) = numeric_pair {
    return predicate(
      left
        .partial_cmp(&right)
        .unwrap_or(std::cmp::Ordering::Equal),
    );
  }
  predicate(left.to_expr_string().cmp(&right.to_expr_string()))
}

fn compare_ordered(
  op: BinaryOp,
  left: &ExprValue,
  right: &ExprValue,
  col: usize,
) -> Result<bool, ExprError> {
  let ordering = match (left, right) {
    (ExprValue::String(left), ExprValue::String(right)) => left.cmp(right),
    _ => left
      .to_number(col)?
      .partial_cmp(&right.to_number(col)?)
      .ok_or_else(|| ExprError::new(col, "values cannot be compared"))?,
  };
  Ok(match op {
    BinaryOp::Gt => ordering == std::cmp::Ordering::Greater,
    BinaryOp::Lt => ordering == std::cmp::Ordering::Less,
    BinaryOp::Ge => ordering != std::cmp::Ordering::Less,
    BinaryOp::Le => ordering != std::cmp::Ordering::Greater,
    _ => unreachable!("non-ordering operator"),
  })
}

fn eval_function(
  name: &str,
  args: &[Expr],
  ctx: &ExprContext<'_>,
  col: usize,
) -> Result<ExprValue, ExprError> {
  match name {
    "len" => {
      expect_arity(name, args, 1, col)?;
      let value = args[0].eval(ctx)?;
      let len = match value {
        ExprValue::Null => 0,
        ExprValue::String(value) => value.chars().count(),
        ExprValue::Array(value) => value.len(),
        ExprValue::Object(value) => value.len(),
        _ => {
          return Err(ExprError::new(
            col,
            "len() expects string, array, object, or null",
          ));
        }
      };
      Ok(ExprValue::Number(len as f64))
    }
    "contains" => {
      expect_arity(name, args, 2, col)?;
      let haystack = args[0].eval(ctx)?;
      let needle = args[1].eval(ctx)?.to_expr_string();
      let result = match haystack {
        ExprValue::String(value) => value.contains(&needle),
        ExprValue::Array(value) => value.iter().any(|item| item.to_expr_string() == needle),
        _ => return Err(ExprError::new(col, "contains() expects string or array")),
      };
      Ok(ExprValue::Bool(result))
    }
    "is_null" => {
      expect_arity(name, args, 1, col)?;
      Ok(ExprValue::Bool(matches!(
        args[0].eval(ctx)?,
        ExprValue::Null
      )))
    }
    "is_empty" => {
      expect_arity(name, args, 1, col)?;
      let value = args[0].eval(ctx)?;
      Ok(ExprValue::Bool(match value {
        ExprValue::Null => true,
        ExprValue::String(value) => value.is_empty(),
        ExprValue::Array(value) => value.is_empty(),
        ExprValue::Object(value) => value.is_empty(),
        _ => false,
      }))
    }
    "to_number" => {
      expect_arity(name, args, 1, col)?;
      Ok(ExprValue::Number(args[0].eval(ctx)?.to_number(col)?))
    }
    "to_string" => {
      expect_arity(name, args, 1, col)?;
      Ok(ExprValue::String(args[0].eval(ctx)?.to_expr_string()))
    }
    _ => unknown_function(name, col),
  }
}

fn unknown_function<T>(name: &str, col: usize) -> Result<T, ExprError> {
  let suggestion = if name == "lenn" {
    ", did you mean 'len'?"
  } else {
    ""
  };
  Err(ExprError::new(
    col,
    format!("unknown function '{name}'{suggestion}"),
  ))
}

fn expect_arity(name: &str, args: &[Expr], expected: usize, col: usize) -> Result<(), ExprError> {
  if args.len() == expected {
    Ok(())
  } else {
    Err(ExprError::new(
      col,
      format!(
        "{name}() expects {expected} argument(s), got {}",
        args.len()
      ),
    ))
  }
}

struct Parser<'a> {
  chars: Vec<char>,
  pos: usize,
  _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> Parser<'a> {
  fn new(input: &'a str) -> Self {
    let input = normalize_expression(input);
    Self {
      chars: input.chars().collect(),
      pos: 0,
      _marker: std::marker::PhantomData,
    }
  }

  fn parse(mut self) -> Result<Expr, ExprError> {
    let expr = self.parse_or()?;
    self.skip_ws();
    if !self.is_eof() {
      return Err(ExprError::new(self.col(), "unexpected token"));
    }
    Ok(expr)
  }

  fn parse_or(&mut self) -> Result<Expr, ExprError> {
    let mut expr = self.parse_and()?;
    while self.consume("||") {
      let col = self.col().saturating_sub(2);
      let right = self.parse_and()?;
      expr = Expr::Binary {
        op: BinaryOp::Or,
        left: Box::new(expr),
        right: Box::new(right),
        col,
      };
    }
    Ok(expr)
  }

  fn parse_and(&mut self) -> Result<Expr, ExprError> {
    let mut expr = self.parse_equality()?;
    while self.consume("&&") {
      let col = self.col().saturating_sub(2);
      let right = self.parse_equality()?;
      expr = Expr::Binary {
        op: BinaryOp::And,
        left: Box::new(expr),
        right: Box::new(right),
        col,
      };
    }
    Ok(expr)
  }

  fn parse_equality(&mut self) -> Result<Expr, ExprError> {
    let mut expr = self.parse_comparison()?;
    loop {
      let (op, width) = if self.consume("==") {
        (BinaryOp::Eq, 2)
      } else if self.consume("!=") {
        (BinaryOp::Ne, 2)
      } else {
        break;
      };
      let col = self.col().saturating_sub(width);
      let right = self.parse_comparison()?;
      expr = Expr::Binary {
        op,
        left: Box::new(expr),
        right: Box::new(right),
        col,
      };
    }
    Ok(expr)
  }

  fn parse_comparison(&mut self) -> Result<Expr, ExprError> {
    let mut expr = self.parse_term()?;
    loop {
      let (op, width) = if self.consume(">=") {
        (BinaryOp::Ge, 2)
      } else if self.consume("<=") {
        (BinaryOp::Le, 2)
      } else if self.consume(">") {
        (BinaryOp::Gt, 1)
      } else if self.consume("<") {
        (BinaryOp::Lt, 1)
      } else {
        break;
      };
      let col = self.col().saturating_sub(width);
      let right = self.parse_term()?;
      expr = Expr::Binary {
        op,
        left: Box::new(expr),
        right: Box::new(right),
        col,
      };
    }
    Ok(expr)
  }

  fn parse_term(&mut self) -> Result<Expr, ExprError> {
    let mut expr = self.parse_factor()?;
    loop {
      let (op, width) = if self.consume("+") {
        (BinaryOp::Add, 1)
      } else if self.consume("-") {
        (BinaryOp::Sub, 1)
      } else {
        break;
      };
      let col = self.col().saturating_sub(width);
      let right = self.parse_factor()?;
      expr = Expr::Binary {
        op,
        left: Box::new(expr),
        right: Box::new(right),
        col,
      };
    }
    Ok(expr)
  }

  fn parse_factor(&mut self) -> Result<Expr, ExprError> {
    let mut expr = self.parse_unary()?;
    loop {
      let (op, width) = if self.consume("*") {
        (BinaryOp::Mul, 1)
      } else if self.consume("/") {
        (BinaryOp::Div, 1)
      } else {
        break;
      };
      let col = self.col().saturating_sub(width);
      let right = self.parse_unary()?;
      expr = Expr::Binary {
        op,
        left: Box::new(expr),
        right: Box::new(right),
        col,
      };
    }
    Ok(expr)
  }

  fn parse_unary(&mut self) -> Result<Expr, ExprError> {
    if self.consume("!") {
      let col = self.col().saturating_sub(1);
      let expr = self.parse_unary()?;
      return Ok(Expr::Unary {
        op: UnaryOp::Not,
        expr: Box::new(expr),
        col,
      });
    }
    if self.consume("-") {
      let col = self.col().saturating_sub(1);
      let expr = self.parse_unary()?;
      return Ok(Expr::Unary {
        op: UnaryOp::Negate,
        expr: Box::new(expr),
        col,
      });
    }
    self.parse_primary()
  }

  fn parse_primary(&mut self) -> Result<Expr, ExprError> {
    self.skip_ws();
    let col = self.col();
    if self.consume("(") {
      let expr = self.parse_or()?;
      if !self.consume(")") {
        return Err(ExprError::new(self.col(), "expected ')'"));
      }
      return Ok(expr);
    }

    if self.peek() == Some('"') || self.peek() == Some('\'') {
      return self.parse_string();
    }

    if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
      return self.parse_number();
    }

    let ident = self.parse_identifier()?;
    match ident.as_str() {
      "true" => Ok(Expr::Literal(ExprValue::Bool(true))),
      "false" => Ok(Expr::Literal(ExprValue::Bool(false))),
      "null" => Ok(Expr::Literal(ExprValue::Null)),
      _ if self.consume("(") => {
        let args = self.parse_args()?;
        Ok(Expr::Function {
          name: ident,
          args,
          col,
        })
      }
      _ => {
        let mut path = vec![ident];
        while self.consume(".") {
          path.push(self.parse_path_segment()?);
        }
        Ok(Expr::Path(path, col))
      }
    }
  }

  fn parse_args(&mut self) -> Result<Vec<Expr>, ExprError> {
    let mut args = Vec::new();
    self.skip_ws();
    if self.consume(")") {
      return Ok(args);
    }
    loop {
      args.push(self.parse_or()?);
      if self.consume(")") {
        return Ok(args);
      }
      if !self.consume(",") {
        return Err(ExprError::new(self.col(), "expected ',' or ')'"));
      }
    }
  }

  fn parse_string(&mut self) -> Result<Expr, ExprError> {
    self.skip_ws();
    let quote = self
      .peek()
      .ok_or_else(|| ExprError::new(self.col(), "expected string"))?;
    self.pos += 1;
    let mut value = String::new();
    while let Some(ch) = self.peek() {
      self.pos += 1;
      if ch == quote {
        return Ok(Expr::Literal(ExprValue::String(value)));
      }
      if ch == '\\' {
        let escaped = self
          .peek()
          .ok_or_else(|| ExprError::new(self.col(), "unterminated escape"))?;
        self.pos += 1;
        value.push(match escaped {
          'n' => '\n',
          'r' => '\r',
          't' => '\t',
          '"' => '"',
          '\'' => '\'',
          '\\' => '\\',
          other => other,
        });
      } else {
        value.push(ch);
      }
    }
    Err(ExprError::new(self.col(), "unterminated string"))
  }

  fn parse_number(&mut self) -> Result<Expr, ExprError> {
    self.skip_ws();
    let start = self.pos;
    while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
      self.pos += 1;
    }
    if self.peek() == Some('.') {
      self.pos += 1;
      while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
        self.pos += 1;
      }
    }
    let raw: String = self.chars[start..self.pos].iter().collect();
    let value = raw
      .parse::<f64>()
      .map_err(|_| ExprError::new(start + 1, format!("invalid number '{raw}'")))?;
    Ok(Expr::Literal(ExprValue::Number(value)))
  }

  fn parse_identifier(&mut self) -> Result<String, ExprError> {
    self.skip_ws();
    let start = self.pos;
    let Some(ch) = self.peek() else {
      return Err(ExprError::new(self.col(), "expected expression"));
    };
    if !is_ident_start(ch) {
      return Err(ExprError::new(
        self.col(),
        format!("unexpected character '{ch}'"),
      ));
    }
    self.pos += 1;
    while self.peek().is_some_and(is_ident_continue) {
      self.pos += 1;
    }
    Ok(self.chars[start..self.pos].iter().collect())
  }

  fn parse_path_segment(&mut self) -> Result<String, ExprError> {
    self.skip_ws();
    let start = self.pos;
    while self
      .peek()
      .is_some_and(|ch| is_ident_continue(ch) || ch.is_ascii_digit())
    {
      self.pos += 1;
    }
    if start == self.pos {
      return Err(ExprError::new(self.col(), "expected path segment"));
    }
    Ok(self.chars[start..self.pos].iter().collect())
  }

  fn consume(&mut self, token: &str) -> bool {
    self.skip_ws();
    let token_chars: Vec<char> = token.chars().collect();
    if self.chars[self.pos..].starts_with(&token_chars) {
      self.pos += token_chars.len();
      true
    } else {
      false
    }
  }

  fn skip_ws(&mut self) {
    while self.peek().is_some_and(char::is_whitespace) {
      self.pos += 1;
    }
  }

  fn peek(&self) -> Option<char> {
    self.chars.get(self.pos).copied()
  }

  fn is_eof(&self) -> bool {
    self.pos >= self.chars.len()
  }

  fn col(&self) -> usize {
    self.pos + 1
  }
}

fn is_ident_start(ch: char) -> bool {
  ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
  ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
  use super::*;

  fn nodes() -> HashMap<String, AsyncNodeResult> {
    HashMap::from([(
      "search".to_string(),
      Ok(HashMap::from([
        (
          "items".to_string(),
          FlowValue::Json(serde_json::json!(["alpha", "beta"])),
        ),
        ("score".to_string(), FlowValue::Json(serde_json::json!(0.8))),
        (
          "enabled".to_string(),
          FlowValue::Json(serde_json::json!(true)),
        ),
      ])),
    )])
  }

  fn inputs() -> HashMap<String, FlowValue> {
    HashMap::from([
      (
        "iteration".to_string(),
        FlowValue::Json(serde_json::json!(1)),
      ),
      (
        "continue".to_string(),
        FlowValue::Json(serde_json::json!(true)),
      ),
    ])
  }

  #[test]
  fn evaluates_compound_node_expression() {
    let result = evaluate_bool(
      "len(nodes.search.outputs.items) > 0 && nodes.search.outputs.score > 0.7",
      &nodes(),
      &HashMap::new(),
    )
    .unwrap();
    assert!(result);
  }

  #[test]
  fn supports_input_paths_and_shorthand() {
    assert!(evaluate_bool("{{ inputs.iteration < 2 }}", &HashMap::new(), &inputs()).unwrap());
    assert!(evaluate_bool("{{ continue }}", &HashMap::new(), &inputs()).unwrap());
  }

  #[test]
  fn supports_array_index_and_contains() {
    let result = evaluate_bool(
      "contains(nodes.search.outputs.items, 'beta') && nodes.search.outputs.items.0 == 'alpha'",
      &nodes(),
      &HashMap::new(),
    )
    .unwrap();
    assert!(result);
  }

  #[test]
  fn reports_unknown_function_with_column() {
    let error = compile("lenn(nodes.search.outputs.items)").unwrap_err();
    assert_eq!(error.col, 1);
    assert!(error.message.contains("unknown function 'lenn'"));
  }
}
