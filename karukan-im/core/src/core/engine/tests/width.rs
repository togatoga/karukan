//! Symbol style (which symbol a key types), character width (how wide it
//! comes out), and the space setting.

use karukan_engine::{PunctuationStyle, SymbolStyle, Width, WidthRules};

use super::*;
use crate::config::settings::SpaceStyle;

/// Engine on the shipped config (`config/default.toml`), with live
/// conversion off so no model runs: the settings under test are the symbol,
/// width and space ones.
fn engine_on_shipped_defaults() -> InputMethodEngine {
    let settings = crate::config::Settings::default();
    InputMethodEngine::with_config(EngineConfig {
        live_conversion: false,
        ..EngineConfig::from_settings(&settings)
    })
}

/// Engine configured with `width`, no model loaded.
fn engine_with_width(width: WidthRules) -> InputMethodEngine {
    InputMethodEngine::with_config(EngineConfig {
        width,
        ..EngineConfig::default()
    })
}

/// The text of the last preedit the engine emitted — what the frontend
/// actually displays, which is where the width applies.
fn shown_preedit(result: &EngineResult) -> Option<String> {
    result.actions.iter().rev().find_map(|a| match a {
        EngineAction::UpdatePreedit(p) => Some(p.text().to_string()),
        _ => None,
    })
}

fn committed(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    })
}

fn shown_candidates(result: &EngineResult) -> Vec<String> {
    result
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            EngineAction::ShowCandidates(list) => Some(
                list.candidates()
                    .iter()
                    .map(|c| c.text.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// The annotation shown next to `text` in the emitted candidate list.
fn candidate_description(result: &EngineResult, text: &str) -> Option<String> {
    result.actions.iter().rev().find_map(|a| match a {
        EngineAction::ShowCandidates(list) => list
            .candidates()
            .iter()
            .find(|c| c.text == text)
            .and_then(|c| c.description.clone()),
        _ => None,
    })
}

fn type_keys(engine: &mut InputMethodEngine, keys: &str) -> EngineResult {
    let mut result = EngineResult::not_consumed();
    for ch in keys.chars() {
        result = engine.process_key(&press(ch));
    }
    result
}

#[test]
fn typed_symbols_come_out_at_the_configured_width() {
    // Both what a rule produced (？！) and what passed through (`(` has no
    // romaji rule) settle by the same rules.
    let mut engine = engine_with_width(WidthRules {
        ascii_symbol: Width::Half,
        ..WidthRules::default()
    });
    let result = type_keys(&mut engine, "a?b!");
    assert_eq!(shown_preedit(&result).as_deref(), Some("あ?b!"));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("あ?b!"));

    let mut engine = engine_with_width(WidthRules {
        ascii_symbol: Width::Full,
        ..WidthRules::default()
    });
    let result = type_keys(&mut engine, "(a)");
    assert_eq!(shown_preedit(&result).as_deref(), Some("（あ）"));
}

#[test]
fn the_kana_symbols_keep_the_style_that_typed_them() {
    // 。、「」・ are their own group, so 「記号は半角」 does not turn them
    // into ｡､｢｣･.
    let mut engine = engine_with_width(WidthRules {
        ascii_symbol: Width::Half,
        digit: Width::Half,
        ..WidthRules::default()
    });

    let result = type_keys(&mut engine, "a,b.");
    assert_eq!(shown_preedit(&result).as_deref(), Some("あ、b。"));
}

#[test]
fn model_output_is_settled_too() {
    // The model is prompted with NFKC and answers in half-width, so `!`
    // comes back half-width however it was typed. Its output settles on the
    // way in, so the setting still wins.
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        width: WidthRules {
            ascii_symbol: Width::Full,
            ..WidthRules::default()
        },
        live_conversion: true,
        ..EngineConfig::default()
    });
    engine.converters.kanji = None;

    type_keys(&mut engine, "a");
    set_live_text(&mut engine, "亜!");

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("亜！"));
}

