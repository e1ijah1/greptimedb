// Copyright 2022 Greptime Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use sqlparser::ast::{ObjectName, SetExpr, Statement, UnaryOperator, Values};
use sqlparser::parser::ParserError;

use crate::ast::{Expr, Value};
use crate::error::{self, Result};
use crate::statements::table_idents_to_full_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insert {
    // Can only be sqlparser::ast::Statement::Insert variant
    pub inner: Statement,
}

impl Insert {
    pub fn full_table_name(&self) -> Result<(String, String, String)> {
        match &self.inner {
            Statement::Insert { table_name, .. } => table_idents_to_full_name(table_name),
            _ => unreachable!(),
        }
    }

    pub fn table_name(&self) -> &ObjectName {
        match &self.inner {
            Statement::Insert { table_name, .. } => table_name,
            _ => unreachable!(),
        }
    }

    pub fn columns(&self) -> Vec<&String> {
        match &self.inner {
            Statement::Insert { columns, .. } => columns.iter().map(|ident| &ident.value).collect(),
            _ => unreachable!(),
        }
    }

    pub fn values(&self) -> Result<Vec<Vec<Value>>> {
        let values = match &self.inner {
            Statement::Insert { source, .. } => match &source.body {
                SetExpr::Values(Values(exprs)) => sql_exprs_to_values(exprs)?,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        Ok(values)
    }
}

fn sql_exprs_to_values(exprs: &Vec<Vec<Expr>>) -> Result<Vec<Vec<Value>>> {
    let mut values = Vec::with_capacity(exprs.len());
    for es in exprs.iter() {
        let mut vs = Vec::with_capacity(es.len());
        for expr in es.iter() {
            vs.push(match expr {
                Expr::Value(v) => v.clone(),
                Expr::Identifier(ident) => Value::SingleQuotedString(ident.value.clone()),
                Expr::UnaryOp { op, expr }
                    if matches!(op, UnaryOperator::Minus | UnaryOperator::Plus) =>
                {
                    if let Expr::Value(Value::Number(s, b)) = &**expr {
                        match op {
                            UnaryOperator::Minus => Value::Number(format!("-{}", s), *b),
                            UnaryOperator::Plus => Value::Number(s.to_string(), *b),
                            _ => unreachable!(),
                        }
                    } else {
                        return error::ParseSqlValueSnafu {
                            msg: format!("{:?}", expr),
                        }
                        .fail();
                    }
                }
                _ => {
                    return error::ParseSqlValueSnafu {
                        msg: format!("{:?}", expr),
                    }
                    .fail()
                }
            });
        }
        values.push(vs);
    }
    Ok(values)
}

impl TryFrom<Statement> for Insert {
    type Error = ParserError;

    fn try_from(value: Statement) -> std::result::Result<Self, Self::Error> {
        match value {
            Statement::Insert { .. } => Ok(Insert { inner: value }),
            unexp => Err(ParserError::ParserError(format!(
                "Not expected to be {}",
                unexp
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlparser::dialect::GenericDialect;

    use super::*;
    use crate::parser::ParserContext;

    #[test]
    fn test_insert_value_with_unary_op() {
        use crate::statements::statement::Statement;

        // insert "-1"
        let sql = "INSERT INTO my_table VALUES(-1)";
        let stmt = ParserContext::create_with_dialect(sql, &GenericDialect {})
            .unwrap()
            .remove(0);
        match stmt {
            Statement::Insert(insert) => {
                let values = insert.values().unwrap();
                assert_eq!(values, vec![vec![Value::Number("-1".to_string(), false)]]);
            }
            _ => unreachable!(),
        }

        // insert "+1"
        let sql = "INSERT INTO my_table VALUES(+1)";
        let stmt = ParserContext::create_with_dialect(sql, &GenericDialect {})
            .unwrap()
            .remove(0);
        match stmt {
            Statement::Insert(insert) => {
                let values = insert.values().unwrap();
                assert_eq!(values, vec![vec![Value::Number("1".to_string(), false)]]);
            }
            _ => unreachable!(),
        }
    }
}
