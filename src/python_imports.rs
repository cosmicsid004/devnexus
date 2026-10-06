use ruff_python_ast::visitor::{walk_stmt, Visitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_parser::parse_module;

#[derive(Debug, Clone)]
pub struct PyImport {
    // Top level module 'import a.b.c' -> "a"
    pub name: String,
    pub line: usize,

    // Inside try/except block
    pub optional: bool,

    // Inside if TYPE_CHECKING
    pub typing_only: bool,
}

struct Collector<'s> {
    src: &'s str,
    out: Vec<PyImport>,
    optional_depth: u32,
    typing_depth: u32,
} 

impl<'s> Collector<'s> {
    fn push(&mut self, module: &str, offset: usize) {
        let top = module.split('.').next().unwrap_or(module);
        if top.is_empty() {
            return;
        }

        // byte offset -> line number
        let line = self.src.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() + 1;

        self.out.push(PyImport { 
            name: top.to_string(), 
            line, 
            optional: self.optional_depth > 0, 
            typing_only: self.typing_depth > 0, 
        });
    }
}

impl<'a, 's> Visitor<'a> for Collector<'s> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            //import os, numpy as np
            Stmt::Import(node) => {
                let offset = usize::from(node.range.start());
                for alias in &node.names {
                    self.push(alias.name.as_str(), offset);
                }
            }

            // from requests import get
            Stmt::ImportFrom(node) => {
                // level > 0 means relative (`from . import x`)
                if node.level == 0 {
                    if let Some(module) = &node.module {
                        self.push(module.as_str(), usize::from(node.range.start()));
                    }
                }
            }

            // try: except ImportError
            Stmt::Try(t) => {
                self.optional_depth += 1;
                self.visit_body(&t.body);
                self.optional_depth -= 1;
                for handler in &t.handlers {
                    self.visit_except_handler(handler);
                }
                self.visit_body(&t.orelse);
                self.visit_body(&t.finalbody);
            }

            // if TYPE_CHECKING
            Stmt::If(i) if is_type_checking(&i.test) => {
                self.typing_depth += 1;
                self.visit_body(&i.body);
                self.typing_depth -= 1;
                for clause in &i.elif_else_clauses {
                    self.visit_body(&clause.body);
                }
            }

            // everything else 
            _ => walk_stmt(self, stmt),
        }
    }
}

fn is_type_checking(test: &Expr) -> bool {
    match test {
        Expr::Name(n) => n.id.as_str() == "TYPE_CHECKING",
        // typing.TYPE_CHECKING
        Expr::Attribute(a) => a.attr.as_str() == "TYPE_CHECKING",
        _ => false,
    }
}

pub fn extract(src: &str) -> Result<Vec<PyImport>, String> {
    let parsed = parse_module(src).map_err(|e| e.to_string())?;
    let mut collector = Collector {
        src,
        out: Vec::new(),
        optional_depth: 0,
        typing_depth: 0,
    };
    for stmt in &parsed.syntax().body {
        collector.visit_stmt(stmt);
    }
    Ok(collector.out)
}