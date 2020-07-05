use std::convert;

use crate::ast::{self, Expr, Ident, Stmt};
use crate::error::{Error, Result};
use crate::miniscript::Policy;
use crate::scope::Scope;

/// A runtime value. This is what gets passed around as function arguments, returned from functions,
/// and assigned to variables.
///
/// This can either be an evaluated miniscript `Policy` or a function.
#[derive(Debug, Clone)]
pub enum Value {
    Policy(Policy),
    Function(ast::FnDef),
    FnNative(Ident),
}

impl_from_variant!(Policy, Value);
impl_from_variant!(ast::FnDef, Value, Function);

/// Evaluate an expression. Expressions have no side-effects and return a value.
pub trait Evaluate {
    fn eval(&self, scope: &Scope) -> Result<Value>;
}

/// Execute a statement. Statements have side-effects and don't have a return value.
pub trait Execute {
    fn exec(&self, scope: &mut Scope) -> Result<()>;
}

impl Execute for ast::Assign {
    fn exec(&self, scope: &mut Scope) -> Result<()> {
        let value = self.rhs.eval(scope)?;
        scope.set(self.lhs.clone(), value)
    }
}

impl Execute for ast::FnDef {
    fn exec(&self, scope: &mut Scope) -> Result<()> {
        scope.set(self.ident.clone(), self.clone().into())
    }
}

impl Execute for Stmt {
    fn exec(&self, scope: &mut Scope) -> Result<()> {
        match self {
            Stmt::FnDef(x) => x.exec(scope),
            Stmt::Assign(x) => x.exec(scope),
        }
    }
}

impl Evaluate for ast::Call {
    fn eval(&self, scope: &Scope) -> Result<Value> {
        let val = scope
            .get(&self.ident)
            .ok_or_else(|| Error::FnNotFound(self.ident.clone()))?;

        let args = eval_exprs(scope, &self.args)?;
        Ok(match val {
            Value::Function(fn_def) => call(fn_def, args, scope)?,
            Value::FnNative(ident) => Policy::Fragment(ident.clone(), map_policy(args)?).into(),
            _ => return Err(Error::NotFn(self.ident.clone())),
        })
    }
}

impl Evaluate for ast::Or {
    fn eval(&self, scope: &Scope) -> Result<Value> {
        ast::Call {
            ident: "or".into(),
            args: self.0.clone(),
        }
        .eval(scope)
    }
}

impl Evaluate for ast::And {
    fn eval(&self, scope: &Scope) -> Result<Value> {
        ast::Call {
            ident: "and".into(),
            args: self.0.clone(),
        }
        .eval(scope)
    }
}

impl Evaluate for ast::TermWord {
    fn eval(&self, scope: &Scope) -> Result<Value> {
        Ok(match scope.get(&self.0) {
            Some(binding) => binding.clone(),
            None => Policy::TermWord(self.0.clone()).into(),
            // TODO error if a $ binding is passed through
        })
    }
}

impl Evaluate for ast::Block {
    fn eval(&self, scope: &Scope) -> Result<Value> {
        let mut scope = Scope::derive(scope);
        for stmt in &self.stmts {
            stmt.exec(&mut scope)?;
        }
        self.return_value.eval(&scope)
    }
}

impl Evaluate for Expr {
    fn eval(&self, scope: &Scope) -> Result<Value> {
        match self {
            Expr::Call(x) => x.eval(scope),
            Expr::Or(x) => x.eval(scope),
            Expr::And(x) => x.eval(scope),
            Expr::Block(x) => x.eval(scope),
            Expr::TermWord(x) => x.eval(scope),
        }
    }
}

fn call(fn_def: &ast::FnDef, args: Vec<Value>, scope: &Scope) -> Result<Value> {
    if fn_def.signature.len() != args.len() {
        return Err(Error::ArgumentMismatch(
            fn_def.ident.clone(),
            fn_def.signature.len(),
            args.len(),
        ));
    }
    let mut scope = scope.child();
    for (index, value) in args.into_iter().enumerate() {
        let ident = fn_def.signature.get(index).unwrap();
        scope.set(ident.clone(), value)?;
    }
    fn_def.body.eval(&scope)
}

impl convert::TryFrom<Value> for Policy {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Policy(policy) => Ok(policy),
            _ => Err(Error::NotMiniscriptRepresentable),
        }
    }
}

impl Value {
    pub fn into_policy(self) -> Result<Policy> {
        convert::TryInto::try_into(self)
    }
}

fn eval_exprs(scope: &Scope, list: &Vec<Expr>) -> Result<Vec<Value>> {
    list.iter().map(|arg| arg.eval(scope)).collect()
}

fn map_policy(list: Vec<Value>) -> Result<Vec<Policy>> {
    list.into_iter().map(Value::into_policy).collect()
}
