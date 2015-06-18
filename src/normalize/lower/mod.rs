/*!
 * Lower
 */

use intern::{self, intern, InternedString};
use normalize::NormResult;
use normalize::norm_util::{self, Symbols};
use grammar::parse_tree as pt;
use grammar::repr as r;
use std::collections::HashMap;
use util::Sep;

pub fn lower(grammar: pt::Grammar, types: r::Types) -> NormResult<r::Grammar> {
    let mut state = LowerState::new(types);
    state.lower(grammar)
}

struct LowerState {
    grammar: r::Grammar
}

impl LowerState {
    fn new(types: r::Types) -> LowerState {
        LowerState {
            grammar: r::Grammar {
                action_fn_defns: vec![],
                productions: vec![],
                conversions: HashMap::new(),
                types: types
            }
        }
    }

    fn lower(mut self, grammar: pt::Grammar) -> NormResult<r::Grammar> {
        for item in grammar.items {
            match item {
                pt::GrammarItem::TokenType(data) => {
                    self.grammar.conversions.extend(data.conversions);
                }

                pt::GrammarItem::Nonterminal(nt) => {
                    for alt in nt.alternatives {
                        let nt_type = self.grammar.types.nonterminal_type(nt.name).clone();
                        let symbols = self.symbols(&alt.expr.symbols);
                        let action_fn = self.action_fn(nt_type, &alt.expr, &symbols, alt.action);
                        let production = r::Production {
                            span: alt.span,
                            nonterminal: nt.name,
                            symbols: symbols,
                            action_fn: action_fn,
                        };
                        self.grammar.productions.push(production);
                    }
                }
            }
        }

        Ok(self.grammar)
    }

    fn action_fn(&mut self,
                 nt_type: r::TypeRepr,
                 expr: &pt::ExprSymbol,
                 symbols: &[r::Symbol],
                 action: Option<String>)
                 -> r::ActionFn
    {
        let action = match action {
            Some(s) => s,
            None => format!("(~~)"),
        };

        // Note that the action fn takes ALL of the symbols in `expr`
        // as arguments, and some of them are simply dropped based on
        // the user's selections.

        // The set of argument types is thus the type of all symbols:
        let arg_types: Vec<r::TypeRepr> =
            symbols.iter().map(|s| s.ty(&self.grammar.types)).cloned().collect();

        let action_fn_defn = match norm_util::analyze_expr(expr) {
            Symbols::Named(names) => {
                // if there are named symbols, we want to give the
                // arguments the names that the user gave them:
                let arg_patterns =
                    patterns(names.iter().map(|&(index, name, _)| (index, name)),
                             symbols.len());

                r::ActionFnDefn {
                    arg_patterns: arg_patterns,
                    arg_types: arg_types,
                    ret_type: nt_type,
                    code: action
                }
            }
            Symbols::Anon(indices) => {
                let names: Vec<_> =
                    (0..indices.len()).map(|i| fresh_name(i, &action)).collect();
                let arg_patterns =
                    patterns(indices.iter().map(|&(index, _)| index)
                                           .zip(names.iter().cloned()),
                             symbols.len());
                let name_str = intern::read(|interner| {
                    let name_strs: Vec<_> = names.iter().map(|&n| interner.data(n)).collect();
                    name_strs.connect(", ")
                });
                let action = action.replace("~~", &name_str);
                r::ActionFnDefn {
                    arg_patterns: arg_patterns,
                    arg_types: arg_types,
                    ret_type: nt_type,
                    code: action
                }
            }
        };

        let index = r::ActionFn::new(self.grammar.action_fn_defns.len());
        self.grammar.action_fn_defns.push(action_fn_defn);

        index
    }

    fn symbols(&mut self, symbols: &[pt::Symbol]) -> Vec<r::Symbol> {
        symbols.iter().map(|sym| self.symbol(sym)).collect()
    }

    fn symbol(&mut self, symbol: &pt::Symbol) -> r::Symbol {
        match *symbol {
            pt::Symbol::Terminal(id) => r::Symbol::Terminal(id),
            pt::Symbol::Nonterminal(id) => r::Symbol::Nonterminal(id),
            pt::Symbol::Choose(ref s) | pt::Symbol::Name(_, ref s) => self.symbol(s),

            pt::Symbol::Macro(..) | pt::Symbol::Repeat(..) | pt::Symbol::Expr(..) => {
                unreachable!("symbol `{}` should have been normalized away by now", symbol)
            }
        }
    }
}

fn patterns<I>(mut chosen: I, num_args: usize) -> Vec<InternedString>
    where I: Iterator<Item=(usize, InternedString)>
{
    let blank = intern("_");

    let mut next_chosen = chosen.next();

    let result =
        (0..num_args)
        .map(|index| {
            match next_chosen {
                Some((chosen_index, chosen_name)) if chosen_index == index => {
                    next_chosen = chosen.next();
                    chosen_name
                }
                _ => blank,
            }
        })
        .collect();

    debug_assert!(next_chosen.is_none());

    result
}

fn fresh_name(counter: usize, action_str: &str) -> InternedString {
    let mut name = format!("__{}", counter);

    // Check whether this string appears anywhere in the action. If
    // so, keep appending an underscore until it doesn't. :) Obviously
    // this is stricter than needed, since the action string might be
    // like `print("__1")`, in which case we'll detect a false
    // conflict (or it might contain a variable named `__1x`,
    // etc). But so what.
    while action_str.contains(&name) {
        name.push('_');
    }

    intern(&name)
}
