//! Result printing, the port of `printer.go`: document separators, leading
//! content and the per-format encoders.

use crate::json;
use crate::node::{Arena, NodeId};
use crate::yaml;
use crate::yaml::decode::DOC_SEPARATOR_MARKER;
use crate::{Error, Format};

#[derive(Debug, Clone, Copy)]
pub struct PrinterPrefs {
    pub format: Format,
    pub indent: usize,
    pub unwrap_scalar: bool,
    pub print_doc_separators: bool,
}

pub struct Printer {
    prefs: PrinterPrefs,
    out: String,
    first_time_printing: bool,
    previous_doc_index: usize,
    previous_file_index: usize,
    printed_matches: bool,
}

impl Printer {
    pub fn new(prefs: PrinterPrefs) -> Printer {
        Printer {
            prefs,
            out: String::new(),
            first_time_printing: true,
            previous_doc_index: 0,
            previous_file_index: 0,
            printed_matches: false,
        }
    }

    pub fn into_output(self) -> String {
        self.out
    }

    pub fn print_results(&mut self, arena: &mut Arena, nodes: &[NodeId]) -> Result<(), Error> {
        for id in nodes {
            self.print_node(arena, *id)?;
        }
        Ok(())
    }

    pub fn print_node(&mut self, arena: &mut Arena, id: NodeId) -> Result<(), Error> {
        let yaml_prefs = yaml::encode::YamlPrefs {
            indent: self.prefs.indent,
            unwrap_scalar: self.prefs.unwrap_scalar,
            print_doc_separators: self.prefs.print_doc_separators,
            compact_sequence_indent: false,
        };
        if self.first_time_printing {
            self.previous_doc_index = arena.get_document(id);
            self.previous_file_index = arena.get_file_index(id);
            self.first_time_printing = false;
        }
        let document = arena.get_document(id);
        let file_index = arena.get_file_index(id);
        let leading = arena.get(id).leading_content.clone();
        let comment_starts_with_separator = leading.starts_with(DOC_SEPARATOR_MARKER);
        if self.prefs.format == Format::Yaml {
            if (self.previous_doc_index != document || self.previous_file_index != file_index)
                && !comment_starts_with_separator
            {
                self.out
                    .push_str(yaml::encode::document_separator(&yaml_prefs));
            }
            self.out.push_str(&yaml::encode::leading_content(
                &leading,
                self.prefs.print_doc_separators,
            ));
        }
        if self.prefs.format == Format::Json {
            // The JSON encoder cannot represent aliases, so yq explodes first.
            crate::ops::meta::explode_node(arena, id, false)?;
        }
        let node = arena.get(id);
        self.printed_matches = self.printed_matches
            || (node.tag != "!!null" && (node.tag != "!!bool" || node.value != "false"));
        let text = match self.prefs.format {
            Format::Yaml => yaml::encode::encode(arena, id, &yaml_prefs)?,
            Format::Json => {
                let json_prefs = json::encode::JsonPrefs {
                    indent: self.prefs.indent,
                    unwrap_scalar: self.prefs.unwrap_scalar,
                };
                json::encode::encode(arena, id, &json_prefs)?
            }
        };
        self.out.push_str(&text);
        self.previous_doc_index = document;
        Ok(())
    }
}
