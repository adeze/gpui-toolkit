//! Analysis filters: expression calculator, grid resampling, statistics.
//!
//! The calculator evaluates a small arithmetic language per point over
//! named scalar fields plus the coordinates `x`, `y`, `z`:
//! `+ - * / ^`, unary minus, parentheses, and the functions `sin cos tan
//! sqrt exp log abs min max` (the last two take two arguments).

// Rust guideline compliant 2026-02-21

use crate::TransformError;
use crate::probe::probe;
use gpui_mesh::{Association, DataSet, Field};

/// Appends a scalar field computed from an arithmetic expression.
///
/// # Errors
///
/// Returns an error for parse failures, unknown fields or functions, and
/// non-scalar referenced fields.
pub fn calculator(
    dataset: &DataSet,
    expression: &str,
    out_name: &str,
) -> Result<DataSet, TransformError> {
    let ast = Parser::new(expression).parse()?;
    let fields: Vec<(&str, &[f64])> = dataset
        .point_data
        .iter()
        .filter_map(|f| f.as_scalar().map(|values| (f.name.as_ref(), values)))
        .collect();
    let values: Vec<f64> = dataset
        .positions
        .iter()
        .enumerate()
        .map(|(index, p)| eval(&ast, &fields, index, *p))
        .collect::<Result<_, _>>()?;
    let mut out = dataset.clone();
    out.point_data
        .push(Field::scalar(out_name.to_string(), values));
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Field(String),
    Coord(usize),
    Neg(Box<Expr>),
    Binary(Box<Expr>, Op, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    source: &'a str,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            source,
        }
    }

    fn parse(mut self) -> Result<Expr, TransformError> {
        let expr = self.expr()?;
        self.skip_ws();
        if self.pos != self.chars.len() {
            return Err(self.error("trailing characters"));
        }
        Ok(expr)
    }

    fn error(&self, message: &str) -> TransformError {
        TransformError::Degenerate(format!(
            "calculator parse error at {} in {:?}: {message}",
            self.pos, self.source
        ))
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.chars.get(self.pos).copied()
    }

    fn expr(&mut self) -> Result<Expr, TransformError> {
        let mut lhs = self.term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    lhs = Expr::Binary(Box::new(lhs), Op::Add, Box::new(self.term()?));
                }
                Some('-') => {
                    self.pos += 1;
                    lhs = Expr::Binary(Box::new(lhs), Op::Sub, Box::new(self.term()?));
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn term(&mut self) -> Result<Expr, TransformError> {
        let mut lhs = self.factor()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    lhs = Expr::Binary(Box::new(lhs), Op::Mul, Box::new(self.factor()?));
                }
                Some('/') => {
                    self.pos += 1;
                    lhs = Expr::Binary(Box::new(lhs), Op::Div, Box::new(self.factor()?));
                }
                _ => return Ok(lhs),
            }
        }
    }

    fn factor(&mut self) -> Result<Expr, TransformError> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.pos += 1;
            return Ok(Expr::Binary(
                Box::new(base),
                Op::Pow,
                Box::new(self.factor()?),
            ));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Expr, TransformError> {
        if self.peek() == Some('-') {
            self.pos += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, TransformError> {
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let inner = self.expr()?;
                if self.peek() != Some(')') {
                    return Err(self.error("expected ')'"));
                }
                self.pos += 1;
                Ok(inner)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.pos;
                while self.pos < self.chars.len()
                    && (self.chars[self.pos].is_ascii_digit() || self.chars[self.pos] == '.')
                {
                    self.pos += 1;
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                text.parse::<f64>()
                    .map(Expr::Number)
                    .map_err(|_| self.error("bad number"))
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let start = self.pos;
                while self.pos < self.chars.len()
                    && (self.chars[self.pos].is_alphanumeric() || self.chars[self.pos] == '_')
                {
                    self.pos += 1;
                }
                let name: String = self.chars[start..self.pos].iter().collect();
                if self.peek() == Some('(') {
                    self.pos += 1;
                    let mut args = vec![self.expr()?];
                    while self.peek() == Some(',') {
                        self.pos += 1;
                        args.push(self.expr()?);
                    }
                    if self.peek() != Some(')') {
                        return Err(self.error("expected ')' after arguments"));
                    }
                    self.pos += 1;
                    return Ok(Expr::Call(name, args));
                }
                match name.as_str() {
                    "x" => Ok(Expr::Coord(0)),
                    "y" => Ok(Expr::Coord(1)),
                    "z" => Ok(Expr::Coord(2)),
                    _ => Ok(Expr::Field(name)),
                }
            }
            _ => Err(self.error("unexpected character")),
        }
    }
}

