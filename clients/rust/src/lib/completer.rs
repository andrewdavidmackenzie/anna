//! Tab-completion for anna CLI commands.

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

/// Commands available in the anna interactive CLI.
pub const ANNA_COMMANDS: &[&str] = &[
    "GET",
    "PUT",
    "GET_SET",
    "PUT_SET",
    "GET_ORDERED_SET",
    "PUT_ORDERED_SET",
    "GET_CAUSAL",
    "PUT_CAUSAL",
    "GET_SINGLE_CAUSAL",
    "PUT_SINGLE_CAUSAL",
    "GET_PRIORITY",
    "PUT_PRIORITY",
    "START",
    "STOP",
    "STATUS",
    "HELP",
    "DELETE",
    "EXIT",
];

/// Provides tab-completion for anna CLI commands.
pub struct AnnaCompleter;

impl Completer for AnnaCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = &line[..pos].to_ascii_uppercase();
        if prefix.contains(' ') {
            return Ok((pos, vec![]));
        }
        let matches: Vec<Pair> = ANNA_COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(prefix.as_str()))
            .map(|cmd| Pair {
                display: cmd.to_string(),
                replacement: cmd.to_string(),
            })
            .collect();
        Ok((0, matches))
    }
}

impl Hinter for AnnaCompleter {
    type Hint = String;
}
impl Highlighter for AnnaCompleter {}
impl Validator for AnnaCompleter {}
impl Helper for AnnaCompleter {}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(input: &str) -> Vec<String> {
        let completer = AnnaCompleter;
        let (_, pairs) = completer
            .complete(input, input.len(), &Context::new(&rustyline::history::DefaultHistory::new()))
            .unwrap();
        pairs.into_iter().map(|p| p.display).collect()
    }

    #[test]
    fn completes_get() {
        let results = complete("GE");
        assert!(results.contains(&"GET".to_string()));
        assert!(results.contains(&"GET_SET".to_string()));
        assert!(results.contains(&"GET_CAUSAL".to_string()));
    }

    #[test]
    fn completes_put() {
        let results = complete("PU");
        assert!(results.contains(&"PUT".to_string()));
        assert!(results.contains(&"PUT_SET".to_string()));
    }

    #[test]
    fn completes_case_insensitive() {
        let results = complete("ge");
        assert!(results.contains(&"GET".to_string()));
    }

    #[test]
    fn no_completion_after_space() {
        let results = complete("GET ");
        assert!(results.is_empty());
    }

    #[test]
    fn empty_prefix_returns_all() {
        let results = complete("");
        assert_eq!(results.len(), ANNA_COMMANDS.len());
    }

    #[test]
    fn exact_match() {
        let results = complete("EXIT");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "EXIT");
    }
}
