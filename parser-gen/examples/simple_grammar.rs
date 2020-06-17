use clap::{App, Arg};
use std::{io, fs};
use common::{grammar::*, parse_arrow_prod};
use parser_gen::{show_lr, show_ll};
use lalr1_core::*;
use common::{IndexMap, HashSet};

fn parse_lines(s: &str) -> Result<RawGrammar, String> {
  let mut production = Vec::new();
  let mut all_lhs = HashSet::new();
  for s in s.lines() {
    let (lhs, rhs) = parse_arrow_prod(s).ok_or_else(|| format!("invalid input \"{}\", expect form of \"lhs -> rhs1 rhs2 ...\"", s))?;
    all_lhs.insert(lhs.clone());
    production.push(RawProduction { lhs, type_: String::new(), rhs: vec![RawProductionRhs { rhs, rhs_arg: None, act: String::new(), prec: None }] });
  }
  let start = production.get(0).ok_or_else(|| "grammar must have at least one production rule".to_owned())?.lhs.clone();
  let mut lexical = IndexMap::default();
  for p in &production {
    for r in &p.rhs {
      for r in &r.rhs {
        if !all_lhs.contains(r.as_str()) {
          // use current len as a unique id (key will be used regex)
          lexical.insert(lexical.len().to_string(), r.clone());
        }
      }
    }
  }
  Ok(RawGrammar { include: String::new(), priority: vec![], lexical, parser_field: None, start, production, parser_def: None })
}

fn main() -> io::Result<()> {
  let m = App::new("simple_grammar")
    .arg(Arg::with_name("input").required(true))
    .arg(Arg::with_name("output").long("output").short("o").takes_value(true).required(true))
    .arg(Arg::with_name("grammar").long("grammar").short("g").takes_value(true).possible_values(&["lr0", "lr1", "lalr1", "ll1"]).required(true))
    .get_matches();
  let input = fs::read_to_string(m.value_of("input").unwrap())?;
  let mut raw = parse_lines(&input).unwrap_or_else(|e| panic!("input is invalid: {}", e));
  let ref g = raw.extend(false).unwrap(); // it should not fail
  let result = match m.value_of("grammar") {
    Some("lr0") => show_lr::lr0_dot(g, &lr0::work(g)),
    Some("lr1") => show_lr::lr1_dot(g, &lr1::work(g)),
    Some("lalr1") => show_lr::lr1_dot(g, &lalr1_by_lr0::work(lr0::work(g), g)),
    Some("ll1") => show_ll::table(&ll1_core::LLCtx::new(g), g),
    _ => unreachable!(),
  };
  fs::write(m.value_of("output").unwrap(), result.replace("_Eof", "#"))
}