fn eval(
    expr: &Expr,
    fields: &[(&str, &[f64])],
    index: usize,
    point: [f64; 3],
) -> Result<f64, TransformError> {
    match expr {
        Expr::Number(v) => Ok(*v),
        Expr::Coord(axis) => Ok(point[*axis]),
        Expr::Field(name) => fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, values)| values[index])
            .ok_or_else(|| TransformError::FieldNotFound(name.clone())),
        Expr::Neg(inner) => Ok(-eval(inner, fields, index, point)?),
        Expr::Binary(a, op, b) => {
            let (x, y) = (
                eval(a, fields, index, point)?,
                eval(b, fields, index, point)?,
            );
            Ok(match op {
                Op::Add => x + y,
                Op::Sub => x - y,
                Op::Mul => x * y,
                Op::Div => x / y,
                Op::Pow => x.powf(y),
            })
        }
        Expr::Call(name, args) => {
            let values: Vec<f64> = args
                .iter()
                .map(|a| eval(a, fields, index, point))
                .collect::<Result<_, _>>()?;
            match (name.as_str(), values.len()) {
                ("sin", 1) => Ok(values[0].sin()),
                ("cos", 1) => Ok(values[0].cos()),
                ("tan", 1) => Ok(values[0].tan()),
                ("sqrt", 1) => Ok(values[0].sqrt()),
                ("exp", 1) => Ok(values[0].exp()),
                ("log", 1) => Ok(values[0].ln()),
                ("abs", 1) => Ok(values[0].abs()),
                ("min", 2) => Ok(values[0].min(values[1])),
                ("max", 2) => Ok(values[0].max(values[1])),
                _ => Err(TransformError::Degenerate(format!(
                    "unknown function '{name}' or wrong arity"
                ))),
            }
        }
    }
}

/// Summary statistics of a scalar point or cell field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldStats {
    /// Number of finite values.
    pub count: usize,
    /// Smallest finite value.
    pub min: f64,
    /// Largest finite value.
    pub max: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// Population standard deviation.
    pub std_dev: f64,
    /// Median (50th percentile).
    pub median: f64,
}

/// Computes summary statistics; non-finite values are ignored.
///
/// # Errors
///
/// Returns an error when the field is missing, not scalar, or empty.
pub fn field_statistics(
    dataset: &DataSet,
    field_name: &str,
    association: Association,
) -> Result<FieldStats, TransformError> {
    let field = dataset
        .field(association, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let values = field
        .as_scalar()
        .ok_or_else(|| TransformError::WrongFieldShape {
            field: field_name.to_string(),
            reason: "statistics require a scalar field".into(),
        })?;
    let mut finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return Err(TransformError::Degenerate(
            "field has no finite values".into(),
        ));
    }
    finite.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = finite.len();
    let mean = finite.iter().sum::<f64>() / count as f64;
    let variance = finite.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
    Ok(FieldStats {
        count,
        min: finite[0],
        max: finite[count - 1],
        mean,
        std_dev: variance.sqrt(),
        median: finite[count / 2],
    })
}

