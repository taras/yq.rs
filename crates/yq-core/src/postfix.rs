//! `expression_postfix.go` and `expression_postfix_if_validator.go`: shunting
//! yard from tokens to postfix operations, with the if/then/else checks.

use std::collections::HashMap;

use crate::lexer::{Token, TokenType};
use crate::operation::{OpType, Operation, Prefs, TraversePrefs};
use crate::Error;

struct StackEntry {
    token: Token,
    id: usize,
}

fn validate_no_open_tokens(token: &Token) -> Result<(), Error> {
    match token.token_type {
        TokenType::OpenCollect => Err(Error(
            "bad expression, could not find matching `]`".to_string(),
        )),
        TokenType::OpenCollectObject => Err(Error(
            "bad expression, could not find matching `}`".to_string(),
        )),
        TokenType::OpenBracket => {
            let closer = if token.expected_closer.is_empty() {
                ")"
            } else {
                token.expected_closer.as_str()
            };
            Err(Error(format!(
                "bad expression, could not find matching `{closer}`"
            )))
        }
        _ => Ok(()),
    }
}

struct IfValidator {
    last_keyword: HashMap<usize, String>,
}

impl IfValidator {
    fn on_open(&mut self, entry: &StackEntry) {
        if entry.token.match_text == "if" {
            self.last_keyword.insert(entry.id, "if".to_string());
        }
    }

    fn on_keyword(&mut self, stack: &[StackEntry], token: &Token) -> Result<(), Error> {
        if !token.is_op(OpType::IfThen) && !token.is_op(OpType::IfElse) {
            return Ok(());
        }
        let keyword = token
            .operation
            .as_ref()
            .map(|op| op.string_value.clone())
            .unwrap_or_default();
        let enclosing = stack
            .iter()
            .rev()
            .find(|entry| entry.token.token_type != TokenType::Operation);
        let if_entry = match enclosing {
            Some(entry)
                if entry.token.token_type == TokenType::OpenBracket
                    && entry.token.match_text == "if" =>
            {
                entry
            }
            _ => {
                return Err(Error(format!(
                    "bad expression, `{keyword}` without matching `if`"
                )))
            }
        };
        let previous = self
            .last_keyword
            .get(&if_entry.id)
            .cloned()
            .unwrap_or_default();
        if keyword == "then" && previous != "if" && previous != "elif" {
            return Err(Error(
                "bad expression, `then` must follow `if` or `elif`".to_string(),
            ));
        }
        if keyword != "then" && previous != "then" {
            return Err(Error(format!(
                "bad expression, `{keyword}` must follow `then`"
            )));
        }
        self.last_keyword.insert(if_entry.id, keyword);
        Ok(())
    }

    fn on_close(&self, opener: &StackEntry, closer: &Token) -> Result<(), Error> {
        let opener_is_if = opener.token.match_text == "if";
        let closer_is_end = closer.match_text == "end";
        if !opener_is_if && !closer_is_end {
            return Ok(());
        }
        if !opener_is_if {
            return Err(Error(
                "bad expression, got `end` without matching `if`".to_string(),
            ));
        }
        if !closer_is_end {
            return Err(Error(
                "bad expression, could not find matching `end`".to_string(),
            ));
        }
        let previous = self
            .last_keyword
            .get(&opener.id)
            .cloned()
            .unwrap_or_default();
        if previous != "then" && previous != "else" {
            return Err(Error(format!(
                "bad expression, `{previous}` must be followed by `then`"
            )));
        }
        Ok(())
    }
}

fn pop_to_result(stack: &mut Vec<StackEntry>, result: &mut Vec<Operation>) {
    if let Some(entry) = stack.pop() {
        if let Some(op) = entry.token.operation {
            result.push(op);
        }
    }
}

