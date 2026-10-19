use syn::visit::Visit; // to walk the syntax tree
use syn::{ItemEnum, ItemExternCrate, ItemMod, ItemUse, UseTree};

// RustImport { name: "serde", line: 1 }
// RustImport { name: "tokio", line: 2 }
#[derive(Debug, Clone)]
pub struct RustImport {
    pub name: String,
    pub line: usize,
}

// thing we got form 1 file
#[derive(Debug, Default)]
pub struct Extracted {
    pub imports: Vec<RustImport>,
    pub modules: Vec<String>, // local modules
}

struct Collector {
    out: Extracted,
}

// Implements the Visit trait for Collector so it can traverse the AST.
impl<'ast> Visit<'ast> for Collector {
    // ItemUse: The type representing the use statement.
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        collect_use_tree(&node.tree, &mut self.out.imports);
        // Keep walking so we visit nested things
        syn::visit::visit_item_use(self, node);
    }

    // ItemExternCrate: External crate declaration
    fn visit_item_extern_crate(&mut self, node: &'ast ItemExternCrate) {
        self.out.imports.push(RustImport {
            name: node.ident.to_string(),
            line: node.ident.span().start().line,
        });

        syn::visit::visit_item_extern_crate(self, node);
    }

    // collecting module and enum names
    // ItemMod: Module declaration
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        // points to another file and inline mod foo { }
        self.out.modules.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
    }

    // collecting: enum Direction { North, South }
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        // use Direction::* : is a local enum, not a crate
        self.out.modules.push(node.ident.to_string());
        syn::visit::visit_item_enum(self, node);
    }
}

fn collect_use_tree(tree: &UseTree, out: &mut Vec<RustImport>) {
    match tree {
        // use serde::Deserialize; -> serde
        UseTree::Path(p) => out.push(RustImport {
            name: p.ident.to_string(),
            line: p.ident.span().start().line,
        }),

        // use serde; -> serde
        UseTree::Name(n) => out.push(RustImport {
            name: n.ident.to_string(),
            line: n.ident.span().start().line,
        }),

        // use serde as s; -> serde
        UseTree::Rename(r) => out.push(RustImport {
            name: r.ident.to_string(),
            line: r.ident.span().start().line,
        }),

        // use {serde::Serialize, tokio::spaw}; -> recurse into each item
        UseTree::Group(g) => {
            for item in &g.items {
                collect_use_tree(item, out);
            }
        }

        // use *; -> nothing useful
        UseTree::Glob(_) => {}
    }
}

pub fn extract(src: &str) -> Result<Extracted, syn::Error> {
    let file = syn::parse_file(src)?;
    let mut collector = Collector {
        out: Extracted::default(),
    };
    collector.visit_file(&file);
    Ok(collector.out)
}
