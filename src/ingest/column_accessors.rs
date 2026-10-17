//! A model's own accessor for one of its columns (`def title=(t)`,
//! `def body`) is the one Rails calls, and its `super` is the attribute:
//! ActiveRecord generates the column's accessors in a module the class
//! includes. Written here as `self[:title] = value` / `self[:title]`,
//! before the lowerings: a bare `super` in a writer passes the
//! parameter's value at that point, and `lower::param_rebind` renames a
//! reassigned parameter's later reads, this one among them.

use crate::App;
use crate::dialect::{MethodReceiver, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{Symbol, VarId};

pub(super) fn rewrite_super(app: &mut App) {
    for model in &mut app.models {
        let Some(table) = app.schema.tables.get(&model.table.0) else { continue };
        let columns: Vec<String> = table.columns.iter().map(|c| c.name.as_str().to_string()).collect();
        for item in &mut model.body {
            let ModelBodyItem::Method { method, .. } = item else { continue };
            if method.receiver != MethodReceiver::Instance || method.name_span.is_synthetic() {
                continue;
            }
            let name = method.name.as_str();
            let (column, writer) = match name.strip_suffix('=') {
                Some(c) => (c, true),
                None => (name, false),
            };
            if !columns.iter().any(|c| c == column) || (writer && method.params.len() != 1) || (!writer && !method.params.is_empty()) {
                continue;
            }
            let column = column.to_string();
            let param = method.params.first().map(|p| p.name.clone());
            super_as_column(&mut method.body, &column, writer, param.as_ref());
        }
    }
}

fn super_as_column(e: &mut Expr, column: &str, writer: bool, param: Option<&Symbol>) {
    e.node.for_each_child_mut(&mut |c| super_as_column(c, column, writer, param));
    let ExprNode::Super { args } = &*e.node else { return };
    let span = e.span;
    let key = Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: Symbol::from(column) } });
    let self_ref = Expr::new(span, ExprNode::SelfRef);
    let node = if writer {
        let value = match args {
            Some(args) if args.len() == 1 => args[0].clone(),
            None => match param {
                Some(p) => Expr::new(span, ExprNode::Var { id: VarId(0), name: p.clone() }),
                None => return,
            },
            _ => return,
        };
        ExprNode::Send { recv: Some(self_ref), method: Symbol::from("[]="), args: vec![key, value], block: None, parenthesized: false }
    } else {
        if args.as_ref().is_some_and(|a| !a.is_empty()) {
            return;
        }
        ExprNode::Send { recv: Some(self_ref), method: Symbol::from("[]"), args: vec![key], block: None, parenthesized: false }
    };
    *e = Expr::new(span, node);
}
