use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use regex::Regex;
use crate::grammar::{Grammar, ProdVec};

#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Assoc {
  Left,
  Right,
  NoAssoc,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawGrammar {
  pub include: String,
  pub lexer_field_ext: Option<Vec<RawLexerFieldExt>>,
  pub terminal: Vec<RawTerminalRow>,
  pub lexical: Vec<RawLexicalRule>,
  //                (nt    , type  )
  pub start: Option<(String, String)>,
  pub production: Vec<RawProduction>,
}

const EPS: &'static str = "_Eps";
const EOF: &'static str = "_Eof";
const INITIAL: &'static str = "_Initial";


impl RawGrammar {
  // will add a production _Start -> Start, so need mut
  pub fn to_grammar(&mut self) -> Result<Grammar, String> {
    // don't allow '_' to be the first char
    let valid_name = regex::Regex::new("^[a-zA-Z][a-zA-Z_0-9]*$").unwrap();
    let mut terminal = vec![(EPS, None), (EOF, None)];
    let mut terminal2id = HashMap::new();
    terminal2id.insert(EPS, 0);
    terminal2id.insert(EOF, 1);
    let mut lex_state = vec![INITIAL];
    let mut lex_state2id = HashMap::new();
    lex_state2id.insert(INITIAL, 0);
    let mut lex = Vec::new();
    let mut nt = Vec::new();
    let mut nt2id = HashMap::new();

    for (pri, term_row) in self.terminal.iter().enumerate() {
      let pri_assoc = term_row.assoc.map(|assoc| (pri as u32, assoc));
      for term in term_row.tokens.iter().map(String::as_str) {
        if term == EPS {
          return Err(format!("Terminal cannot have the builtin name `{}`.", EPS));
        } else if term == EOF {
          return Err(format!("Terminal cannot have the builtin name `{}`.", EOF));
        } else if !valid_name.is_match(term) {
          return Err(format!("Terminal is not a valid variable name: `{}`.", term));
        } else if terminal2id.contains_key(term) {
          return Err(format!("Find duplicate token: `{}`.", term));
        } else {
          terminal2id.insert(term, terminal.len() as u32);
          terminal.push((term, pri_assoc));
        }
      }
    }

    for lexical in &self.lexical {
      let re = if lexical.escape { regex::escape(&lexical.re) } else { lexical.re.clone() };
      if let Err(err) = Regex::new(&re) {
        return Err(format!("Error regex: `{}`, reason: {}.", lexical.re, err));
      } else {
        let id = *lex_state2id.entry(lexical.state.as_str()).or_insert_with(|| {
          let id = lex_state.len() as u32;
          lex_state.push(lexical.state.as_str());
          id
        }) as usize;
        if lex.len() < id + 1 {
          lex.resize_with(id + 1, || Vec::new());
        }
        let term = lexical.term.as_str();
        if term != EOF && term != EPS && !valid_name.is_match(term) {
          return Err(format!("Terminal is not a valid variable name: `{}`.", term));
        }
        terminal2id.entry(term).or_insert_with(|| {
          let id = terminal.len() as u32;
          terminal.push((term, None));
          id
        });
        lex[id].push((re, lexical.act.as_str(), term));
      }
    }

    if self.production.is_empty() {
      return Err("Grammar must have at least one production rule.".into());
    }

    // 2 pass scan, so a terminal can be used before declared

    // getting production must be after this mut operation
    // this may seem stupid...
    {
      let start = self.start.clone().unwrap_or_else(|| (self.production[0].lhs.clone(), self.production[0].type_.clone()));
      self.production.push(RawProduction {
        lhs: format!("_{}", start.0),
        type_: start.1,
        rhs: vec![RawProductionRhs {
          rhs: start.0,
          act: "let _0 = _1;".into(),
          prec: None,
        }],
      });
    }

    for raw in &self.production {
      let lhs = raw.lhs.as_str();
      // again this may seem stupid...
      // self.production.last().unwrap().lhs is generated by the code above
      if !valid_name.is_match(lhs) && lhs != &self.production.last().unwrap().lhs {
        return Err(format!("Non-terminal is not a valid variable name: `{}`.", lhs));
      } else if terminal2id.contains_key(lhs) {
        return Err(format!("Non-terminal has a duplicate name with terminal: `{}`.", lhs));
      } else {
        nt2id.entry(lhs).or_insert_with(|| {
          let id = nt.len() as u32;
          nt.push((lhs, raw.type_.as_str()));
          id
        });
      }
    }

    let mut prod = vec![Vec::new(); nt.len()];
    let mut prod_extra = Vec::new();
    let mut prod_id = 0u32;

    for raw in &self.production {
      let lhs = nt2id.get(raw.lhs.as_str()).unwrap();
      let lhs_prod = &mut prod[*lhs as usize];
      for rhs in &raw.rhs {
        let mut prod_rhs = ProdVec::new();
        let mut pri_assoc = None;
        for rhs in rhs.rhs.split_whitespace() {
          // impossible to have a (Some(), Some()) here
          match (nt2id.get(rhs), terminal2id.get(rhs)) {
            (Some(&nt), _) => prod_rhs.push(nt),
            (_, Some(&t)) => {
              prod_rhs.push(t + nt.len() as u32);
              pri_assoc = terminal[t as usize].1;
            }
            _ => return Err(format!("Production rhs contains undefined item: `{}`", rhs)),
          }
        }
        if let Some(prec) = rhs.prec.as_ref() {
          match terminal2id.get(prec.as_str()) {
            None => return Err(format!("Prec uses undefined terminal: `{}`", prec)),
            Some(&t) => {
              pri_assoc = terminal[t as usize].1;
            }
          }
        }
        let id = lhs_prod.len() as u32;
        lhs_prod.push((prod_rhs, prod_id));
        prod_extra.push((rhs.act.as_str(), (*lhs, id), pri_assoc));
        prod_id += 1;
      }
    }

    Ok(Grammar {
      raw: self,
      nt,
      terminal,
      lex_state,
      lex,
      prod,
      prod_extra,
    })
  }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawTerminalRow {
  pub assoc: Option<Assoc>,
  pub tokens: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawLexerFieldExt {
  pub field: String,
  #[serde(rename = "type")]
  pub type_: String,
  pub init: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawLexicalRule {
  #[serde(default = "default_state")]
  pub state: String,
  pub re: String,
  #[serde(default = "default_act")]
  pub act: String,
  // the terminal name that this lex rule returns
  // will be extracted and add to terminal list(no need to declare)
  pub term: String,
  // whether use regex::escape to modify the pattern string
  // in most case, yes(like "+"); if it is "real" regex, no(like "[0-9]")
  #[serde(default = "default_escape")]
  pub escape: bool,
}

fn default_state() -> String {
  INITIAL.into()
}

fn default_act() -> String {
  "".into()
}

fn default_escape() -> bool {
  true
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawProduction {
  pub lhs: String,
  #[serde(rename = "type")]
  pub type_: String,
  pub rhs: Vec<RawProductionRhs>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawProductionRhs {
  pub rhs: String,
  pub act: String,
  pub prec: Option<String>,
}