pub fn convert_to_postfix(infix: Vec<Token>) -> Result<Vec<Operation>, Error> {
    let mut result: Vec<Operation> = Vec::new();
    let mut next_id = 0usize;
    let mut new_entry = |token: Token| {
        next_id += 1;
        StackEntry { token, id: next_id }
    };
    let mut stack: Vec<StackEntry> = vec![new_entry(Token {
        token_type: TokenType::OpenBracket,
        operation: None,
        assign_operation: None,
        check_for_post_traverse: false,
        match_text: String::new(),
        expected_closer: String::new(),
    })];
    let mut tokens = infix;
    tokens.push(Token {
        token_type: TokenType::CloseBracket,
        operation: None,
        assign_operation: None,
        check_for_post_traverse: true,
        match_text: String::new(),
        expected_closer: String::new(),
    });
    let mut validator = IfValidator {
        last_keyword: HashMap::new(),
    };

    for current in tokens {
        match current.token_type {
            TokenType::OpenBracket | TokenType::OpenCollect | TokenType::OpenCollectObject => {
                let entry = new_entry(current);
                validator.on_open(&entry);
                stack.push(entry);
            }
            TokenType::CloseCollect | TokenType::CloseCollectObject => {
                let (opener, collect_op) = if current.token_type == TokenType::CloseCollectObject {
                    (TokenType::OpenCollectObject, OpType::CollectObject)
                } else {
                    (TokenType::OpenCollect, OpType::Collect)
                };
                while stack
                    .last()
                    .is_some_and(|entry| entry.token.token_type != opener)
                {
                    if let Some(top) = stack.last() {
                        validate_no_open_tokens(&top.token)?;
                    }
                    pop_to_result(&mut stack, &mut result);
                }
                if stack.is_empty() {
                    return Err(Error(
                        "bad path expression, got close collect brackets without matching opening bracket".to_string(),
                    ));
                }
                stack.pop();
                let mut prefs = TraversePrefs::default();
                if current.match_text.ends_with('?') {
                    prefs.optional_traverse = true;
                }
                result.push(Operation::new(collect_op));
                if opener != TokenType::OpenCollect {
                    result.push(Operation::new(OpType::ShortPipe));
                }
                let top_is_traverse_array = stack
                    .last()
                    .is_some_and(|entry| entry.token.is_op(OpType::TraverseArray));
                if top_is_traverse_array {
                    if let Some(entry) = stack.last_mut() {
                        if let Some(op) = entry.token.operation.as_mut() {
                            op.prefs = Prefs::Traverse(prefs);
                        }
                    }
                    pop_to_result(&mut stack, &mut result);
                }
            }
            TokenType::CloseBracket => {
                while stack
                    .last()
                    .is_some_and(|entry| entry.token.token_type != TokenType::OpenBracket)
                {
                    if let Some(top) = stack.last() {
                        validate_no_open_tokens(&top.token)?;
                    }
                    pop_to_result(&mut stack, &mut result);
                }
                let opener =
                    match stack.last() {
                        Some(opener) => opener,
                        None => return Err(Error(
                            "bad expression, got close brackets without matching opening bracket"
                                .to_string(),
                        )),
                    };
                validator.on_close(opener, &current)?;
                stack.pop();
            }
            _ => {
                validator.on_keyword(&stack, &current)?;
                let precedence = current
                    .operation
                    .as_ref()
                    .map(|op| op.op_type.info().precedence)
                    .unwrap_or(0);
                while stack.last().is_some_and(|entry| {
                    entry.token.token_type == TokenType::Operation
                        && entry
                            .token
                            .operation
                            .as_ref()
                            .is_some_and(|op| op.op_type.info().precedence > precedence)
                }) {
                    pop_to_result(&mut stack, &mut result);
                }
                stack.push(new_entry(current));
            }
        }
    }

    if let Some(top) = stack.last() {
        return Err(Error(format!(
            "bad expression - probably missing close bracket on {}",
            top.token.describe()
        )));
    }
    Ok(result)
}