#[test]
fn the_shipped_default_makes_kana_input_full_width() {
    // config/default.toml, end to end: typing in kana mode comes out
    // full-width, and the same keys in alphabet mode come out as typed.
    let mut engine = engine_on_shipped_defaults();

    let result = type_keys(&mut engine, "(a)");
    assert_eq!(shown_preedit(&result).as_deref(), Some("（あ）"));

    // Digits are the exception: nothing here remembers a width the user
    // picked, so a full-width default would be one they cannot take back.
    engine.process_key(&press_key(Keysym::ESCAPE));
    let result = type_keys(&mut engine, "123");
    assert_eq!(shown_preedit(&result).as_deref(), Some("123"));

    engine.process_key(&press_key(Keysym::ESCAPE));
    engine.process_key(&press_shift('A'));
    let result = type_keys(&mut engine, "bc, d.");
    assert_eq!(shown_preedit(&result).as_deref(), Some("Abc, d."));
}

#[test]
fn rewriter_variants_keep_their_own_width() {
    // The rewriter's width variants *are* the choice being offered, so
    // settling them would collapse `＠` and `@` into one entry.
    let mut engine = engine_with_width(WidthRules {
        ascii_symbol: Width::Half,
        ..WidthRules::default()
    });

    type_keys(&mut engine, "@");
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let candidates = shown_candidates(&result);
    assert!(
        candidates.iter().any(|c| c == "＠"),
        "the full-width variant must survive, got {candidates:?}"
    );
}

#[test]
fn folding_a_variant_into_its_twin_drops_the_duplicate() {
    // With full-width digits the rewriter's `１` is no longer a second
    // choice: it is the same text as the settled `1`. Dropping it must not
    // shift the numbering, since the window and the list Ctrl+digit indexes
    // are the same list.
    let mut engine = engine_with_width(WidthRules {
        digit: Width::Full,
        ..WidthRules::default()
    });

    type_keys(&mut engine, "1");
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let shown = shown_candidates(&result);
    assert_eq!(
        shown.iter().filter(|c| c.as_str() == "１").count(),
        1,
        "１ must appear once, got {shown:?}"
    );

    let second = shown.get(1).cloned().expect("at least two candidates");
    let result = engine.select_candidate_on_page(1);
    assert_eq!(committed(&result).as_deref(), Some(second.as_str()));
}

#[test]
fn dictionary_surfaces_keep_the_width_they_were_written_at() {
    // A dictionary surface is spelled the way its author spelled it —
    // `Yahoo!` carries a half-width `!` — and the width setting is about
    // what the IME produces, not about rewriting someone's entry.
    let mut engine = engine_with_width(WidthRules {
        ascii_symbol: Width::Full,
        digit: Width::Half,
        ..WidthRules::default()
    });
    engine.converters.kanji = None;
    engine.dicts.system = Some(dict_from_json(
        r#"[{"reading":"かぶ","candidates":[{"surface":"Yahoo!","score":1.0}]}]"#,
    ));
    engine.dicts.user = Some(dict_from_json(
        r#"[{"reading":"かぶ","candidates":[{"surface":"１２３号","score":1.0}]}]"#,
    ));

    type_keys(&mut engine, "kabu");
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let candidates = shown_candidates(&result);
    for expected in ["Yahoo!", "１２３号"] {
        assert!(
            candidates.iter().any(|c| c == expected),
            "{expected} should be left as written, got {candidates:?}"
        );
    }
}

#[test]
fn an_emoji_query_is_never_answered_with_a_width_variant() {
    // `:smile` is a query, not text to offer a width for: the picker shows
    // emojis, and Enter commits one.
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;

    type_keys(&mut engine, ":smile");
    let candidates = shown_candidates(&engine.process_key(&press('e')));
    assert!(
        !candidates.iter().any(|c| c.contains('：')),
        "no width variant of the query, got {candidates:?}"
    );
}