/// Histogram of a scalar field: `bins` equal-width counts over its range,
/// returned as `(bin_centre, count)` pairs.
///
/// # Errors
///
/// Returns an error when the field is missing, not scalar, or `bins` is 0.
pub fn histogram(
    dataset: &DataSet,
    field_name: &str,
    bins: usize,
) -> Result<Vec<(f64, usize)>, TransformError> {
    if bins == 0 {
        return Err(TransformError::Degenerate(
            "histogram needs bins > 0".into(),
        ));
    }
    let stats = field_statistics(dataset, field_name, Association::Point)?;
    let field = dataset
        .field(Association::Point, field_name)
        .ok_or_else(|| TransformError::FieldNotFound(field_name.to_string()))?;
    let values = field.as_scalar().unwrap_or(&[]);
    let width = (stats.max - stats.min) / bins as f64;
    let mut counts = vec![0usize; bins];
    for v in values.iter().filter(|v| v.is_finite()) {
        let index = if width == 0.0 {
            0
        } else {
            ((v - stats.min) / width).floor() as usize
        };
        counts[index.min(bins - 1)] += 1;
    }
    Ok(counts
        .iter()
        .enumerate()
        .map(|(i, &c)| (stats.min + (i as f64 + 0.5) * width, c))
        .collect())
}

/// Resamples scalar point fields onto a new structured grid by probing.
///
/// Points outside the source volume get `fill`. All scalar point fields
/// of the source are resampled.
///
/// # Errors
///
/// Returns an error when the grid cannot be built or probing fails.
pub fn resample_to_grid(
    dataset: &DataSet,
    dims: [usize; 3],
    origin: [f64; 3],
    spacing: [f64; 3],
    fill: f64,
) -> Result<DataSet, TransformError> {
    let mut grid =
        DataSet::structured_grid(format!("{}-resampled", dataset.id), dims, origin, spacing)?;
    let points: Vec<[f64; 3]> = grid.positions.iter().copied().collect();
    let names: Vec<String> = dataset
        .point_data
        .iter()
        .filter(|f| f.as_scalar().is_some())
        .map(|f| f.name.to_string())
        .collect();
    for name in names {
        let sampled = probe(dataset, &name, &points)?;
        let values: Vec<f64> = sampled.iter().map(|v| v.unwrap_or(fill)).collect();
        grid.point_data.push(Field::scalar(name, values));
    }
    Ok(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> DataSet {
        let ds = DataSet::structured_grid("g", [3, 3, 3], [0.0; 3], [1.0; 3]).unwrap();
        let values: Vec<f64> = ds.positions.iter().map(|p| p[0] + 10.0 * p[1]).collect();
        ds.with_field(Field::scalar("p", values))
    }

    #[test]
    fn calculator_evaluates_fields_and_coordinates() {
        let out = calculator(&grid(), "p * 2 + x - sqrt(y + 1)", "q").unwrap();
        let values = out
            .field(Association::Point, "q")
            .unwrap()
            .as_scalar()
            .unwrap();
        // Point (1, 2, 0): p = 21, q = 42 + 1 - sqrt(3).
        let index = out
            .positions
            .iter()
            .position(|p| *p == [1.0, 2.0, 0.0])
            .unwrap();
        assert!((values[index] - (43.0 - 3.0f64.sqrt())).abs() < 1e-12);
        calculator(&grid(), "nope + 1", "q").unwrap_err();
        calculator(&grid(), "p +", "q").unwrap_err();
    }

    #[test]
    fn statistics_and_histogram_are_exact() {
        let stats = field_statistics(&grid(), "p", Association::Point).unwrap();
        assert_eq!(stats.count, 27);
        assert_eq!(stats.min, 0.0);
        assert_eq!(stats.max, 22.0);
        let bins = histogram(&grid(), "p", 2).unwrap();
        assert_eq!(bins.iter().map(|(_, c)| c).sum::<usize>(), 27);
    }

    #[test]
    fn resample_recovers_linear_field() {
        let out = resample_to_grid(&grid(), [5, 5, 5], [0.0; 3], [0.5; 3], -1.0).unwrap();
        let values = out
            .field(Association::Point, "p")
            .unwrap()
            .as_scalar()
            .unwrap();
        for (p, v) in out.positions.iter().zip(values.iter()) {
            assert!((v - (p[0] + 10.0 * p[1])).abs() < 1e-9, "at {p:?}: {v}");
        }
    }
}