#[test]
fn the_symbol_style_picks_the_mark_and_the_width_folds_it() {
    let style = SymbolStyle {
        punctuation: PunctuationStyle::CommaPeriod,
        ..SymbolStyle::default()
    };
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        symbol: style,
        ..EngineConfig::default()
    });
    let result = type_keys(&mut engine, "a,b.");
    assert_eq!(shown_preedit(&result).as_deref(), Some("あ，b．"));

    // The style still picks ，．; the width folds them to the ASCII pair.
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        symbol: style,
        width: WidthRules {
            ascii_symbol: Width::Half,
            ..WidthRules::default()
        },
        ..EngineConfig::default()
    });
    let result = type_keys(&mut engine, "a,b.");
    assert_eq!(shown_preedit(&result).as_deref(), Some("あ,b."));
}

#[test]
fn alphabet_input_always_takes_an_ascii_space() {
    // The setting describes kana input and stops at its edge, like the
    // width rules: direct input types what the key says.
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        space: SpaceStyle::Full,
        ..EngineConfig::default()
    });
    engine.process_key(&press_shift('A'));
    let result = engine.process_key(&press_key(Keysym::SPACE));

    assert_eq!(shown_preedit(&result).as_deref(), Some("A "));
}

#[test]
fn shift_space_alone_is_always_the_full_width_one() {
    for space in [SpaceStyle::Half, SpaceStyle::Full] {
        let mut engine = InputMethodEngine::with_config(EngineConfig {
            space,
            ..EngineConfig::default()
        });
        let result = engine.process_key(&press_shift_key(Keysym::SPACE));
        assert_eq!(committed(&result).as_deref(), Some("\u{3000}"));
    }
}

#[test]
fn picking_a_width_variant_commits_that_width() {
    // The candidate the user picked *is* the width they asked for. A second
    // pass on commit would fold `＜＞１２３４` back to `＜＞1234`.
    let mut engine = engine_on_shipped_defaults();
    engine.converters.kanji = None;

    type_keys(&mut engine, "<>1234");
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let shown = shown_candidates(&result);
    let index = shown
        .iter()
        .position(|c| c == "＜＞１２３４")
        .unwrap_or_else(|| panic!("no full-width variant in {shown:?}"));

    let result = engine.select_candidate_on_page(index);
    assert_eq!(committed(&result).as_deref(), Some("＜＞１２３４"));
}

#[test]
fn the_configured_form_and_both_widths_are_all_offered() {
    // Whatever the setting is, the user can reach the other forms from the
    // list: `<>1234` shows as `＜＞1234` under the default (symbols full,
    // digits half), with the all-full and all-half patterns alongside.
    let mut engine = engine_on_shipped_defaults();
    engine.converters.kanji = None;

    type_keys(&mut engine, "<>1234");
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let shown = shown_candidates(&result);
    for expected in ["＜＞1234", "＜＞１２３４", "<>1234"] {
        assert!(
            shown.iter().any(|c| c == expected),
            "{expected} missing from {shown:?}"
        );
    }
    // The annotation says which form each one is.
    assert_eq!(
        candidate_description(&result, "<>1234").as_deref(),
        Some("[半]記号")
    );
}

#[test]
fn a_settled_symbol_keeps_its_width_when_alphabet_input_starts() {
    // Shift+A switches the rest of the word to direct input, where the
    // width rules do not apply. What was typed before it is already
    // settled, and stays that way.
    let mut engine = engine_on_shipped_defaults();
    engine.converters.kanji = None;

    engine.process_key(&press('('));
    let result = engine.process_key(&press_shift('A'));
    assert_eq!(shown_preedit(&result).as_deref(), Some("（A"));

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("（A"));
}

#[test]
fn model_output_spaces_follow_the_space_setting() {
    // NFKC flattens `　` to ` ` in the prompt, so the model can only ever
    // answer with the half-width one. The setting decides what comes out.
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        space: SpaceStyle::Full,
        live_conversion: true,
        ..EngineConfig::default()
    });
    engine.converters.kanji = None;

    type_keys(&mut engine, "a");
    set_live_text(&mut engine, "亜 井");

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("亜　井"));
}
